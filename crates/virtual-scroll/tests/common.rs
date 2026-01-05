//! Test utilities for virtual-scroll integration tests

use std::sync::Arc;

use ankurah::policy::DEFAULT_CONTEXT;
use ankurah::{Context, Model, Node, PermissiveAgent};
use ankurah_storage_sled::SledStorageEngine;
use serde::{Deserialize, Serialize};
use tracing::Level;

// Re-export useful types
pub use ankurah::core::selection::filter::Filterable;
pub use ankurah::core::value::Value;
pub use ankurah::error::MutationError;
pub use ankurah::model::View;
pub use ankurah::signals::Peek;
pub use ankurah::EntityId;
pub use virtual_scroll::{LoadDirection, ScrollConfig, ScrollManager, ScrollMode, VisibleSet};

/// Test message model with timestamp for ordering
#[derive(Model, Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub text: String,
    pub timestamp: i64,
    pub room: String,
    pub deleted: bool,
}

// Initialize tracing for tests
#[ctor::ctor]
fn init_tracing() {
    if let Ok(level) = std::env::var("LOG_LEVEL") {
        let level = level.parse::<Level>().unwrap_or(Level::INFO);
        let _ = tracing_subscriber::fmt()
            .with_max_level(level)
            .with_test_writer()
            .try_init();
    } else {
        let _ = tracing_subscriber::fmt()
            .with_max_level(Level::INFO)
            .with_test_writer()
            .try_init();
    }
}

/// Create a durable sled-backed context for testing
pub async fn durable_sled_setup() -> Result<Context, anyhow::Error> {
    let node = Node::new_durable(
        Arc::new(SledStorageEngine::new_test().unwrap()),
        PermissiveAgent::new(),
    );
    node.system.create().await?;
    Ok(node.context_async(DEFAULT_CONTEXT).await)
}

/// Create multiple messages in a single transaction
pub async fn create_messages(
    ctx: &Context,
    messages: impl IntoIterator<Item = (i64, &str, &str)>,
) -> Result<Vec<EntityId>, MutationError> {
    let trx = ctx.begin();
    let mut ids = Vec::new();
    for (timestamp, text, room) in messages {
        let msg = trx
            .create(&Message {
                text: text.to_string(),
                timestamp,
                room: room.to_string(),
                deleted: false,
            })
            .await?;
        ids.push(msg.id());
    }
    trx.commit().await?;
    Ok(ids)
}

/// Create a sequence of messages with auto-incrementing timestamps
pub async fn create_message_sequence(
    ctx: &Context,
    room: &str,
    count: usize,
    start_timestamp: i64,
) -> Result<Vec<EntityId>, MutationError> {
    let messages: Vec<_> = (0..count)
        .map(|i| {
            (
                start_timestamp + i as i64,
                format!("Message {}", i).leak() as &str,
                room,
            )
        })
        .collect();
    create_messages(ctx, messages).await
}

/// Extract timestamps from a VisibleSet of MessageViews
pub fn timestamps<V: ankurah::model::View>(visible_set: &VisibleSet<V>) -> Vec<i64> {
    visible_set
        .items
        .iter()
        .filter_map(|item| {
            item.entity()
                .value("timestamp")
                .and_then(|v| match v {
                    Value::I64(ts) => Some(ts),
                    _ => None,
                })
        })
        .collect()
}

/// Extract texts from a VisibleSet of MessageViews
pub fn texts<V: ankurah::model::View>(visible_set: &VisibleSet<V>) -> Vec<String> {
    visible_set
        .items
        .iter()
        .filter_map(|item| {
            item.entity()
                .value("text")
                .and_then(|v| match v {
                    Value::String(s) => Some(s),
                    _ => None,
                })
        })
        .collect()
}
