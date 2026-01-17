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
/// Average height = 100px, so 60 items = 6000px total, 30 items = 3000px.
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
    // After reverse: index 0=ts1030(h=50), index 1=ts1031(h=75), ..., index 29=ts1059(h=150)
    // Heights cycle [50,75,100,125,150], each 5 items = 500px
    // Total height = 6 cycles * 500px = 3000px
    // Auto-scroll to bottom: offset = 3000 - 500 = 2500
    //
    // Index to cumulative height (bottom edge):
    // 0-4:   50, 125, 225, 350, 500
    // 5-9:   550, 625, 725, 850, 1000
    // 10-14: 1050, 1125, 1225, 1350, 1500
    // 15-19: 1550, 1625, 1725, 1850, 2000
    // 20-24: 2050, 2125, 2225, 2350, 2500
    // 25-29: 2550, 2625, 2725, 2850, 3000
    //
    // At offset 2500, viewport 500, visible area: [2500, 3000)
    // First visible: index 25 (bottom=2550 > 2500) -> ts 1055
    // Last visible: index 29 (top=2850 < 3000) -> ts 1059
    let vs = r.next_render().await?;
    r.assert(&vs, 30, 1030..=1059, None, true, false, true, 1055, 1059);
    assert_eq!(r.scroll_offset, 2500);

    // Scroll up 500px to offset 2000, visible area: [2000, 2500)
    // First visible: index 20 (bottom=2050 > 2000) -> ts 1050
    // Last visible: index 24 (top=2350 < 2500) -> ts 1054
    r.up_no_render(500, 1050, 1054).await;
    assert_eq!(r.scroll_offset, 2000);

    // Scroll up 500px to offset 1500, visible area: [1500, 2000)
    // First visible: index 15 (bottom=1550 > 1500) -> ts 1045
    // Last visible: index 19 (top=1850 < 2000) -> ts 1049
    r.up_no_render(500, 1045, 1049).await;
    assert_eq!(r.scroll_offset, 1500);

    // Scroll up 500px to offset 1000, visible area: [1000, 1500)
    // First visible: index 10 (bottom=1050 > 1000) -> ts 1040
    // Last visible: index 14 (top=1350 < 1500) -> ts 1044
    // items_above = 10, which equals screen, triggering backward pagination
    //
    // After pagination:
    // - Window expands to 40 items (ts 1020-1059)
    // - Intersection at ts 1044 (was newest visible), now at new index 24
    // - New cumulative for index 24 = 2500px
    // - Offset for intersection at viewport bottom = 2500 - 500 = 2000
    // - At offset 2000, visible: ts 1040-1044 (indices 20-24)
    r.scroll_up_and_expect(
        500, 40, 1020..=1059, Some(1044),
        true, true, false, 1040, 1044, 2000,
        Some("TRUE AND \"timestamp\" <= 1059 ORDER BY timestamp DESC LIMIT 41"),
    ).await?;

    assert_eq!(sm.mode(), ankurah_virtual_scroll::ScrollMode::Backward);
    Ok(())
}
