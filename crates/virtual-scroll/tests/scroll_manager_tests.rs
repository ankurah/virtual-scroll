//! Integration tests for ScrollManager with real Ankurah queries

mod common;

use ankql::ast::{OrderByItem, OrderDirection, PathExpr};
use common::*;

/// Helper to create a timestamp DESC order
fn timestamp_desc() -> Vec<OrderByItem> {
    vec![OrderByItem {
        path: PathExpr::simple("timestamp"),
        direction: OrderDirection::Desc,
    }]
}

#[tokio::test]
async fn test_scroll_manager_creation() -> Result<(), anyhow::Error> {
    let ctx = durable_sled_setup().await?;

    // Create some messages
    create_message_sequence(&ctx, "room1", 10, 1000).await?;

    // Create scroll manager
    let mut manager: ScrollManager<MessageView> = ScrollManager::new(
        &ctx,
        "room = 'room1'",
        timestamp_desc(),
        600.0, // viewport height
    )?;
    manager.start().await;

    // Check initial state
    assert_eq!(manager.mode(), ScrollMode::Live);
    assert!(manager.visible_set().peek().should_auto_scroll);
    assert!(!manager.visible_set().peek().has_more_newer);

    // Check visible set has items
    let visible_set = manager.visible_set().peek();
    assert_eq!(visible_set.items.len(), 10);
    assert!(visible_set.intersection.is_none());

    // For chat-style rendering, items are reversed so oldest is at index 0
    // (renders at top of list, newest at bottom)
    let ts = timestamps(&visible_set);
    assert_eq!(ts[0], 1000); // oldest at top
    assert_eq!(ts[9], 1009); // newest at bottom

    Ok(())
}

#[tokio::test]
async fn test_backward_pagination() -> Result<(), anyhow::Error> {
    let ctx = durable_sled_setup().await?;

    // Create 100 messages to exceed the default query limit
    create_message_sequence(&ctx, "room1", 100, 1000).await?;

    // Create scroll manager with small limit config
    let config = ScrollConfig {
        estimated_row_height: 50.0,
        query_size_ratio: 1.0, // Only 12 items per query (600/50)
        ..Default::default()
    };

    let mut manager: ScrollManager<MessageView> = ScrollManager::with_config(
        &ctx,
        "room = 'room1'",
        timestamp_desc(),
        600.0,
        config,
    )?;
    manager.start().await;

    // Initial load should have ~12 items (limited by config)
    let initial_set = manager.visible_set().peek();
    assert!(initial_set.items.len() <= 20); // Should be limited

    // Simulate scrolling up near top to trigger backward pagination
    // on_scroll automatically picks anchor and triggers pagination
    let load_direction = manager.on_scroll(
        50.0,   // top_gap - near top
        750.0,  // bottom_gap - far from bottom
        true,   // scrolling_up
    ).await;

    assert!(load_direction.is_some());
    assert_eq!(load_direction.unwrap(), LoadDirection::Backward);

    // After loading, we should have an intersection
    let new_set = manager.visible_set().peek();
    assert!(new_set.intersection.is_some());

    // Mode should be Backward now
    assert_eq!(manager.mode(), ScrollMode::Backward);
    assert!(new_set.has_more_newer); // Not at live anymore

    Ok(())
}

#[tokio::test]
async fn test_jump_to_live() -> Result<(), anyhow::Error> {
    let ctx = durable_sled_setup().await?;

    create_message_sequence(&ctx, "room1", 50, 1000).await?;

    let config = ScrollConfig {
        estimated_row_height: 50.0,
        query_size_ratio: 1.0,
        ..Default::default()
    };

    let mut manager: ScrollManager<MessageView> = ScrollManager::with_config(
        &ctx,
        "room = 'room1'",
        timestamp_desc(),
        600.0,
        config,
    )?;
    manager.start().await;

    // Simulate backward pagination via on_scroll
    manager.on_scroll(50.0, 750.0, true).await;

    assert_eq!(manager.mode(), ScrollMode::Backward);

    // Jump to live
    manager.jump_to_live().await;

    assert_eq!(manager.mode(), ScrollMode::Live);

    let live_set = manager.visible_set().peek();
    assert!(live_set.should_auto_scroll);
    assert!(!live_set.has_more_newer);

    // Should have newest messages again (reversed for chat-style display)
    let ts = timestamps(&live_set);
    // For chat-style: oldest at index 0, newest at last index
    // Live mode shows most recent ~12 items (1038-1049 with limit 12)
    // After reversal: oldest of the batch at [0], newest at [last]
    let last = ts.len() - 1;
    assert_eq!(ts[last], 1049); // Newest at bottom

    Ok(())
}

#[tokio::test]
async fn test_filter_update() -> Result<(), anyhow::Error> {
    let ctx = durable_sled_setup().await?;

    // Create messages in two rooms
    create_messages(&ctx, vec![
        (1000, "Room1 Msg1", "room1"),
        (1001, "Room1 Msg2", "room1"),
        (1002, "Room2 Msg1", "room2"),
        (1003, "Room2 Msg2", "room2"),
    ]).await?;

    let mut manager: ScrollManager<MessageView> = ScrollManager::new(
        &ctx,
        "room = 'room1'",
        timestamp_desc(),
        600.0,
    )?;
    manager.start().await;

    let set1 = manager.visible_set().peek();
    assert_eq!(set1.items.len(), 2);

    // Update filter to room2
    manager.update_filter("room = 'room2'", true).await;

    let set2 = manager.visible_set().peek();
    assert_eq!(set2.items.len(), 2);

    // Check it's room2 messages
    let txt = texts(&set2);
    assert!(txt[0].contains("Room2"));

    Ok(())
}

#[tokio::test]
async fn test_visible_set_flags() -> Result<(), anyhow::Error> {
    let ctx = durable_sled_setup().await?;

    // Create exactly 5 messages (fewer than limit)
    create_message_sequence(&ctx, "room1", 5, 1000).await?;

    let config = ScrollConfig {
        estimated_row_height: 50.0,
        query_size_ratio: 1.0, // Limit ~12
        ..Default::default()
    };

    let mut manager: ScrollManager<MessageView> = ScrollManager::with_config(
        &ctx,
        "room = 'room1'",
        timestamp_desc(),
        600.0,
        config,
    )?;
    manager.start().await;

    let visible_set = manager.visible_set().peek();

    // Only 5 items returned, less than limit - should be at earliest
    assert!(!visible_set.has_more_older); // at earliest
    assert!(!visible_set.has_more_newer); // live mode
    assert!(visible_set.should_auto_scroll); // live mode

    Ok(())
}

#[tokio::test]
async fn test_intersection_item_found() -> Result<(), anyhow::Error> {
    let ctx = durable_sled_setup().await?;

    create_message_sequence(&ctx, "room1", 50, 1000).await?;

    let config = ScrollConfig {
        estimated_row_height: 50.0,
        query_size_ratio: 1.0,
        ..Default::default()
    };

    let mut manager: ScrollManager<MessageView> = ScrollManager::with_config(
        &ctx,
        "room = 'room1'",
        timestamp_desc(),
        600.0,
        config,
    )?;
    manager.start().await;

    // Trigger backward pagination via on_scroll
    // This will use the oldest item's timestamp as anchor
    let initial_set = manager.visible_set().peek();
    let oldest_timestamp = match initial_set.items.last() {
        Some(item) => match item.entity().value("timestamp") {
            Some(ankurah::core::value::Value::I64(ts)) => ts,
            _ => panic!("Should have timestamp"),
        },
        None => panic!("Should have items"),
    };

    manager.on_scroll(50.0, 750.0, true).await;

    let new_set = manager.visible_set().peek();

    // Should have found the intersection item (the anchor)
    assert!(new_set.intersection.is_some());
    let intersection = new_set.intersection.as_ref().unwrap();

    // Verify the intersection item has the correct timestamp (oldest from before)
    let intersection_item = &new_set.items[intersection.index];
    let item_ts = match intersection_item.entity().value("timestamp") {
        Some(ankurah::core::value::Value::I64(ts)) => ts,
        _ => panic!("Should have timestamp"),
    };

    assert_eq!(item_ts, oldest_timestamp);

    Ok(())
}

#[tokio::test]
async fn test_display_order_consistency() -> Result<(), anyhow::Error> {
    let ctx = durable_sled_setup().await?;

    // Create messages with specific timestamps
    create_messages(&ctx, vec![
        (1000, "Oldest", "room1"),
        (1005, "Middle", "room1"),
        (1010, "Newest", "room1"),
    ]).await?;

    // Test DESC ordering - items are reversed for chat-style display (oldest at top)
    let mut manager: ScrollManager<MessageView> = ScrollManager::new(
        &ctx,
        "room = 'room1'",
        timestamp_desc(),
        600.0,
    )?;
    manager.start().await;

    let set = manager.visible_set().peek();
    let ts = timestamps(&set);

    // DESC query results are reversed: oldest at index 0 (top), newest at last (bottom)
    assert_eq!(ts, vec![1000, 1005, 1010]);

    Ok(())
}

#[tokio::test]
async fn test_on_scroll_guards() -> Result<(), anyhow::Error> {
    let ctx = durable_sled_setup().await?;

    create_message_sequence(&ctx, "room1", 5, 1000).await?;

    let mut manager: ScrollManager<MessageView> = ScrollManager::new(
        &ctx,
        "room = 'room1'",
        timestamp_desc(),
        600.0,
    )?;
    manager.start().await;

    // With only 5 messages, we're at earliest boundary
    // Scrolling up near top should NOT trigger because at_earliest
    let load_direction = manager.on_scroll(
        50.0,   // top_gap - near top
        750.0,  // bottom_gap
        true,   // scrolling_up
    ).await;

    assert!(load_direction.is_none()); // Already at earliest

    // Scrolling down should NOT trigger because in live mode (at_latest)
    let load_direction = manager.on_scroll(
        750.0,  // top_gap
        50.0,   // bottom_gap - near bottom
        false,  // scrolling down
    ).await;

    assert!(load_direction.is_none()); // Already at latest (live mode)

    Ok(())
}
