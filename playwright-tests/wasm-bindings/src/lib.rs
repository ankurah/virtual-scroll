//! WASM bindings for ankurah-virtual-scroll Playwright tests
//!
//! This crate provides the Message model and MessageScrollManager for browser testing.
//! Uses IndexedDB for local storage (no server needed).

use std::{panic, sync::Arc};

use ankurah::core::context::Context;
use ankurah::{policy::DEFAULT_CONTEXT as c, Model, Node, PermissiveAgent, View};
pub use ankurah_storage_indexeddb_wasm::IndexedDBStorageEngine;
use lazy_static::lazy_static;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use tracing::error;
use wasm_bindgen::{prelude::wasm_bindgen, JsValue};

// Re-export the React hooks from ankurah-signals
pub use ankurah_signals::{react::*, JsValueMut, JsValueRead};

/// Test message model with timestamp for ordering and variable content for height testing
#[derive(Model, Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Message content (can be short or long for varied heights)
    pub text: String,
    /// Timestamp for ordering (i64 for consistency with tests)
    pub timestamp: i64,
    /// Room identifier for filtering
    pub room: String,
    /// Soft delete flag
    #[active_type(LWW)]
    pub deleted: bool,
}

// Generate MessageScrollManager with timestamp DESC ordering
ankurah_virtual_scroll::generate_scroll_manager!(
    Message,
    MessageView,
    MessageLiveQuery,
    timestamp_field = "timestamp"
);

lazy_static! {
    static ref NODE: OnceCell<Node<IndexedDBStorageEngine, PermissiveAgent>> = OnceCell::new();
    static ref NOTIFY: tokio::sync::Notify = tokio::sync::Notify::new();
}

#[wasm_bindgen(start)]
pub async fn start() -> Result<(), JsValue> {
    // Configure tracing
    tracing_wasm::set_as_global_default_with_config(
        tracing_wasm::WASMLayerConfigBuilder::new()
            .set_max_level(tracing::Level::INFO)
            .build(),
    );
    panic::set_hook(Box::new(console_error_panic_hook::hook));

    let storage_engine = IndexedDBStorageEngine::open("virtual_scroll_playwright_test")
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    let node = Node::new_durable(Arc::new(storage_engine), PermissiveAgent::new());

    // Create or get the system (local-only mode, no server)
    // Use get_or_create to handle case where system already exists from previous test run
    if let Err(e) = node.system.create().await {
        // If creation fails, try to get existing - the error might be "already exists"
        let err_str = e.to_string();
        if !err_str.contains("already exists") {
            return Err(JsValue::from_str(&err_str));
        }
        // System already exists, that's fine - continue
    }

    if NODE.set(node).is_err() {
        error!("Failed to set node");
    }
    NOTIFY.notify_waiters();

    Ok(())
}

pub fn get_node() -> Node<IndexedDBStorageEngine, PermissiveAgent> {
    NODE.get().expect("Node not initialized").clone()
}

#[wasm_bindgen]
pub fn ctx() -> Result<Context, JsValue> {
    get_node()
        .context(c)
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

#[wasm_bindgen]
pub async fn ready() -> Result<(), JsValue> {
    match NODE.get() {
        Some(_) => Ok(()),
        None => {
            NOTIFY.notified().await;
            Ok(())
        }
    }
}

/// Seed test data with messages for a room
///
/// Creates messages with varied heights when varied_heights is true:
/// - Short messages (1 line)
/// - Medium messages (2-3 lines)
/// - Long messages (5+ lines)
#[wasm_bindgen]
pub async fn seed_test_data(
    room: String,
    count: u32,
    start_timestamp: i64,
    varied_heights: bool,
) -> Result<(), JsValue> {
    let ctx = ctx()?;
    let trx = ctx.begin();

    for i in 0..count {
        let text = if varied_heights {
            match i % 5 {
                0 => format!("Short msg {}", i),
                1 => format!("Medium message with a bit more content for testing purposes. Message number {}", i),
                2 => format!(
                    "Long message that spans multiple lines. This is message {} in the sequence. \
                    It contains enough text to make the row significantly taller than others, \
                    which is important for testing scroll behavior with varied item heights.",
                    i
                ),
                3 => format!("Another short one {}", i),
                _ => format!(
                    "Very long message {} with lots of content. {} {} {}",
                    i,
                    "Lorem ipsum dolor sit amet, consectetur adipiscing elit.",
                    "Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.",
                    "Ut enim ad minim veniam, quis nostrud exercitation ullamco."
                ),
            }
        } else {
            format!("Message {}", i)
        };

        trx.create(&Message {
            text,
            timestamp: start_timestamp + i as i64,
            room: room.clone(),
            deleted: false,
        })
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    }

    trx.commit()
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?;

    Ok(())
}

/// Clear all messages (for test isolation)
///
/// Since Ankurah doesn't have a delete API yet, this uses soft-delete
/// by setting `deleted = true` on all messages.
#[wasm_bindgen]
pub async fn clear_all_messages() -> Result<(), JsValue> {
    use ankurah_signals::Peek;

    let ctx = ctx()?;

    // Query all messages (including already deleted ones)
    let query = ctx
        .query_wait::<MessageView>("true")
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?;

    let items: Vec<MessageView> = query.peek();

    // Soft-delete each message by setting deleted = true
    let trx = ctx.begin();
    for msg in items {
        let m = trx
            .edit::<Message>(msg.entity())
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        m.deleted().set(&true)?;
    }

    trx.commit()
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?;

    Ok(())
}
