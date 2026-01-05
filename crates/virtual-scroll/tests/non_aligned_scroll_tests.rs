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
/// Scroll by odd amounts to test partial visibility detection.
///
/// Height layout for first 10 items (ts 1030-1039 after reverse):
/// Index 0 (ts1030): h=50,  cumulative bottom=50
/// Index 1 (ts1031): h=75,  cumulative bottom=125
/// Index 2 (ts1032): h=100, cumulative bottom=225
/// Index 3 (ts1033): h=125, cumulative bottom=350
/// Index 4 (ts1034): h=150, cumulative bottom=500
/// Index 5 (ts1035): h=50,  cumulative bottom=550
/// Index 6 (ts1036): h=75,  cumulative bottom=625
/// Index 7 (ts1037): h=100, cumulative bottom=725
/// Index 8 (ts1038): h=125, cumulative bottom=850
/// Index 9 (ts1039): h=150, cumulative bottom=1000
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

    // === Test 1: Scroll by odd amount (37px) ===
    // offset: 2500 → 2463
    // Visible area: [2463, 2963)
    // Need to find which items are visible
    //
    // Cumulative heights from bottom (indices 25-29 at end of 30-item window):
    // Index 25 (ts1055): h=50, bottom=2550, top=2500
    // Index 26 (ts1056): h=75, bottom=2625, top=2550
    // Index 27 (ts1057): h=100, bottom=2725, top=2625
    // Index 28 (ts1058): h=125, bottom=2850, top=2725
    // Index 29 (ts1059): h=150, bottom=3000, top=2850
    //
    // At offset 2463:
    // - First visible: index 24 (ts1054), bottom=2500 > 2463 ✓, top=2350
    // - Last visible: index 29 (ts1059), top=2850 < 2963 ✓
    r.up_no_render(37, 1054, 1059).await;
    assert_eq!(r.scroll_offset, 2463);

    // === Test 2: Scroll by another odd amount (123px) ===
    // offset: 2463 → 2340
    // Visible area: [2340, 2840)
    //
    // Index 23 (ts1053): h=125, bottom=2350, top=2225
    // Index 24 (ts1054): h=150, bottom=2500, top=2350
    // ...
    // Index 28 (ts1058): h=125, bottom=2850, top=2725
    //
    // At offset 2340:
    // - First visible: index 23 (ts1053), bottom=2350 > 2340 ✓
    // - Last visible: index 28 (ts1058), top=2725 < 2840 ✓
    //   (index 29's top=2850 >= 2840)
    r.up_no_render(123, 1053, 1058).await;
    assert_eq!(r.scroll_offset, 2340);

    // === Test 3: Scroll by 289px ===
    // offset: 2340 → 2051
    // Visible area: [2051, 2551)
    //
    // Index 20 (ts1050): h=50, bottom=2050, top=2000
    // Index 21 (ts1051): h=75, bottom=2125, top=2050
    // Index 22 (ts1052): h=100, bottom=2225, top=2125
    // Index 23 (ts1053): h=125, bottom=2350, top=2225
    // Index 24 (ts1054): h=150, bottom=2500, top=2350
    // Index 25 (ts1055): h=50, bottom=2550, top=2500
    // Index 26 (ts1056): h=75, bottom=2625, top=2550
    //
    // At offset 2051:
    // - First visible: index 21 (ts1051), bottom=2125 > 2051 ✓ (index 20's bottom=2050 ≤ 2051)
    // - Last visible: index 26 (ts1056), top=2550 < 2551 ✓
    //   (index 27's top=2625 >= 2551)
    r.up_no_render(289, 1051, 1056).await;
    assert_eq!(r.scroll_offset, 2051);

    // === Test 4: Scroll to trigger pagination with non-aligned position ===
    // Current: offset=2051, 30 items
    // Need to scroll up until items_above <= screen (10)
    // Trigger happens when first_visible_idx <= 10, i.e., offset < 1050 (index 10's bottom)
    //
    // Scroll up by 551px to offset=1500
    // Visible area: [1500, 2000)
    // First visible: index 15 (ts1045), bottom=1550 > 1500 ✓
    // Last visible: index 19 (ts1049), top=1850 < 2000 ✓
    r.up_no_render(551, 1045, 1049).await;
    assert_eq!(r.scroll_offset, 1500);

    // Scroll up by 173px to offset=1327
    // Visible area: [1327, 1827)
    // First visible: index 13 (ts1043), bottom=1350 > 1327 ✓
    // Last visible: index 18 (ts1048), top=1725 < 1827 ✓
    r.up_no_render(173, 1043, 1048).await;
    assert_eq!(r.scroll_offset, 1327);

    // Scroll up by 227px to offset=1100
    // Visible area: [1100, 1600)
    // First visible: index 11 (ts1041), bottom=1125 > 1100 ✓
    // Last visible: index 16 (ts1046), top=1550 < 1600 ✓
    // items_above = 11 > 10, no trigger yet
    r.up_no_render(227, 1041, 1046).await;
    assert_eq!(r.scroll_offset, 1100);

    // === Test 5: Scroll by odd amount (51px) to trigger backward pagination ===
    // offset: 1100 → 1049
    // Visible area: [1049, 1549)
    // First visible: index 10 (ts1040), bottom=1050 > 1049 ✓
    // Last visible: index 15 (ts1045), top=1500 < 1549 ✓
    // items_above = 10, which equals screen - trigger!
    //
    // After pagination: 40 items (ts 1020-1059)
    // Intersection at newest visible (ts1045), at new index 25
    // Cumulative bottom at index 25 = 2550 (5 cycles * 500 + 50)
    // New offset = 2550 - 500 = 2050 (anchor at viewport bottom)
    //
    // At offset 2050, viewport [2050, 2550):
    // - Index 20 (ts1040): bottom=2050, NOT > 2050, not visible
    // - Index 21 (ts1041): bottom=2125 > 2050 ✓, first visible
    // - Index 25 (ts1045): top=2500 < 2550 ✓, last visible
    r.scroll_up_and_expect(
        51, 40, 1020..=1059, Some(1045),
        true, true, false, 1041, 1045, 2050,
        "TRUE AND \"timestamp\" <= 1059 ORDER BY timestamp DESC LIMIT 41",
    ).await?;

    assert_eq!(sm.mode(), ankurah_virtual_scroll::ScrollMode::Backward);
    Ok(())
}

/// Test mid-item scroll positions where items are partially clipped at both edges.
/// This verifies the visible_indices calculation handles edge cases correctly.
/// Uses uniform 100px heights for easier calculation of boundary conditions.
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

    // Scroll to offset 2450 (50px up)
    // Visible area: [2450, 2950)
    // First visible: index 24 (bottom=2500 > 2450) -> ts 1054
    // Last visible: index 29 (top=2900 < 2950) -> ts 1059
    r.up_no_render(50, 1054, 1059).await;
    assert_eq!(r.scroll_offset, 2450);

    // Scroll to offset 2401 (49px up)
    // Visible area: [2401, 2901)
    // First visible: index 24 (bottom=2500 > 2401) -> ts 1054
    // Last visible: index 29 (top=2900 < 2901) -> ts 1059
    r.up_no_render(49, 1054, 1059).await;
    assert_eq!(r.scroll_offset, 2401);

    // Scroll to offset 2400 (1px up)
    // Visible area: [2400, 2900)
    // First visible: index 24 (bottom=2500 > 2400) -> ts 1054
    // Last visible: index 28 (top=2800 < 2900) -> ts 1058
    // Note: index 29's top=2900 is NOT < 2900, so NOT visible
    r.up_no_render(1, 1054, 1058).await;
    assert_eq!(r.scroll_offset, 2400);

    // Scroll to offset 2399 (1px up)
    // Visible area: [2399, 2899)
    // First visible: index 23 (bottom=2400 > 2399) -> ts 1053
    // Last visible: index 28 (top=2800 < 2899) -> ts 1058
    r.up_no_render(1, 1053, 1058).await;
    assert_eq!(r.scroll_offset, 2399);

    Ok(())
}
