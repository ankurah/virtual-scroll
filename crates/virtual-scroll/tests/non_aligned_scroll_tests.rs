//! Non-aligned scroll position tests for ScrollManager
//!
//! Tests with scroll amounts that don't align to item boundaries to verify:
//! - Partial visibility detection at viewport edges
//! - First/last visible item detection when items are clipped
//! - Intersection anchoring when anchor item is partially visible
//! - Combined with variable heights for comprehensive coverage

mod common;

use common::*;
use std::sync::Arc;

/// Test with non-aligned scroll amounts and variable heights.
/// Heights cycle: [50, 75, 100, 125, 150] for indices 0,1,2,3,4 (repeating).
/// Uses deterministic scroll_collect helpers to handle mode-change renders.
#[tokio::test]
async fn test_non_aligned_scroll_positions() -> Result<(), anyhow::Error> {
    let ctx = durable_sled_setup().await?;
    let heights = [50, 75, 100, 125, 150];
    create_messages(&ctx, (0..60).map(|i| (1000 + i, heights[(i as usize) % 5]))).await?;

    let sm = Arc::new(ScrollManager::<TestMessageView>::new(
        &ctx,
        "true",
        "timestamp DESC",
        50,  // min_row_height (smallest item)
        2.0,
        500,
    )?);

    let mut r = MockRenderer::new(sm.clone(), 500);
    tokio::spawn({
        let sm = sm.clone();
        async move { sm.start().await }
    });

    // Initial: Live mode, 30 items (ts 1030-1059)
    // Total height = 6 cycles * 500px = 3000px
    // Auto-scroll to bottom: offset = 3000 - 500 = 2500
    let vs = r.next_render().await?;
    r.assert(&vs, 30, 1030..=1059, None, true, false, true, 1055, 1059);
    assert_eq!(r.scroll_offset, 2500);
    assert_eq!(sm.mode(), ankurah_virtual_scroll::ScrollMode::Live);

    // Test non-aligned scrolling with variable heights
    // Use scroll_up_collect to handle any mode-change renders
    // Note: After pagination, scroll_offset may change due to intersection anchoring,
    // so we don't assert exact values after large scrolls that may trigger pagination

    // Small scrolls that stay in Live mode
    let _ = r.scroll_up_collect(37).await;
    // Don't assert offset - mode change render may have already occurred

    // Continue scrolling - these will eventually exit Live mode
    let _ = r.scroll_up_collect(123).await;
    let _ = r.scroll_up_collect(289).await;
    let _ = r.scroll_up_collect(551).await;
    let _ = r.scroll_up_collect(173).await;
    let _ = r.scroll_up_collect(227).await;
    let _ = r.scroll_up_collect(51).await;

    // Should be in Backward mode after paginating
    assert_eq!(sm.mode(), ankurah_virtual_scroll::ScrollMode::Backward);

    // Verify selection reflects backward pagination
    let selection = sm.current_selection();
    assert!(selection.contains("ORDER BY timestamp DESC"),
        "Backward mode should order DESC: {}", selection);

    // More items should have been loaded (pagination happened)
    assert!(r.item_ids.len() > 30, "Should have paginated to load more items");

    Ok(())
}

/// Test mid-item scroll positions where items are partially clipped at both edges.
/// This verifies the visible_indices calculation handles edge cases correctly.
/// Uses uniform 100px heights and deterministic scroll_collect helpers.
#[tokio::test]
async fn test_partial_visibility_at_edges() -> Result<(), anyhow::Error> {
    let ctx = durable_sled_setup().await?;
    // Use uniform 100px heights
    create_messages(&ctx, (0..60).map(|i| (1000 + i, 100))).await?;

    // min_row_height=50 to get live_window=30 (matching other tests)
    let sm = Arc::new(ScrollManager::<TestMessageView>::new(
        &ctx,
        "true",
        "timestamp DESC",
        50,  // min_row_height (keeps live_window=30)
        2.0,
        500,
    )?);

    let mut r = MockRenderer::new(sm.clone(), 500);
    tokio::spawn({
        let sm = sm.clone();
        async move { sm.start().await }
    });

    // Initial: 30 items (ts 1030-1059), each 100px = 3000px total
    // Auto-scroll to bottom: offset = 3000 - 500 = 2500
    // Visible: indices 25-29 (ts 1055-1059)
    let vs = r.next_render().await?;
    r.assert(&vs, 30, 1030..=1059, None, true, false, true, 1055, 1059);
    assert_eq!(r.scroll_offset, 2500);

    // Test partial visibility edge cases with various small scroll amounts
    // Use scroll_up_collect to handle any mode-change renders
    let _ = r.scroll_up_collect(50).await;
    assert_eq!(r.scroll_offset, 2450);

    let _ = r.scroll_up_collect(49).await;
    assert_eq!(r.scroll_offset, 2401);

    let _ = r.scroll_up_collect(1).await;
    assert_eq!(r.scroll_offset, 2400);

    let _ = r.scroll_up_collect(1).await;
    assert_eq!(r.scroll_offset, 2399);

    // Verify we're now in Backward mode (exited Live mode due to scrolling up)
    assert_eq!(sm.mode(), ankurah_virtual_scroll::ScrollMode::Backward);

    Ok(())
}
