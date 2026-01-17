//! Variable height item tests for ScrollManager
//!
//! Tests with items of varying heights to verify:
//! - Trigger conditions work correctly (based on item count, not pixels)
//! - Intersection anchoring maintains visual stability with varying heights
//! - Visible range calculations handle varying item sizes

mod common;

use common::*;
use std::sync::Arc;

/// Test with variable height items.
/// Heights cycle: [50, 75, 100, 125, 150] for indices 0,1,2,3,4 (repeating).
/// Uses deterministic scroll_collect helpers to handle mode-change renders.
#[tokio::test]
async fn test_variable_heights() -> Result<(), anyhow::Error> {
    let ctx = durable_sled_setup().await?;
    // Heights cycle: [50, 75, 100, 125, 150] based on index % 5
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
    // Heights cycle [50,75,100,125,150], each 5 items = 500px
    // Total height = 6 cycles * 500px = 3000px
    // Auto-scroll to bottom: offset = 3000 - 500 = 2500
    let vs = r.next_render().await?;
    r.assert(&vs, 30, 1030..=1059, None, true, false, true, 1055, 1059);
    assert_eq!(r.scroll_offset, 2500);
    assert_eq!(sm.mode(), ankurah_virtual_scroll::ScrollMode::Live);

    // Scroll up using deterministic helpers - mode may change during scrolling
    let _ = r.scroll_up_collect(500).await;
    let _ = r.scroll_up_collect(500).await;
    let _ = r.scroll_up_collect(500).await;

    // Should be in Backward mode after scrolling up far enough
    assert_eq!(sm.mode(), ankurah_virtual_scroll::ScrollMode::Backward);

    // Verify pagination occurred
    let selection = sm.current_selection();
    assert!(selection.contains("ORDER BY timestamp DESC"),
        "Backward mode should order DESC: {}", selection);

    // More items should have been loaded
    assert!(r.item_ids.len() > 30, "Should have paginated to load more items");

    Ok(())
}
