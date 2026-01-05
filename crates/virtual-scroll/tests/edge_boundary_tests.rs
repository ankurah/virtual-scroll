//! Edge boundary tests for ScrollManager
//!
//! Tests datasets at or below live_window size to verify correct
//! handling of has_more_preceding/has_more_following flags.

mod common;

use common::*;
use std::sync::Arc;

/// Test edge boundaries: dataset smaller than live_window.
/// When dataset is smaller than live_window, has_more_preceding should be false from start.
#[tokio::test]
async fn test_edge_boundaries_smaller_than_live_window() -> Result<(), anyhow::Error> {
    let ctx = durable_sled_setup().await?;
    // 25 items, less than live_window (30)
    create_messages(&ctx, (0..25).map(|i| (1000 + i, 50))).await?;

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

    // Initial: Only 25 items, has_more_preceding=false (25 < 30)
    let vs = r.next_render().await?;
    r.assert(&vs, 25, 1000..=1024, None, false, false, true, 1015, 1024);
    // 25*50 = 1250, 1250 - 500 = 750
    assert_eq!(r.scroll_offset, 750);

    // Scroll to top - no pagination triggers since has_more_preceding=false
    r.up_no_render(350, 1008, 1017).await;
    r.up_no_render(400, 1000, 1009).await;
    assert_eq!(r.scroll_offset, 0);

    // Scroll back down - still no pagination
    r.down_no_render(400, 1008, 1017).await;
    r.down_no_render(350, 1015, 1024).await;
    assert_eq!(r.scroll_offset, 750);

    // Still in Live mode (never left)
    assert_eq!(sm.mode(), ankurah_virtual_scroll::ScrollMode::Live);
    Ok(())
}

/// Test with dataset smaller than live_window - no backward pagination possible.
#[tokio::test]
async fn test_edge_boundaries_small_dataset() -> Result<(), anyhow::Error> {
    let ctx = durable_sled_setup().await?;
    // Only 20 items, less than live_window (30)
    create_messages(&ctx, (0..20).map(|i| (1000 + i, 50))).await?;

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

    // Initial: Only 20 items, has_more_preceding=false (20 < 30)
    let vs = r.next_render().await?;
    r.assert(&vs, 20, 1000..=1019, None, false, false, true, 1010, 1019);
    assert_eq!(r.scroll_offset, 500); // 20*50 - 500 = 500

    // Scroll to top - no pagination triggers since has_more_preceding=false
    r.up_no_render(500, 1000, 1009).await;
    assert_eq!(r.scroll_offset, 0);

    // Scroll back down - still no pagination needed
    r.down_no_render(500, 1010, 1019).await;
    assert_eq!(r.scroll_offset, 500);

    // Still in Live mode (never left it)
    assert_eq!(sm.mode(), ankurah_virtual_scroll::ScrollMode::Live);
    Ok(())
}
