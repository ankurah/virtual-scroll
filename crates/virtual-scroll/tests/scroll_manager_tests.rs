//! ScrollManager integration tests
//!
//! Windowing parameters: 60 messages (ts 1000-1059), 50px height, 500px viewport (10 visible),
//! S=10 screen_items, B=20 buffer, live_window=30. Trigger at items_above/below <= 10.

mod common;

use common::*;
use std::sync::Arc;

/// Test full round-trip: Live → oldest edge → back to Live.
/// Window progression (backward): 30 → 40 → 50 → 50 (hits oldest)
/// Window progression (forward): 50 → 60 (hits live edge)
///
/// Note: Only one scroll_down_and_expect because we lazily advance the window on direction
/// reversal, giving a slightly larger buffer than normal. This is preferable to immediately
/// updating the selection/predicate on every direction change.
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

    // Cycle 1: 30 → 40 items
    r.up_no_render(400, 1042, 1051).await;
    r.scroll_up_and_expect(
        100, 40, 1020..=1059, Some(1049),
        true, true, false, 1040, 1049, 1000,
        "TRUE AND \"timestamp\" <= 1059 ORDER BY timestamp DESC LIMIT 41",
    ).await?;

    // Cycle 2: 40 → 50 items
    r.up_no_render(400, 1032, 1041).await;
    r.scroll_up_and_expect(
        100, 50, 1010..=1059, Some(1039),
        true, true, false, 1030, 1039, 1000,
        "TRUE AND \"timestamp\" <= 1059 ORDER BY timestamp DESC LIMIT 51",
    ).await?;

    // Cycle 3: 50 → 50 items (slide), hits oldest edge
    r.up_no_render(400, 1022, 1031).await;
    r.scroll_up_and_expect(
        100, 50, 1000..=1049, Some(1029),
        false, true, false, 1020, 1029, 1000,
        "TRUE AND \"timestamp\" <= 1049 ORDER BY timestamp DESC LIMIT 51",
    ).await?;

    // Scroll to top
    r.up_no_render(400, 1012, 1021).await;
    r.up_no_render(400, 1004, 1013).await;
    r.up_no_render(200, 1000, 1009).await;
    assert_eq!(r.scroll_offset, 0);

    // === PHASE 2: Scroll forward back to live edge ===

    r.down_no_render(400, 1008, 1017).await;
    r.down_no_render(400, 1016, 1025).await;
    r.down_no_render(400, 1024, 1033).await;
    r.down_no_render(250, 1029, 1038).await;

    // Forward pagination: 50 → 60 items, back to Live mode
    r.scroll_down_and_expect(
        50, 60, 1000..=1059, Some(1030),
        false, false, true, 1050, 1059, 2500,
        "TRUE AND \"timestamp\" >= 1000 ORDER BY timestamp ASC LIMIT 61",
    ).await?;

    assert_eq!(r.scroll_offset, 2500);
    assert_eq!(sm.mode(), ankurah_virtual_scroll::ScrollMode::Live);

    Ok(())
}

/// Test direction reversal: backward → forward (no trigger) → backward again.
/// Ensures the system handles direction changes without data loss or incorrect pagination.
#[tokio::test]
async fn test_direction_reversal() -> Result<(), anyhow::Error> {
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

    // Initial: Live mode, 30 items (1030-1059)
    let vs = r.next_render().await?;
    r.assert(&vs, 30, 1030..=1059, None, true, false, true, 1050, 1059);
    assert_eq!(r.scroll_offset, 1000);

    // Scroll backward: 30 → 40 items
    r.up_no_render(400, 1042, 1051).await;
    r.scroll_up_and_expect(
        100, 40, 1020..=1059, Some(1049),
        true, true, false, 1040, 1049, 1000,
        "TRUE AND \"timestamp\" <= 1059 ORDER BY timestamp DESC LIMIT 41",
    ).await?;

    // At offset=1000: items_below=10, already at forward trigger threshold
    // First scroll up to get buffer room for forward scroll test
    r.up_no_render(200, 1036, 1045).await; // offset=800, items_below=14

    // Scroll forward WITHOUT triggering (offset=800 → 900, items_below=12 > 10)
    r.down_no_render(100, 1038, 1047).await; // offset=900

    // Reverse: scroll backward again, continue to trigger 40 → 50
    r.up_no_render(300, 1032, 1041).await; // offset=600
    r.scroll_up_and_expect(
        100, 50, 1010..=1059, Some(1039),
        true, true, false, 1030, 1039, 1000,
        "TRUE AND \"timestamp\" <= 1059 ORDER BY timestamp DESC LIMIT 51",
    ).await?;

    // Verify we're still in Backward mode (scrolling toward older items)
    assert_eq!(sm.mode(), ankurah_virtual_scroll::ScrollMode::Backward);

    Ok(())
}
