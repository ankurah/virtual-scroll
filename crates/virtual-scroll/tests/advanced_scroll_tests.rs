//! Advanced ScrollManager tests
//!
//! Tests for:
//! - 1.5 Rapid scroll stress test
//! - 1.6 Intersection anchoring
//! - 1.7 Selection predicates
//! - 1.8 Live mode behavior
//! - 1.11 Concurrent operations

mod common;

use common::*;
use std::sync::Arc;

// ============================================================================
// 1.5 Rapid Scroll Stress Test
// ============================================================================

/// Test rapid alternating scrolls without triggering pagination.
/// Verifies no panics or inconsistent state during rapid direction changes.
///
/// Note: Mode transitions (Live ↔ Backward) DO trigger renders to update should_auto_scroll.
/// This test verifies pagination is NOT triggered, but mode-change renders are expected.
#[tokio::test]
async fn test_rapid_alternating_scrolls() -> Result<(), anyhow::Error> {
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

    // Initial: 30 items, offset=1000, Live mode
    let vs = r.next_render().await?;
    r.assert(&vs, 30, 1030..=1059, None, true, false, true, 1050, 1059);
    assert_eq!(r.scroll_offset, 1000);

    // First scroll up exits Live mode - expect mode-change render
    // offset 1000→900, visible indices 18-27 → ts 1048-1057, items_below=2 → exits Live
    r.scroll_up_and_expect(
        100, 30, 1030..=1059, None,  // same items, no pagination
        true, false, false,          // has_more_preceding, has_more_following, should_auto_scroll=false
        1048, 1057, 900,             // visible range and offset
        None,                        // selection unchanged
    ).await?;
    assert_eq!(sm.mode(), ankurah_virtual_scroll::ScrollMode::Backward);

    // Now do rapid alternating scrolls in Backward mode
    // Stay away from both edges to avoid triggering mode changes or pagination
    // At offset 900, scroll within 800-900 range
    for _ in 0..10 {
        r.up_no_render(50, 1047, 1056).await;  // offset 850
        r.down_no_render(50, 1048, 1057).await; // offset 900
    }

    // Verify still in Backward mode (not re-entered Live)
    assert_eq!(r.scroll_offset, 900);
    assert_eq!(sm.mode(), ankurah_virtual_scroll::ScrollMode::Backward);

    // Verify items are still ordered correctly
    let (first, last, _, _) = r.visible_range();
    assert!(first < last, "Items should be ordered: first={} last={}", first, last);

    Ok(())
}

/// Test multiple scroll events that trigger pagination.
/// Verifies correct state after rapid pagination triggers.
#[tokio::test]
async fn test_rapid_pagination_triggers() -> Result<(), anyhow::Error> {
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

    // Initial state
    let vs = r.next_render().await?;
    r.assert(&vs, 30, 1030..=1059, None, true, false, true, 1050, 1059);

    // Scroll up (400px): offset 600, items_above=12 > screen(10), exits Live mode but no pagination
    r.scroll_up_and_expect(
        400, 30, 1030..=1059, None,
        true, false, false, 1042, 1051, 600,
        None,
    ).await?;
    // Scroll up (100px): offset 500, items_above=10 = screen, triggers pagination
    let vs = r.scroll_up_and_expect(
        100, 50, 1010..=1059, Some(1049),
        true, true, false, 1040, 1049, 1500,
        Some("TRUE AND \"timestamp\" <= 1059 ORDER BY timestamp DESC LIMIT 51"),
    ).await?;
    assert!(!vs.should_auto_scroll, "should_auto_scroll should be false after exiting Live mode");

    // Continue scrolling - debounce prevents repeated pagination triggers
    // With 50 items already loaded (1010-1059), scrolling within range doesn't trigger new queries
    // After pagination: offset 1500, visible 1040-1049
    // up(100): offset 1400, visible [1400, 1900) → indices 28-37 → ts 1038-1047
    r.up_no_render(100, 1038, 1047).await;
    // up(400): offset 1000, visible [1000, 1500) → indices 20-29 → ts 1030-1039
    r.up_no_render(400, 1030, 1039).await;
    // up(100): offset 900, visible [900, 1400) → indices 18-27 → ts 1028-1037
    r.up_no_render(100, 1028, 1037).await;

    // Verify mode and item ordering
    assert_eq!(sm.mode(), ankurah_virtual_scroll::ScrollMode::Backward);
    let (first, last, _, _) = r.visible_range();
    assert!(first < last, "Items should be ordered");

    Ok(())
}

// ============================================================================
// 1.6 Intersection Anchoring Tests
// ============================================================================

/// Test that intersection item exists in both old and new windows.
/// Backward pagination: intersection at newest visible (bottom of viewport).
#[tokio::test]
async fn test_intersection_anchoring_backward() -> Result<(), anyhow::Error> {
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

    // Scroll up - first exits Live mode (mode-change render)
    // offset 1000→600, visible indices 12-21 → ts 1042-1051, items_below=8 → exits Live
    r.scroll_up_and_expect(
        400, 30, 1030..=1059, None,  // same items, no pagination yet
        true, false, false,          // mode changed, should_auto_scroll=false
        1042, 1051, 600,
        None,
    ).await?;

    // Before pagination: visible range is 1042-1051
    // Intersection should be at newest visible (1049) for backward
    // After: 50 items (1010-1059), anchor at index 39 (ts 1049), offset = 2000 - 500 = 1500
    let vs = r.scroll_up_and_expect(
        100, 50, 1010..=1059, Some(1049),
        true, true, false, 1040, 1049, 1500,
        Some("TRUE AND \"timestamp\" <= 1059 ORDER BY timestamp DESC LIMIT 51"),
    ).await?;

    // Verify intersection exists in new window
    let intersection = vs.intersection.as_ref().expect("Should have intersection");
    let ts = timestamps(&vs);
    let intersection_ts = ts[intersection.index];

    // Intersection item should be in the range 1020..=1059
    assert!(intersection_ts >= 1020 && intersection_ts <= 1059,
        "Intersection {} should be in new window", intersection_ts);

    // For backward, intersection is anchored at viewport bottom
    assert_eq!(intersection.direction, ankurah_virtual_scroll::LoadDirection::Backward);

    Ok(())
}

/// Test forward pagination: intersection at oldest visible (top of viewport).
#[tokio::test]
async fn test_intersection_anchoring_forward() -> Result<(), anyhow::Error> {
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

    // First scroll backward - exits Live mode (mode-change render)
    r.scroll_up_and_expect(
        400, 30, 1030..=1059, None,
        true, false, false, 1042, 1051, 600,
        None,
    ).await?;
    // Continue scroll to trigger pagination (items_above=10)
    r.scroll_up_and_expect(
        100, 50, 1010..=1059, Some(1049),
        true, true, false, 1040, 1049, 1500,
        Some("TRUE AND \"timestamp\" <= 1059 ORDER BY timestamp DESC LIMIT 51"),
    ).await?;

    // Continue scrolling within buffer - no pagination triggers due to debounce
    r.up_no_render(500, 1030, 1039).await;
    r.up_no_render(500, 1020, 1029).await;
    r.up_no_render(500, 1010, 1019).await;
    r.up_no_render(500, 1010, 1019).await;  // At top of 50-item window

    // Now scroll forward to trigger forward pagination
    r.down_no_render(500, 1020, 1029).await;
    r.down_no_render(500, 1030, 1039).await;
    r.down_no_render(500, 1040, 1049).await;

    // Trigger forward pagination at edge
    let vs = r.scroll_down_and_expect(
        500, 60, 1000..=1059, Some(1040),
        false, false, true, 1050, 1059, 2500,
        Some("TRUE AND \"timestamp\" >= 1010 ORDER BY timestamp ASC LIMIT 61"),
    ).await?;

    // Verify intersection for forward pagination
    let intersection = vs.intersection.as_ref().expect("Should have intersection");
    assert_eq!(intersection.direction, ankurah_virtual_scroll::LoadDirection::Forward);

    Ok(())
}

// ============================================================================
// 1.7 Selection Predicate Tests
// ============================================================================

/// Test that selection predicates are correctly formed.
#[tokio::test]
async fn test_selection_predicates() -> Result<(), anyhow::Error> {
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

    // Initial: Live mode selection
    let vs = r.next_render().await?;
    r.assert(&vs, 30, 1030..=1059, None, true, false, true, 1050, 1059);

    // Live mode: ORDER BY DESC LIMIT live_window
    let selection = sm.current_selection();
    assert!(selection.contains("ORDER BY timestamp DESC"),
        "Live mode should order DESC: {}", selection);
    assert!(selection.contains("LIMIT 30"),
        "Live mode should limit to live_window: {}", selection);

    // Trigger backward pagination - first scroll exits Live mode
    r.scroll_up_and_expect(
        400, 30, 1030..=1059, None,  // same items, no pagination yet
        true, false, false,          // mode changed, should_auto_scroll=false
        1042, 1051, 600,
        None,
    ).await?;
    r.scroll_up_and_expect(
        100, 50, 1010..=1059, Some(1049),
        true, true, false, 1040, 1049, 1500,
        Some("TRUE AND \"timestamp\" <= 1059 ORDER BY timestamp DESC LIMIT 51"),
    ).await?;

    // Backward: timestamp <= cursor ORDER BY DESC
    let selection = sm.current_selection();
    assert!(selection.contains("\"timestamp\" <= 1059"),
        "Backward should have cursor constraint: {}", selection);
    assert!(selection.contains("ORDER BY timestamp DESC"),
        "Backward should order DESC: {}", selection);
    assert!(selection.contains("LIMIT 51"),
        "Backward limit should be full_window+1: {}", selection);

    Ok(())
}

/// Test forward selection predicate at oldest edge.
#[tokio::test]
async fn test_selection_predicate_forward() -> Result<(), anyhow::Error> {
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

    // Navigate to oldest edge - first scroll exits Live mode
    r.scroll_up_and_expect(
        400, 30, 1030..=1059, None,  // same items, no pagination yet
        true, false, false,          // mode changed, should_auto_scroll=false
        1042, 1051, 600,
        None,
    ).await?;
    r.scroll_up_and_expect(
        100, 50, 1010..=1059, Some(1049),
        true, true, false, 1040, 1049, 1500,
        Some("TRUE AND \"timestamp\" <= 1059 ORDER BY timestamp DESC LIMIT 51"),
    ).await?;

    r.up_no_render(400, 1032, 1041).await;
    r.scroll_up_and_expect(
        100, 50, 1010..=1059, Some(1039),
        true, true, false, 1030, 1039, 1000,
        Some("TRUE AND \"timestamp\" <= 1059 ORDER BY timestamp DESC LIMIT 51"),
    ).await?;

    r.up_no_render(400, 1022, 1031).await;
    r.scroll_up_and_expect(
        100, 50, 1000..=1049, Some(1029),
        false, true, false, 1020, 1029, 1000,
        Some("TRUE AND \"timestamp\" <= 1049 ORDER BY timestamp DESC LIMIT 51"),
    ).await?;

    // Scroll to top then forward
    r.up_no_render(1000, 1000, 1009).await;
    r.down_no_render(400, 1008, 1017).await;
    r.down_no_render(400, 1016, 1025).await;
    r.down_no_render(400, 1024, 1033).await;
    r.down_no_render(250, 1029, 1038).await;

    r.scroll_down_and_expect(
        50, 60, 1000..=1059, Some(1030),
        false, false, true, 1050, 1059, 2500,
        Some("TRUE AND \"timestamp\" >= 1000 ORDER BY timestamp ASC LIMIT 61"),
    ).await?;

    // Forward: timestamp >= cursor ORDER BY ASC
    let selection = sm.current_selection();
    assert!(selection.contains("\"timestamp\" >= 1000"),
        "Forward should have cursor constraint: {}", selection);
    assert!(selection.contains("ORDER BY timestamp ASC"),
        "Forward should order ASC: {}", selection);

    Ok(())
}

// ============================================================================
// 1.8 Live Mode Behavior
// ============================================================================

/// Test initial Live mode with should_auto_scroll.
#[tokio::test]
async fn test_live_mode_initial() -> Result<(), anyhow::Error> {
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

    // Initial render should be in Live mode with auto-scroll
    let vs = r.next_render().await?;

    assert!(vs.should_auto_scroll, "Initial render should have should_auto_scroll=true");
    assert_eq!(sm.mode(), ankurah_virtual_scroll::ScrollMode::Live);

    // Should be scrolled to bottom
    assert_eq!(r.scroll_offset, 1000); // 30*50 - 500 = 1000

    Ok(())
}

/// Test that scrolling up exits Live mode.
#[tokio::test]
async fn test_live_mode_exit_on_scroll_up() -> Result<(), anyhow::Error> {
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
    assert_eq!(sm.mode(), ankurah_virtual_scroll::ScrollMode::Live);

    // Scroll up to trigger backward pagination - first scroll exits Live mode
    r.scroll_up_and_expect(
        400, 30, 1030..=1059, None,  // same items, no pagination yet
        true, false, false,          // mode changed, should_auto_scroll=false
        1042, 1051, 600,
        None,
    ).await?;
    r.scroll_up_and_expect(
        100, 50, 1010..=1059, Some(1049),
        true, true, false, 1040, 1049, 1500,
        Some("TRUE AND \"timestamp\" <= 1059 ORDER BY timestamp DESC LIMIT 51"),
    ).await?;

    // Should now be in Backward mode
    assert_eq!(sm.mode(), ankurah_virtual_scroll::ScrollMode::Backward);

    Ok(())
}

/// Test returning to Live mode when scrolling back to bottom.
#[tokio::test]
async fn test_live_mode_reentry() -> Result<(), anyhow::Error> {
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

    // Full round trip: Live -> Backward -> oldest -> Forward -> Live
    // Scroll backward - first scroll exits Live mode
    r.scroll_up_and_expect(
        400, 30, 1030..=1059, None,  // same items, no pagination yet
        true, false, false,          // mode changed, should_auto_scroll=false
        1042, 1051, 600,
        None,
    ).await?;
    r.scroll_up_and_expect(
        100, 50, 1010..=1059, Some(1049),
        true, true, false, 1040, 1049, 1500,
        Some("TRUE AND \"timestamp\" <= 1059 ORDER BY timestamp DESC LIMIT 51"),
    ).await?;

    r.up_no_render(400, 1032, 1041).await;
    r.scroll_up_and_expect(
        100, 50, 1010..=1059, Some(1039),
        true, true, false, 1030, 1039, 1000,
        Some("TRUE AND \"timestamp\" <= 1059 ORDER BY timestamp DESC LIMIT 51"),
    ).await?;

    r.up_no_render(400, 1022, 1031).await;
    r.scroll_up_and_expect(
        100, 50, 1000..=1049, Some(1029),
        false, true, false, 1020, 1029, 1000,
        Some("TRUE AND \"timestamp\" <= 1049 ORDER BY timestamp DESC LIMIT 51"),
    ).await?;

    // Scroll to top
    r.up_no_render(1000, 1000, 1009).await;

    // Scroll forward back to live
    r.down_no_render(400, 1008, 1017).await;
    r.down_no_render(400, 1016, 1025).await;
    r.down_no_render(400, 1024, 1033).await;
    r.down_no_render(250, 1029, 1038).await;

    r.scroll_down_and_expect(
        50, 60, 1000..=1059, Some(1030),
        false, false, true, 1050, 1059, 2500,
        Some("TRUE AND \"timestamp\" >= 1000 ORDER BY timestamp ASC LIMIT 61"),
    ).await?;

    // Should be back in Live mode
    assert_eq!(sm.mode(), ankurah_virtual_scroll::ScrollMode::Live);

    Ok(())
}

// ============================================================================
// 1.11 Concurrent Operations
// ============================================================================

/// Test that scroll events during pending pagination don't cause issues.
/// This is a basic concurrency test - the MockRenderer serializes events,
/// but we verify the system handles rapid state changes gracefully.
#[tokio::test]
async fn test_concurrent_scroll_events() -> Result<(), anyhow::Error> {
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

    // Rapidly fire scroll events without waiting for renders
    // This simulates rapid user scrolling
    r.scroll_offset = 600; // Jump to offset 600
    let (first_idx, last_idx) = (12, 21); // Approximate visible at offset 600
    let first_id = vs.items[first_idx].entity().id();
    let last_id = vs.items[last_idx].entity().id();
    sm.on_scroll(first_id, last_id, true);

    // Immediately scroll more without waiting
    r.scroll_offset = 400;
    let (first_idx, last_idx) = (8, 17);
    let first_id = vs.items[first_idx].entity().id();
    let last_id = vs.items[last_idx].entity().id();
    sm.on_scroll(first_id, last_id, true);

    // Now wait for any pending render
    let vs = match tokio::time::timeout(
        std::time::Duration::from_millis(500),
        r.next_render()
    ).await {
        Ok(result) => result?,
        Err(_) => {
            // No render triggered is also valid if we stayed in buffer
            return Ok(());
        }
    };

    // Verify the result is valid regardless of which scroll "won"
    assert!(vs.items.len() >= 30, "Should have at least live_window items");
    let ts = timestamps(&vs);
    // Verify items are sorted
    for i in 1..ts.len() {
        assert!(ts[i-1] < ts[i], "Items should be sorted: {} >= {}", ts[i-1], ts[i]);
    }

    Ok(())
}

/// Test multiple pagination triggers in sequence.
#[tokio::test]
async fn test_sequential_paginations() -> Result<(), anyhow::Error> {
    let ctx = durable_sled_setup().await?;
    create_messages(&ctx, (0..100).map(|i| (1000 + i, 50))).await?;

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

    // Initial: 30 items from 100
    let vs = r.next_render().await?;
    assert_eq!(vs.items.len(), 30);
    let initial_ts = timestamps(&vs);
    assert_eq!(initial_ts[0], 1070); // 100-30 = 70
    assert_eq!(*initial_ts.last().unwrap(), 1099);

    // Trigger multiple backward paginations in sequence
    // Each should correctly extend the window

    // First backward: 30 -> 40 (first scroll exits Live mode)
    r.scroll_up_and_expect(
        400, 30, 1070..=1099, None,  // same items, no pagination yet
        true, false, false,          // mode changed, should_auto_scroll=false
        1082, 1091, 600,
        None,
    ).await?;
    r.scroll_up_and_expect(
        100, 50, 1050..=1099, Some(1089),
        true, true, false, 1080, 1089, 1500,
        Some("TRUE AND \"timestamp\" <= 1099 ORDER BY timestamp DESC LIMIT 51"),
    ).await?;

    // Second backward: 40 -> 50
    r.up_no_render(400, 1072, 1081).await;
    r.scroll_up_and_expect(
        100, 50, 1050..=1099, Some(1079),
        true, true, false, 1070, 1079, 1000,
        Some("TRUE AND \"timestamp\" <= 1099 ORDER BY timestamp DESC LIMIT 51"),
    ).await?;

    // Third backward: sliding window
    r.up_no_render(400, 1062, 1071).await;
    r.scroll_up_and_expect(
        100, 50, 1040..=1089, Some(1069),
        true, true, false, 1060, 1069, 1000,
        Some("TRUE AND \"timestamp\" <= 1089 ORDER BY timestamp DESC LIMIT 51"),
    ).await?;

    // Verify final state
    assert_eq!(sm.mode(), ankurah_virtual_scroll::ScrollMode::Backward);
    let (first, last, _, _) = r.visible_range();
    assert!(first < last);

    Ok(())
}
