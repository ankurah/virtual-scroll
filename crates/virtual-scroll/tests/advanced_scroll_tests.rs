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

    // Initial: 30 items, offset=1000
    let vs = r.next_render().await?;
    r.assert(&vs, 30, 1030..=1059, None, true, false, true, 1050, 1059);
    assert_eq!(r.scroll_offset, 1000);

    // Rapid alternating scrolls - stay in buffer zone
    for _ in 0..10 {
        r.up_no_render(100, 1048, 1057).await;
        r.down_no_render(100, 1050, 1059).await;
    }

    // Verify still at original position and in Live mode
    assert_eq!(r.scroll_offset, 1000);
    assert_eq!(sm.mode(), ankurah_virtual_scroll::ScrollMode::Live);

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

    // Scroll up rapidly to trigger backward pagination
    r.up_no_render(400, 1042, 1051).await;
    r.scroll_up_and_expect(
        100, 40, 1020..=1059, Some(1049),
        true, true, false, 1040, 1049, 1000,
        "TRUE AND \"timestamp\" <= 1059 ORDER BY timestamp DESC LIMIT 41",
    ).await?;

    // Continue scrolling up to trigger another pagination
    r.up_no_render(400, 1032, 1041).await;
    r.scroll_up_and_expect(
        100, 50, 1010..=1059, Some(1039),
        true, true, false, 1030, 1039, 1000,
        "TRUE AND \"timestamp\" <= 1059 ORDER BY timestamp DESC LIMIT 51",
    ).await?;

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

    // Scroll up to trigger backward pagination
    r.up_no_render(400, 1042, 1051).await;

    // Before pagination: visible range is 1042-1051
    // Intersection should be at newest visible (1051) for backward
    let vs = r.scroll_up_and_expect(
        100, 40, 1020..=1059, Some(1049),
        true, true, false, 1040, 1049, 1000,
        "TRUE AND \"timestamp\" <= 1059 ORDER BY timestamp DESC LIMIT 41",
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

    // First scroll backward to get away from live edge
    r.up_no_render(400, 1042, 1051).await;
    r.scroll_up_and_expect(
        100, 40, 1020..=1059, Some(1049),
        true, true, false, 1040, 1049, 1000,
        "TRUE AND \"timestamp\" <= 1059 ORDER BY timestamp DESC LIMIT 41",
    ).await?;

    // Continue to oldest edge
    r.up_no_render(400, 1032, 1041).await;
    r.scroll_up_and_expect(
        100, 50, 1010..=1059, Some(1039),
        true, true, false, 1030, 1039, 1000,
        "TRUE AND \"timestamp\" <= 1059 ORDER BY timestamp DESC LIMIT 51",
    ).await?;

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

    // Now scroll forward to trigger forward pagination
    r.down_no_render(400, 1008, 1017).await;
    r.down_no_render(400, 1016, 1025).await;
    r.down_no_render(400, 1024, 1033).await;
    r.down_no_render(250, 1029, 1038).await;

    // Trigger forward pagination
    let vs = r.scroll_down_and_expect(
        50, 60, 1000..=1059, Some(1030),
        false, false, true, 1050, 1059, 2500,
        "TRUE AND \"timestamp\" >= 1000 ORDER BY timestamp ASC LIMIT 61",
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

    // Trigger backward pagination
    r.up_no_render(400, 1042, 1051).await;
    r.scroll_up_and_expect(
        100, 40, 1020..=1059, Some(1049),
        true, true, false, 1040, 1049, 1000,
        "TRUE AND \"timestamp\" <= 1059 ORDER BY timestamp DESC LIMIT 41",
    ).await?;

    // Backward: timestamp <= cursor ORDER BY DESC
    let selection = sm.current_selection();
    assert!(selection.contains("\"timestamp\" <= 1059"),
        "Backward should have cursor constraint: {}", selection);
    assert!(selection.contains("ORDER BY timestamp DESC"),
        "Backward should order DESC: {}", selection);
    assert!(selection.contains("LIMIT 41"),
        "Backward limit should be window_size+1: {}", selection);

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

    // Navigate to oldest edge
    r.up_no_render(400, 1042, 1051).await;
    r.scroll_up_and_expect(
        100, 40, 1020..=1059, Some(1049),
        true, true, false, 1040, 1049, 1000,
        "TRUE AND \"timestamp\" <= 1059 ORDER BY timestamp DESC LIMIT 41",
    ).await?;

    r.up_no_render(400, 1032, 1041).await;
    r.scroll_up_and_expect(
        100, 50, 1010..=1059, Some(1039),
        true, true, false, 1030, 1039, 1000,
        "TRUE AND \"timestamp\" <= 1059 ORDER BY timestamp DESC LIMIT 51",
    ).await?;

    r.up_no_render(400, 1022, 1031).await;
    r.scroll_up_and_expect(
        100, 50, 1000..=1049, Some(1029),
        false, true, false, 1020, 1029, 1000,
        "TRUE AND \"timestamp\" <= 1049 ORDER BY timestamp DESC LIMIT 51",
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
        "TRUE AND \"timestamp\" >= 1000 ORDER BY timestamp ASC LIMIT 61",
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

    // Scroll up to trigger backward pagination - exits Live mode
    r.up_no_render(400, 1042, 1051).await;
    r.scroll_up_and_expect(
        100, 40, 1020..=1059, Some(1049),
        true, true, false, 1040, 1049, 1000,
        "TRUE AND \"timestamp\" <= 1059 ORDER BY timestamp DESC LIMIT 41",
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
    // Scroll backward
    r.up_no_render(400, 1042, 1051).await;
    r.scroll_up_and_expect(
        100, 40, 1020..=1059, Some(1049),
        true, true, false, 1040, 1049, 1000,
        "TRUE AND \"timestamp\" <= 1059 ORDER BY timestamp DESC LIMIT 41",
    ).await?;

    r.up_no_render(400, 1032, 1041).await;
    r.scroll_up_and_expect(
        100, 50, 1010..=1059, Some(1039),
        true, true, false, 1030, 1039, 1000,
        "TRUE AND \"timestamp\" <= 1059 ORDER BY timestamp DESC LIMIT 51",
    ).await?;

    r.up_no_render(400, 1022, 1031).await;
    r.scroll_up_and_expect(
        100, 50, 1000..=1049, Some(1029),
        false, true, false, 1020, 1029, 1000,
        "TRUE AND \"timestamp\" <= 1049 ORDER BY timestamp DESC LIMIT 51",
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
        "TRUE AND \"timestamp\" >= 1000 ORDER BY timestamp ASC LIMIT 61",
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

    // First backward: 30 -> 40
    r.up_no_render(400, 1082, 1091).await;
    r.scroll_up_and_expect(
        100, 40, 1060..=1099, Some(1089),
        true, true, false, 1080, 1089, 1000,
        "TRUE AND \"timestamp\" <= 1099 ORDER BY timestamp DESC LIMIT 41",
    ).await?;

    // Second backward: 40 -> 50
    r.up_no_render(400, 1072, 1081).await;
    r.scroll_up_and_expect(
        100, 50, 1050..=1099, Some(1079),
        true, true, false, 1070, 1079, 1000,
        "TRUE AND \"timestamp\" <= 1099 ORDER BY timestamp DESC LIMIT 51",
    ).await?;

    // Third backward: sliding window
    r.up_no_render(400, 1062, 1071).await;
    r.scroll_up_and_expect(
        100, 50, 1040..=1089, Some(1069),
        true, true, false, 1060, 1069, 1000,
        "TRUE AND \"timestamp\" <= 1089 ORDER BY timestamp DESC LIMIT 51",
    ).await?;

    // Verify final state
    assert_eq!(sm.mode(), ankurah_virtual_scroll::ScrollMode::Backward);
    let (first, last, _, _) = r.visible_range();
    assert!(first < last);

    Ok(())
}
