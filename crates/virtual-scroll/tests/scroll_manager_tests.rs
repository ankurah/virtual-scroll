//! ScrollManager integration tests
//!
//! ## Standard Test Configuration
//!
//! Most tests use: 60 messages (ts 1000-1059), 50px height, 500px viewport (10 visible).
//! - S = 10 screen_items (viewport_height / min_row_height)
//! - B = 20 buffer (buffer_factor * S = 2 * 10)
//! - live_window = 30 ((2N + 1) * S where N = buffer_factor / 2 = 1)
//! - Trigger threshold = 10 items (items_above/below <= S)
//! - Limit = 50 (visible_span + 2*buffer = 10 + 40)
//!
//! ## Scroll Stability (Anchor vs Cursor)
//!
//! The ANCHOR is the visible edge item used for scroll stability:
//! - Backward pagination: anchor = newest_visible (bottom of visible area)
//! - Forward pagination: anchor = oldest_visible (top of visible area)
//!
//! The CURSOR is a different item used for the query boundary:
//! - Backward: cursor = (newest_visible + buffer).min(max) → query <= cursor_ts
//! - Forward: cursor = (oldest_visible - buffer).max(0) → query >= cursor_ts
//!
//! The frontend uses the anchor to maintain scroll position after pagination.
//!
//! ## Race Condition Fix
//!
//! When using ephemeral nodes with remote subscriptions, the subscription callback can fire
//! multiple times during a single pagination request. The fix uses `resultset.is_loaded()`
//! to detect when the full result is ready before consuming the pending slide state.
//!
//! Note: These tests use durable sled storage which processes results synchronously.

mod common;

use common::*;
use std::sync::Arc;

/// Test full round-trip: Live → oldest edge → back to Live.
///
/// Math trace for first backward pagination:
/// - Initial: 30 items (1030-1059), scroll_offset=1000 (auto-scroll to bottom)
/// - up_no_render(400): offset=600, visible=1042-1051
/// - scroll_up(100): offset=500, visible=1040-1049, items_above=10 → TRIGGER
/// - cursor_index = min(19+20, 29) = 29 → continuation at ts=1059
/// - Result: 50 items (1010-1059), intersection at index 49 (ts=1059)
/// - Backward anchor: scroll_offset = 2500-500 = 2000, visible=1050-1059
#[tokio::test]
async fn test_scroll_live_to_oldest_and_back() -> Result<(), anyhow::Error> {
    let ctx = durable_sled_setup().await?;
    create_messages(&ctx, (0..60).map(|i| (1000 + i, 50))).await?;

    let sm = Arc::new(ScrollManager::<TestMessageView>::new(
        &ctx,
        "true",
        "timestamp DESC",
        50,  // min_row_height
        2.0, // buffer_factor
        500, // viewport_height
    )?);

    let mut r = MockRenderer::new(sm.clone(), 500);
    tokio::spawn({
        let sm = sm.clone();
        async move { sm.start().await }
    });

    // Initial: Live mode, 30 items (1030-1059), auto-scrolled to bottom
    let vs = r.next_render().await?;
    r.assert(&vs, 30, 1030..=1059, None, true, false, true, 1050, 1059);
    assert_eq!(r.scroll_offset, 1000);

    // === PHASE 1: Scroll backward to oldest edge ===

    // Scroll up: offset 1000→600, visible 1042-1051
    r.up_no_render(400, 1042, 1051).await;

    // Trigger backward: offset 600→500, visible indices 10-19 → ts 1040-1049, items_above=10
    // For backward: anchor = newest_visible = ts 1049
    // After: 50 items, anchor at index 39 → visible indices 30-39, offset 1500
    r.scroll_up_and_expect(
        100, 50, 1010..=1059, Some(1049),
        true, true, false, 1040, 1049, 1500,
        "TRUE AND \"timestamp\" <= 1059 ORDER BY timestamp DESC LIMIT 51",
    ).await?;

    // Continue scrolling backward toward oldest edge
    // After first pagination: 50 items (1010-1059), offset=1500
    // offset 1500→1000: visible indices 20-29 → ts 1030-1039
    r.up_no_render(500, 1030, 1039).await;

    // offset 1000→500: visible 1020-1029, items_above=10 → TRIGGER
    // For backward: anchor = newest_visible = ts 1029
    // After: 50 items (1000-1049), anchor at index 29 → visible 20-29, offset 1000
    r.scroll_up_and_expect(
        500, 50, 1000..=1049, Some(1029),
        false, true, false, 1020, 1029, 1000,
        "TRUE AND \"timestamp\" <= 1049 ORDER BY timestamp DESC LIMIT 51",
    ).await?;

    // Scroll to top (50 items 1000-1049, starting at offset 1000)
    // offset 1000→500: visible 1010-1019
    r.up_no_render(500, 1010, 1019).await;
    // offset 500→0: visible 1000-1009
    r.up_no_render(500, 1000, 1009).await;
    assert_eq!(r.scroll_offset, 0);

    // === PHASE 2: Scroll forward back to live edge ===
    // From offset 0, scroll down through 50 items (1000-1049)

    // offset 0→500: visible 1010-1019
    r.down_no_render(500, 1010, 1019).await;
    // offset 500→1000: visible 1020-1029
    r.down_no_render(500, 1020, 1029).await;
    // offset 1000→1500: visible 1030-1039, items_below=10 → TRIGGER (exactly at threshold!)
    // cursor_index = (30 - 20).max(0) = 10 → ts = 1000+10 = 1010
    // Query: timestamp >= 1010 ORDER BY ASC LIMIT 51 → returns 1010-1059 = 50 items
    // Window slides forward, dropping items 1000-1009
    // Intersection at ts=1030, auto-scroll to bottom
    // 50 items * 50px = 2500px content, offset = 2500 - 500 = 2000
    r.scroll_down_and_expect(
        500, 50, 1010..=1059, Some(1030),
        true, false, true, 1050, 1059, 2000,
        "TRUE AND \"timestamp\" >= 1010 ORDER BY timestamp ASC LIMIT 51",
    ).await?;

    assert_eq!(sm.mode(), ankurah_virtual_scroll::ScrollMode::Live);

    Ok(())
}

/// Test scrolling to the absolute oldest item.
/// Verifies has_more_preceding becomes false when at the edge.
#[tokio::test]
async fn test_scroll_to_absolute_oldest() -> Result<(), anyhow::Error> {
    let ctx = durable_sled_setup().await?;
    create_messages(&ctx, (0..60).map(|i| (1000 + i, 50))).await?;

    let sm = Arc::new(ScrollManager::<TestMessageView>::new(
        &ctx,
        "true",
        "timestamp DESC",
        50,
        2.0,
        500,
    )?);

    let mut r = MockRenderer::new(sm.clone(), 500);
    tokio::spawn({
        let sm = sm.clone();
        async move { sm.start().await }
    });

    // Initial: 30 items (1030-1059)
    let vs = r.next_render().await?;
    r.assert(&vs, 30, 1030..=1059, None, true, false, true, 1050, 1059);

    // First backward pagination: 30 → 50 items
    // up_no_render(400): offset 600, visible ts 1042-1051
    // scroll_up(100): offset 500, visible indices 10-19 → ts 1040-1049
    // For backward: anchor = newest_visible = ts 1049
    // After: 50 items, anchor at index 39 → visible indices 30-39, offset 1500
    r.up_no_render(400, 1042, 1051).await;
    r.scroll_up_and_expect(
        100, 50, 1010..=1059, Some(1049),
        true, true, false, 1040, 1049, 1500,
        "TRUE AND \"timestamp\" <= 1059 ORDER BY timestamp DESC LIMIT 51",
    ).await?;

    // Continue backward until we hit the oldest edge
    // Starting at offset 1500, use 500px increments
    // offset 1500→1000: visible indices 20-29 → ts 1030-1039
    r.up_no_render(500, 1030, 1039).await;
    // offset 1000→500: visible indices 10-19 → ts 1020-1029, items_above=10 → TRIGGER
    // anchor = newest_visible = ts 1029
    // New set 1000-1049, anchor at index 29 → visible indices 20-29, offset 1000
    let vs = r.scroll_up_and_expect(
        500, 50, 1000..=1049, Some(1029),
        false, true, false, 1020, 1029, 1000,
        "TRUE AND \"timestamp\" <= 1049 ORDER BY timestamp DESC LIMIT 51",
    ).await?;

    // CRITICAL: has_more_preceding must be false
    assert!(!vs.has_more_preceding, "should have no more preceding items at oldest edge");

    Ok(())
}

/// Test with a small dataset that fits entirely within the live window.
#[tokio::test]
async fn test_small_dataset_no_pagination() -> Result<(), anyhow::Error> {
    let ctx = durable_sled_setup().await?;
    // Only 15 items - less than live_window of 30
    create_messages(&ctx, (0..15).map(|i| (1000 + i, 50))).await?;

    let sm = Arc::new(ScrollManager::<TestMessageView>::new(
        &ctx,
        "true",
        "timestamp DESC",
        50,
        2.0,
        500,
    )?);

    let mut r = MockRenderer::new(sm.clone(), 500);
    tokio::spawn({
        let sm = sm.clone();
        async move { sm.start().await }
    });

    // Initial: All 15 items (1000-1014)
    // Content: 750px, viewport: 500px, auto-scroll: offset = 250
    // Visible: indices 5-14 (ts 1005-1014)
    let vs = r.next_render().await?;
    r.assert(&vs, 15, 1000..=1014, None, false, false, true, 1005, 1014);

    // Try scrolling up - should NOT trigger pagination since has_more_preceding=false
    r.up_no_render(200, 1001, 1010).await;
    r.up_no_render(50, 1000, 1009).await;

    assert_eq!(r.scroll_offset, 0);
    assert_eq!(sm.mode(), ankurah_virtual_scroll::ScrollMode::Live);

    Ok(())
}

/// Test dataset with exactly one more than live window (31 items).
#[tokio::test]
async fn test_one_more_than_live_window() -> Result<(), anyhow::Error> {
    let ctx = durable_sled_setup().await?;
    // 31 items = live_window + 1
    create_messages(&ctx, (0..31).map(|i| (1000 + i, 50))).await?;

    let sm = Arc::new(ScrollManager::<TestMessageView>::new(
        &ctx,
        "true",
        "timestamp DESC",
        50,
        2.0,
        500,
    )?);

    let mut r = MockRenderer::new(sm.clone(), 500);
    tokio::spawn({
        let sm = sm.clone();
        async move { sm.start().await }
    });

    // Initial: 30 items (1001-1030)
    // Content: 1500px, viewport: 500px, offset: 1000
    // Visible: indices 20-29 (ts 1021-1030)
    let vs = r.next_render().await?;
    r.assert(&vs, 30, 1001..=1030, None, true, false, true, 1021, 1030);

    // Scroll backward to get the missing item
    // up_no_render(400): offset 600, visible indices 12-21 → ts 1013-1022
    // scroll_up(100): offset 500, visible indices 10-19 → ts 1011-1020
    // For backward: anchor = newest_visible = ts 1020
    // After: 31 items, anchor at index 20 → visible indices 11-20, offset 550
    r.up_no_render(400, 1013, 1022).await;
    let vs = r.scroll_up_and_expect(
        100, 31, 1000..=1030, Some(1020),
        false, true, false, 1011, 1020, 550,
        "TRUE AND \"timestamp\" <= 1030 ORDER BY timestamp DESC LIMIT 51",
    ).await?;

    // Now has_more_preceding should be false - we have all 31 items
    assert!(!vs.has_more_preceding);

    Ok(())
}

/// Test rapid successive scrolls (debounce behavior).
#[tokio::test]
async fn test_debounce_rapid_scrolls() -> Result<(), anyhow::Error> {
    let ctx = durable_sled_setup().await?;
    create_messages(&ctx, (0..60).map(|i| (1000 + i, 50))).await?;

    let sm = Arc::new(ScrollManager::<TestMessageView>::new(
        &ctx,
        "true",
        "timestamp DESC",
        50,
        2.0,
        500,
    )?);

    let mut r = MockRenderer::new(sm.clone(), 500);
    tokio::spawn({
        let sm = sm.clone();
        async move { sm.start().await }
    });

    let vs = r.next_render().await?;
    r.assert(&vs, 30, 1030..=1059, None, true, false, true, 1050, 1059);

    // First scroll triggers pagination
    // up_no_render(400): offset 600, visible indices 12-21 → ts 1042-1051
    // scroll_up(100): offset 500, visible indices 10-19 → ts 1040-1049
    // For backward: anchor = newest_visible = ts 1049
    // After: 50 items, anchor at index 39 → visible indices 30-39, offset 1500
    r.up_no_render(400, 1042, 1051).await;
    r.scroll_up_and_expect(
        100, 50, 1010..=1059, Some(1049),
        true, true, false, 1040, 1049, 1500,
        "TRUE AND \"timestamp\" <= 1059 ORDER BY timestamp DESC LIMIT 51",
    ).await?;

    // Small scrolls within buffer - should NOT trigger new pagination
    // At offset 1500, scrolling up 50px each time
    r.up_no_render(50, 1039, 1048).await;
    r.up_no_render(50, 1038, 1047).await;

    // Mode should still be Backward
    assert_eq!(sm.mode(), ankurah_virtual_scroll::ScrollMode::Backward);

    Ok(())
}

/// Test with variable item heights.
#[tokio::test]
async fn test_variable_item_heights() -> Result<(), anyhow::Error> {
    let ctx = durable_sled_setup().await?;
    let heights = [50, 75, 100];
    create_messages(
        &ctx,
        (0..60).map(|i| (1000 + i, heights[i as usize % 3])),
    ).await?;

    let sm = Arc::new(ScrollManager::<TestMessageView>::new(
        &ctx,
        "true",
        "timestamp DESC",
        50,
        2.0,
        500,
    )?);

    let mut r = MockRenderer::new(sm.clone(), 500);
    tokio::spawn({
        let sm = sm.clone();
        async move { sm.start().await }
    });

    let vs = r.next_render().await?;
    assert_eq!(vs.items.len(), 30);
    assert!(vs.has_more_preceding);

    // Verify content height with variable heights
    // Last 30 items (indices 30-59) have heights cycling 50,75,100
    let expected_content_height: i32 = (0..30).map(|i| heights[(30 + i) % 3]).sum();
    assert_eq!(r.content_height(), expected_content_height);

    Ok(())
}

/// Test empty dataset.
#[tokio::test]
async fn test_empty_dataset() -> Result<(), anyhow::Error> {
    let ctx = durable_sled_setup().await?;
    // No messages created

    let sm = Arc::new(ScrollManager::<TestMessageView>::new(
        &ctx,
        "true",
        "timestamp DESC",
        50,
        2.0,
        500,
    )?);

    let mut r = MockRenderer::new(sm.clone(), 500);
    tokio::spawn({
        let sm = sm.clone();
        async move { sm.start().await }
    });

    let vs = r.next_render().await?;
    assert_eq!(vs.items.len(), 0);
    // Empty set means no more items in either direction
    // Note: start() sets has_more_preceding = items.len() >= live_window = false
    assert!(!vs.has_more_following);
    assert_eq!(sm.mode(), ankurah_virtual_scroll::ScrollMode::Live);

    Ok(())
}

/// Test single item dataset.
#[tokio::test]
async fn test_single_item() -> Result<(), anyhow::Error> {
    let ctx = durable_sled_setup().await?;
    create_messages(&ctx, [(1000, 50)]).await?;

    let sm = Arc::new(ScrollManager::<TestMessageView>::new(
        &ctx,
        "true",
        "timestamp DESC",
        50,
        2.0,
        500,
    )?);

    let mut r = MockRenderer::new(sm.clone(), 500);
    tokio::spawn({
        let sm = sm.clone();
        async move { sm.start().await }
    });

    let vs = r.next_render().await?;
    assert_eq!(vs.items.len(), 1);
    assert!(!vs.has_more_preceding);
    assert!(!vs.has_more_following);
    assert!(vs.should_auto_scroll);

    Ok(())
}

/// Test ascending display order (oldest first).
#[tokio::test]
async fn test_ascending_order() -> Result<(), anyhow::Error> {
    let ctx = durable_sled_setup().await?;
    create_messages(&ctx, (0..60).map(|i| (1000 + i, 50))).await?;

    let sm = Arc::new(ScrollManager::<TestMessageView>::new(
        &ctx,
        "true",
        "timestamp ASC",  // Oldest first
        50,
        2.0,
        500,
    )?);

    let mut r = MockRenderer::new(sm.clone(), 500);
    tokio::spawn({
        let sm = sm.clone();
        async move { sm.start().await }
    });

    let vs = r.next_render().await?;
    let ts = timestamps(&vs);

    // In ASC order, items should be 1000, 1001, ..., 1029
    assert_eq!(ts.len(), 30);
    assert_eq!(*ts.first().unwrap(), 1000);
    assert_eq!(*ts.last().unwrap(), 1029);

    Ok(())
}

/// Test scroll behavior with large viewport.
#[tokio::test]
async fn test_large_viewport() -> Result<(), anyhow::Error> {
    let ctx = durable_sled_setup().await?;
    create_messages(&ctx, (0..60).map(|i| (1000 + i, 50))).await?;

    // 1000px viewport = 20 visible items
    // screen_items = 20, live_window = (2*1 + 1) * 20 = 60
    let sm = Arc::new(ScrollManager::<TestMessageView>::new(
        &ctx,
        "true",
        "timestamp DESC",
        50,
        2.0,
        1000,
    )?);

    let mut r = MockRenderer::new(sm.clone(), 1000);
    tokio::spawn({
        let sm = sm.clone();
        async move { sm.start().await }
    });

    // With live_window=60 and 60 items, we get all items
    let vs = r.next_render().await?;
    assert_eq!(vs.items.len(), 60);
    assert!(!vs.has_more_following);

    Ok(())
}

/// Test two items dataset.
#[tokio::test]
async fn test_two_items() -> Result<(), anyhow::Error> {
    let ctx = durable_sled_setup().await?;
    create_messages(&ctx, [(1000, 50), (1001, 50)]).await?;

    let sm = Arc::new(ScrollManager::<TestMessageView>::new(
        &ctx,
        "true",
        "timestamp DESC",
        50,
        2.0,
        500,
    )?);

    let mut r = MockRenderer::new(sm.clone(), 500);
    tokio::spawn({
        let sm = sm.clone();
        async move { sm.start().await }
    });

    let vs = r.next_render().await?;
    assert_eq!(vs.items.len(), 2);
    assert!(!vs.has_more_preceding);
    assert!(!vs.has_more_following);
    assert!(vs.should_auto_scroll);

    let ts = timestamps(&vs);
    assert_eq!(ts, vec![1000, 1001]);

    Ok(())
}

/// Test initial auto-scroll puts us at the bottom.
#[tokio::test]
async fn test_initial_auto_scroll() -> Result<(), anyhow::Error> {
    let ctx = durable_sled_setup().await?;
    create_messages(&ctx, (0..60).map(|i| (1000 + i, 50))).await?;

    let sm = Arc::new(ScrollManager::<TestMessageView>::new(
        &ctx,
        "true",
        "timestamp DESC",
        50,
        2.0,
        500,
    )?);

    let mut r = MockRenderer::new(sm.clone(), 500);
    tokio::spawn({
        let sm = sm.clone();
        async move { sm.start().await }
    });

    let vs = r.next_render().await?;

    assert!(vs.should_auto_scroll);

    let ts = timestamps(&vs);
    assert!(ts.contains(&1059)); // Newest
    assert!(ts.contains(&1030)); // Oldest in window

    // content_height = 30 * 50 = 1500, viewport = 500
    // scroll_offset = 1500 - 500 = 1000
    assert_eq!(r.scroll_offset, 1000);

    Ok(())
}

/// Test mode transitions through the lifecycle.
#[tokio::test]
async fn test_mode_transitions() -> Result<(), anyhow::Error> {
    let ctx = durable_sled_setup().await?;
    create_messages(&ctx, (0..60).map(|i| (1000 + i, 50))).await?;

    let sm = Arc::new(ScrollManager::<TestMessageView>::new(
        &ctx,
        "true",
        "timestamp DESC",
        50,
        2.0,
        500,
    )?);

    let mut r = MockRenderer::new(sm.clone(), 500);
    tokio::spawn({
        let sm = sm.clone();
        async move { sm.start().await }
    });

    // Initial: Live mode
    let _ = r.next_render().await?;
    assert_eq!(sm.mode(), ankurah_virtual_scroll::ScrollMode::Live);

    // Scroll backward → Backward mode
    // up_no_render(400): offset 600, visible indices 12-21 → ts 1042-1051
    // scroll_up(100): offset 500, visible indices 10-19 → ts 1040-1049
    // For backward: anchor = newest_visible = ts 1049
    // After: 50 items, anchor at index 39 → visible indices 30-39, offset 1500
    r.up_no_render(400, 1042, 1051).await;
    r.scroll_up_and_expect(
        100, 50, 1010..=1059, Some(1049),
        true, true, false, 1040, 1049, 1500,
        "TRUE AND \"timestamp\" <= 1059 ORDER BY timestamp DESC LIMIT 51",
    ).await?;
    assert_eq!(sm.mode(), ankurah_virtual_scroll::ScrollMode::Backward);

    Ok(())
}

// ============================================================================
// Additional Edge Case Tests
// ============================================================================

/// Test exactly live_window items (30) - boundary case.
#[tokio::test]
async fn test_exactly_live_window_items() -> Result<(), anyhow::Error> {
    let ctx = durable_sled_setup().await?;
    create_messages(&ctx, (0..30).map(|i| (1000 + i, 50))).await?;

    let sm = Arc::new(ScrollManager::<TestMessageView>::new(
        &ctx,
        "true",
        "timestamp DESC",
        50,
        2.0,
        500,
    )?);

    let mut r = MockRenderer::new(sm.clone(), 500);
    tokio::spawn({
        let sm = sm.clone();
        async move { sm.start().await }
    });

    // Should get all 30 items
    let vs = r.next_render().await?;
    assert_eq!(vs.items.len(), 30);

    // Verify timestamps span the full range
    let ts = timestamps(&vs);
    assert_eq!(*ts.first().unwrap(), 1000);
    assert_eq!(*ts.last().unwrap(), 1029);

    // Scroll through entire content - no pagination should trigger
    r.up_no_render(500, 1010, 1019).await;
    r.up_no_render(500, 1000, 1009).await;
    assert_eq!(r.scroll_offset, 0);

    Ok(())
}

/// Test has_more_following is false at newest edge (we start there in DESC).
#[tokio::test]
async fn test_at_newest_edge() -> Result<(), anyhow::Error> {
    let ctx = durable_sled_setup().await?;
    create_messages(&ctx, (0..60).map(|i| (1000 + i, 50))).await?;

    let sm = Arc::new(ScrollManager::<TestMessageView>::new(
        &ctx,
        "true",
        "timestamp DESC",
        50,
        2.0,
        500,
    )?);

    let mut r = MockRenderer::new(sm.clone(), 500);
    tokio::spawn({
        let sm = sm.clone();
        async move { sm.start().await }
    });

    // Initial: 30 newest items (1030-1059)
    let vs = r.next_render().await?;

    // CRITICAL: We're at the newest edge, has_more_following must be false
    assert!(!vs.has_more_following, "should have no more following items at newest edge");
    assert!(vs.has_more_preceding, "should have more preceding items (older)");

    // Verify we have the newest item
    let ts = timestamps(&vs);
    assert!(ts.contains(&1059), "should contain newest item");

    Ok(())
}

/// Test large scroll after small scrolls escapes debounce.
#[tokio::test]
async fn test_debounce_escape() -> Result<(), anyhow::Error> {
    let ctx = durable_sled_setup().await?;
    create_messages(&ctx, (0..60).map(|i| (1000 + i, 50))).await?;

    let sm = Arc::new(ScrollManager::<TestMessageView>::new(
        &ctx,
        "true",
        "timestamp DESC",
        50,
        2.0,
        500,
    )?);

    let mut r = MockRenderer::new(sm.clone(), 500);
    tokio::spawn({
        let sm = sm.clone();
        async move { sm.start().await }
    });

    let _ = r.next_render().await?;

    // First pagination
    r.up_no_render(400, 1042, 1051).await;
    r.scroll_up_and_expect(
        100, 50, 1010..=1059, Some(1049),
        true, true, false, 1040, 1049, 1500,
        "TRUE AND \"timestamp\" <= 1059 ORDER BY timestamp DESC LIMIT 51",
    ).await?;

    // Small scrolls that stay within debounce threshold
    r.up_no_render(50, 1039, 1048).await;  // offset 1450
    r.up_no_render(50, 1038, 1047).await;  // offset 1400
    r.up_no_render(50, 1037, 1046).await;  // offset 1350

    // Mode should still be Backward, no new pagination yet
    assert_eq!(sm.mode(), ankurah_virtual_scroll::ScrollMode::Backward);

    // Now a BIG scroll that should escape debounce and trigger pagination
    // offset 1350 → 500, visible indices 10-19 → ts 1020-1029, items_above=10 → TRIGGER
    r.scroll_up_and_expect(
        850, 50, 1000..=1049, Some(1029),
        false, true, false, 1020, 1029, 1000,
        "TRUE AND \"timestamp\" <= 1049 ORDER BY timestamp DESC LIMIT 51",
    ).await?;

    Ok(())
}


/// Test immediate pagination on first big scroll.
#[tokio::test]
async fn test_immediate_pagination_trigger() -> Result<(), anyhow::Error> {
    let ctx = durable_sled_setup().await?;
    create_messages(&ctx, (0..60).map(|i| (1000 + i, 50))).await?;

    let sm = Arc::new(ScrollManager::<TestMessageView>::new(
        &ctx,
        "true",
        "timestamp DESC",
        50,
        2.0,
        500,
    )?);

    let mut r = MockRenderer::new(sm.clone(), 500);
    tokio::spawn({
        let sm = sm.clone();
        async move { sm.start().await }
    });

    let _ = r.next_render().await?;

    // One big scroll that immediately triggers pagination (no warmup scrolls)
    // offset 1000→500, visible indices 10-19 → ts 1040-1049, items_above=10 → TRIGGER
    r.scroll_up_and_expect(
        500, 50, 1010..=1059, Some(1049),
        true, true, false, 1040, 1049, 1500,
        "TRUE AND \"timestamp\" <= 1059 ORDER BY timestamp DESC LIMIT 51",
    ).await?;

    Ok(())
}

/// Test exactly 50 items (full_window size) - boundary case.
#[tokio::test]
async fn test_exactly_full_window_items() -> Result<(), anyhow::Error> {
    let ctx = durable_sled_setup().await?;
    create_messages(&ctx, (0..50).map(|i| (1000 + i, 50))).await?;

    let sm = Arc::new(ScrollManager::<TestMessageView>::new(
        &ctx,
        "true",
        "timestamp DESC",
        50,
        2.0,
        500,
    )?);

    let mut r = MockRenderer::new(sm.clone(), 500);
    tokio::spawn({
        let sm = sm.clone();
        async move { sm.start().await }
    });

    // Initial: 30 items (1020-1049)
    let vs = r.next_render().await?;
    assert_eq!(vs.items.len(), 30);
    assert!(vs.has_more_preceding);

    // Scroll backward to get all 50 items
    r.up_no_render(400, 1032, 1041).await;
    let vs = r.scroll_up_and_expect(
        100, 50, 1000..=1049, Some(1039),
        false, true, false, 1030, 1039, 1500,
        "TRUE AND \"timestamp\" <= 1049 ORDER BY timestamp DESC LIMIT 51",
    ).await?;

    // Now we have all 50 items
    assert_eq!(vs.items.len(), 50);
    assert!(!vs.has_more_preceding);

    Ok(())
}

// ============================================================================
// TODO: Additional test coverage needed
// See: https://github.com/ankurah/ankurah/issues/XXX (create issue)
// ============================================================================
//
// 1. test_large_viewport_no_pagination
//    - When viewport is large enough that live_window encompasses all items
//    - Verify scrolling through content doesn't trigger spurious pagination
//    - Challenge: Calculating exact visible items with 1000px viewport
//
// 2. test_multiple_backward_paginations (3+ paginations)
//    - With 100+ items, require 3-4 backward paginations to reach oldest
//    - Verify each pagination maintains scroll stability
//    - Verify eventually reaches has_more_preceding = false
//    - Challenge: Tracing exact timestamp ranges through multiple window slides
//
// 3. test_forward_pagination_dedicated
//    - Start from oldest, scroll forward to newest (ASC order or after backward)
//    - Currently forward is only tested in test_scroll_live_to_oldest_and_back
//
// 4. test_real_time_insertion
//    - New items arriving while scrolling backward
//    - Verify scroll position stability when items are added
//
// 5. test_predicate_filtering
//    - Tests with non-trivial predicates (not just "true")
//    - Verify pagination works correctly with filtered datasets

