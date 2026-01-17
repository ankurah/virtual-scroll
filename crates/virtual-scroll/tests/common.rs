//! Test utilities for ankurah-virtual-scroll integration tests
#![allow(unused_imports)]

use std::sync::Arc;

// Workaround for https://github.com/ankurah/ankurah/issues/211
use wasm_bindgen::prelude::*;

use ankurah::policy::DEFAULT_CONTEXT;
use ankurah::{Context, Model, Node, PermissiveAgent};
use ankurah_storage_sled::SledStorageEngine;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tracing::Level;

// Re-export useful types
pub use ankurah::core::selection::filter::Filterable;
pub use ankurah::core::value::Value;
pub use ankurah::error::MutationError;
pub use ankurah::model::View;
pub use ankurah::signals::Subscribe;
pub use ankurah::EntityId;
pub use ankurah_virtual_scroll::{ScrollManager, VisibleSet};

/// Test message model for scroll testing
#[derive(Model, Debug, Clone, Serialize, Deserialize)]
pub struct TestMessage {
    pub timestamp: i64,
    pub height: i32,
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

/// Create multiple test messages in a single transaction
pub async fn create_messages(
    ctx: &Context,
    messages: impl IntoIterator<Item = (i64, i32)>,
) -> Result<Vec<EntityId>, MutationError> {
    let trx = ctx.begin();
    let mut ids = Vec::new();
    for (timestamp, height) in messages {
        let msg = trx.create(&TestMessage { timestamp, height }).await?;
        ids.push(msg.id());
    }
    trx.commit().await?;
    Ok(ids)
}

/// Extract timestamps from a VisibleSet
pub fn timestamps<V: ankurah::model::View>(visible_set: &VisibleSet<V>) -> Vec<i64> {
    visible_set
        .items
        .iter()
        .filter_map(|item| {
            item.entity().value("timestamp").and_then(|v| match v {
                Value::I64(ts) => Some(ts),
                _ => None,
            })
        })
        .collect()
}

// ============================================================================
// MockRenderer
// ============================================================================

/// Simulates a UI renderer that consumes VisibleSet updates from ScrollManager.
///
/// Tracks scroll position, computes which items are visible in the viewport,
/// and notifies ScrollManager of scroll events. Handles two types of window changes:
/// - **Expansion**: Items added at the start of the window; scroll_offset increases to compensate
/// - **Sliding**: Window moves; scroll anchors to the intersection item
pub struct MockRenderer<V: View + Clone + Send + Sync + 'static> {
    sm: std::sync::Arc<ScrollManager<V>>,
    rx: mpsc::UnboundedReceiver<VisibleSet<V>>,
    _guard: ankurah_signals::SubscriptionGuard,
    pub scroll_offset: i32,
    content_height: i32,
    viewport_height: i32,
    item_heights: Vec<i32>,
    item_ids: Vec<EntityId>,
    item_timestamps: Vec<i64>,
    prev_item_count: usize,
}

impl<V: View + Clone + Send + Sync + 'static> MockRenderer<V> {
    /// Create a new MockRenderer subscribed to the ScrollManager's visible_set signal.
    pub fn new(sm: std::sync::Arc<ScrollManager<V>>, viewport_height: i32) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let guard = sm.visible_set().subscribe(move |vs: VisibleSet<V>| {
            let _ = tx.send(vs);
        });
        Self {
            sm,
            rx,
            _guard: guard,
            scroll_offset: 0,
            content_height: 0,
            viewport_height,
            item_heights: Vec::new(),
            item_ids: Vec::new(),
            item_timestamps: Vec::new(),
            prev_item_count: 0,
        }
    }

    /// Wait for the next VisibleSet update from ScrollManager.
    ///
    /// Caches item data (heights, ids, timestamps) and adjusts scroll_offset based on
    /// the intersection hint:
    /// - **Backward expansion**: Items added at START (older side); shift scroll to compensate
    /// - **Forward expansion**: Items added at END (newer side); no scroll adjustment needed
    /// - **Sliding**: Window moves; anchor scroll to the intersection item
    pub async fn next_render(&mut self) -> Result<VisibleSet<V>, MockRendererError> {
        let vs = self
            .rx
            .recv()
            .await
            .ok_or(MockRendererError("channel closed"))?;

        if let Some(ref err) = vs.error {
            return Err(MockRendererError(Box::leak(err.clone().into_boxed_str())));
        }

        // Cache item data
        self.prev_item_count = self.item_heights.len();
        self.item_heights = vs
            .items
            .iter()
            .filter_map(|item| {
                item.entity().value("height").and_then(|v| match v {
                    Value::I32(h) => Some(h),
                    _ => None,
                })
            })
            .collect();
        self.item_ids = vs.items.iter().map(|item| item.entity().id()).collect();
        self.item_timestamps = vs
            .items
            .iter()
            .filter_map(|item| {
                item.entity().value("timestamp").and_then(|v| match v {
                    Value::I64(ts) => Some(ts),
                    _ => None,
                })
            })
            .collect();
        self.content_height = self.item_heights.iter().sum();

        // Adjust scroll position based on intersection
        if vs.should_auto_scroll {
            self.scroll_offset = (self.content_height - self.viewport_height).max(0);
        } else if let Some(ref intersection) = vs.intersection {
            use ankurah_virtual_scroll::LoadDirection;
            match intersection.direction {
                LoadDirection::Forward => {
                    // Forward: anchor intersection at viewport top
                    let intersection_top: i32 =
                        self.item_heights[..intersection.index].iter().sum();
                    self.scroll_offset = intersection_top;
                }
                LoadDirection::Backward => {
                    // Backward: anchor intersection at viewport bottom
                    let intersection_bottom: i32 =
                        self.item_heights[..=intersection.index].iter().sum();
                    self.scroll_offset = (intersection_bottom - self.viewport_height).max(0);
                }
            }
        }
        Ok(vs)
    }

    /// Compute which item indices are currently visible in the viewport.
    ///
    /// - First visible: first item with pixels in viewport (bottom edge past viewport top)
    /// - Last visible: last item whose top edge is before the viewport bottom
    fn visible_indices(&self) -> (usize, usize) {
        // First visible: find first item whose bottom edge > scroll_offset
        // (item with bottom exactly at viewport top has 0 pixels visible)
        let mut bottom_edge = 0;
        let mut first_idx = 0;
        for (i, &height) in self.item_heights.iter().enumerate() {
            bottom_edge += height;
            if bottom_edge > self.scroll_offset {
                first_idx = i;
                break;
            }
        }

        // Last visible: find last item whose top edge < viewport_end
        let viewport_end = self.scroll_offset + self.viewport_height;
        let mut top_edge = 0;
        let mut last_idx = 0;
        for (i, &height) in self.item_heights.iter().enumerate() {
            if top_edge >= viewport_end {
                break;
            }
            last_idx = i;
            top_edge += height;
        }
        (first_idx, last_idx)
    }

    /// Get the total content height (sum of all item heights).
    pub fn content_height(&self) -> i32 {
        self.content_height
    }

    /// Get visible range info: (first_visible_ts, last_visible_ts, items_above, items_below).
    #[allow(dead_code)]
    pub fn visible_range(&self) -> (i64, i64, usize, usize) {
        let (first_idx, last_idx) = self.visible_indices();
        let total = self.item_heights.len();
        (
            self.item_timestamps.get(first_idx).copied().unwrap_or(0),
            self.item_timestamps.get(last_idx).copied().unwrap_or(0),
            first_idx,
            total.saturating_sub(last_idx + 1),
        )
    }

    /// Scroll up by `px` pixels and verify no window change is triggered.
    ///
    /// Asserts that the visible timestamps match expectations, notifies ScrollManager
    /// of the new visible range, and panics if an unexpected render arrives within 10ms.
    /// Use this for scrolls that stay within the buffer zone (items_above >= screen_items).
    pub async fn up_no_render(&mut self, px: i32, first_visible_ts: i64, last_visible_ts: i64) {
        self.scroll_offset = (self.scroll_offset - px).max(0);
        let (first_idx, last_idx) = self.visible_indices();
        let actual_first_ts = self.item_timestamps.get(first_idx).copied().unwrap_or(-1);
        let actual_last_ts = self.item_timestamps.get(last_idx).copied().unwrap_or(-1);
        assert_eq!(
            actual_first_ts, first_visible_ts,
            "first visible ts mismatch (no render)"
        );
        assert_eq!(
            actual_last_ts, last_visible_ts,
            "last visible ts mismatch (no render)"
        );
        if let (Some(&first), Some(&last)) =
            (self.item_ids.get(first_idx), self.item_ids.get(last_idx))
        {
            self.sm.on_scroll(first, last, true);
        }
        match tokio::time::timeout(std::time::Duration::from_millis(10), self.rx.recv()).await {
            Ok(Some(_)) => panic!("unexpected render received"),
            Ok(None) => panic!("channel closed"),
            Err(_) => {} // timeout - good
        }
    }

    /// Scroll up by `px` pixels and verify a window change is triggered.
    ///
    /// Notifies ScrollManager of the scroll, waits up to 500ms for a render, then asserts:
    /// - Item count matches expected
    /// - Window contains expected timestamps
    /// - Intersection item matches expected timestamp
    /// - Pagination flags (has_more_preceding, has_more_following, should_auto_scroll) match
    /// - First/last visible timestamps match after scroll_offset adjustment
    /// - Final scroll_offset matches expected value
    /// - Current selection (query) matches expected (if provided)
    ///
    /// Use this when scrolling triggers a render (mode change or pagination).
    /// Pass `expected_selection: None` for mode-change-only renders where selection doesn't change.
    #[allow(clippy::too_many_arguments)]
    pub async fn scroll_up_and_expect(
        &mut self,
        px: i32,
        items: usize,
        expected_ts: std::ops::RangeInclusive<i64>,
        intersection_ts: Option<i64>,
        has_more_preceding: bool,
        has_more_following: bool,
        should_auto_scroll: bool,
        first_visible_ts: i64,
        last_visible_ts: i64,
        expected_offset: i32,
        expected_selection: Option<&str>,
    ) -> Result<VisibleSet<V>, MockRendererError> {
        self.scroll_offset = (self.scroll_offset - px).max(0);
        let (first_idx, last_idx) = self.visible_indices();
        if let (Some(&first), Some(&last)) =
            (self.item_ids.get(first_idx), self.item_ids.get(last_idx))
        {
            self.sm.on_scroll(first, last, true);
        }
        // 500ms timeout - if render doesn't arrive, crash
        let vs =
            match tokio::time::timeout(std::time::Duration::from_millis(500), self.next_render())
                .await
            {
                Ok(result) => result?,
                Err(_) => panic!("expected render did not arrive within 500ms"),
            };

        assert_eq!(vs.items.len(), items, "items count mismatch");

        let ts = timestamps(&vs);
        let expected: Vec<i64> = expected_ts.collect();
        assert_eq!(ts, expected, "timestamps mismatch");

        let actual_int = vs
            .intersection
            .as_ref()
            .map(|i| ts.get(i.index).copied().unwrap_or(-1));
        assert_eq!(actual_int, intersection_ts, "intersection mismatch");

        assert_eq!(
            (vs.has_more_preceding, vs.has_more_following, vs.should_auto_scroll),
            (has_more_preceding, has_more_following, should_auto_scroll),
            "flags mismatch"
        );

        let (first_idx, last_idx) = self.visible_indices();
        let actual_first_ts = self.item_timestamps.get(first_idx).copied().unwrap_or(-1);
        let actual_last_ts = self.item_timestamps.get(last_idx).copied().unwrap_or(-1);
        assert_eq!(
            actual_first_ts, first_visible_ts,
            "first visible ts mismatch"
        );
        assert_eq!(actual_last_ts, last_visible_ts, "last visible ts mismatch");

        assert_eq!(
            self.scroll_offset, expected_offset,
            "scroll_offset mismatch"
        );

        if let Some(sel) = expected_selection {
            assert_eq!(
                self.sm.current_selection(),
                sel,
                "selection mismatch"
            );
        }

        Ok(vs)
    }

    /// Assert the state of a VisibleSet (typically from initial render).
    ///
    /// Verifies:
    /// - Item count matches expected
    /// - Window contains expected timestamps
    /// - Intersection item matches expected timestamp (None for initial render)
    /// - Pagination flags (has_more_preceding, has_more_following, should_auto_scroll) match
    /// - First/last visible timestamps match current scroll position
    #[allow(clippy::too_many_arguments)]
    pub fn assert(
        &self,
        vs: &VisibleSet<V>,
        items: usize,
        expected_ts: std::ops::RangeInclusive<i64>,
        intersection_ts: Option<i64>,
        has_more_preceding: bool,
        has_more_following: bool,
        should_auto_scroll: bool,
        first_visible_ts: i64,
        last_visible_ts: i64,
    ) {
        assert_eq!(vs.items.len(), items, "items count mismatch");

        let ts = timestamps(vs);
        let expected: Vec<i64> = expected_ts.collect();
        assert_eq!(ts, expected, "timestamps mismatch");

        let actual_int = vs
            .intersection
            .as_ref()
            .map(|i| ts.get(i.index).copied().unwrap_or(-1));
        assert_eq!(actual_int, intersection_ts, "intersection mismatch");

        assert_eq!(
            (vs.has_more_preceding, vs.has_more_following, vs.should_auto_scroll),
            (has_more_preceding, has_more_following, should_auto_scroll),
            "flags mismatch"
        );

        let (first_idx, last_idx) = self.visible_indices();
        let actual_first_ts = self.item_timestamps.get(first_idx).copied().unwrap_or(-1);
        let actual_last_ts = self.item_timestamps.get(last_idx).copied().unwrap_or(-1);
        assert_eq!(
            actual_first_ts, first_visible_ts,
            "first visible ts mismatch"
        );
        assert_eq!(actual_last_ts, last_visible_ts, "last visible ts mismatch");
    }

    /// Scroll down by `px` pixels and verify no window change is triggered.
    ///
    /// Asserts that the visible timestamps match expectations, notifies ScrollManager
    /// of the new visible range, and panics if an unexpected render arrives within 10ms.
    /// Use this for scrolls that stay within the buffer zone (items_below >= screen_items).
    pub async fn down_no_render(&mut self, px: i32, first_visible_ts: i64, last_visible_ts: i64) {
        self.scroll_offset = (self.scroll_offset + px).min(self.content_height - self.viewport_height);
        let (first_idx, last_idx) = self.visible_indices();
        let actual_first_ts = self.item_timestamps.get(first_idx).copied().unwrap_or(-1);
        let actual_last_ts = self.item_timestamps.get(last_idx).copied().unwrap_or(-1);
        assert_eq!(
            actual_first_ts, first_visible_ts,
            "first visible ts mismatch (no render)"
        );
        assert_eq!(
            actual_last_ts, last_visible_ts,
            "last visible ts mismatch (no render)"
        );
        if let (Some(&first), Some(&last)) =
            (self.item_ids.get(first_idx), self.item_ids.get(last_idx))
        {
            self.sm.on_scroll(first, last, false); // scrolling_backward = false
        }
        match tokio::time::timeout(std::time::Duration::from_millis(10), self.rx.recv()).await {
            Ok(Some(_)) => panic!("unexpected render received"),
            Ok(None) => panic!("channel closed"),
            Err(_) => {} // timeout - good
        }
    }

    /// Scroll down by `px` pixels and verify a window change is triggered.
    ///
    /// Notifies ScrollManager of the scroll, waits up to 100ms for a render, then asserts:
    /// - Item count matches expected
    /// - Window contains expected timestamps
    /// - Intersection item matches expected timestamp
    /// - Pagination flags (has_more_preceding, has_more_following, should_auto_scroll) match
    /// - First/last visible timestamps match after scroll_offset adjustment
    /// - Final scroll_offset matches expected value
    /// - Current selection (query) matches expected (if provided)
    ///
    /// Use this when scrolling triggers a render (mode change or pagination).
    /// Pass `expected_selection: None` for mode-change-only renders where selection doesn't change.
    #[allow(clippy::too_many_arguments)]
    pub async fn scroll_down_and_expect(
        &mut self,
        px: i32,
        items: usize,
        expected_ts: std::ops::RangeInclusive<i64>,
        intersection_ts: Option<i64>,
        has_more_preceding: bool,
        has_more_following: bool,
        should_auto_scroll: bool,
        first_visible_ts: i64,
        last_visible_ts: i64,
        expected_offset: i32,
        expected_selection: Option<&str>,
    ) -> Result<VisibleSet<V>, MockRendererError> {
        self.scroll_offset = (self.scroll_offset + px).min(self.content_height - self.viewport_height);
        let (first_idx, last_idx) = self.visible_indices();
        if let (Some(&first), Some(&last)) =
            (self.item_ids.get(first_idx), self.item_ids.get(last_idx))
        {
            self.sm.on_scroll(first, last, false); // scrolling_backward = false
        }
        // 500ms timeout - if render doesn't arrive, crash
        let vs =
            match tokio::time::timeout(std::time::Duration::from_millis(500), self.next_render())
                .await
            {
                Ok(result) => result?,
                Err(_) => panic!("expected render did not arrive within 500ms"),
            };

        assert_eq!(vs.items.len(), items, "items count mismatch");

        let ts = timestamps(&vs);
        let expected: Vec<i64> = expected_ts.collect();
        assert_eq!(ts, expected, "timestamps mismatch");

        let actual_int = vs
            .intersection
            .as_ref()
            .map(|i| ts.get(i.index).copied().unwrap_or(-1));
        assert_eq!(actual_int, intersection_ts, "intersection mismatch");

        assert_eq!(
            (vs.has_more_preceding, vs.has_more_following, vs.should_auto_scroll),
            (has_more_preceding, has_more_following, should_auto_scroll),
            "flags mismatch"
        );

        let (first_idx, last_idx) = self.visible_indices();
        let actual_first_ts = self.item_timestamps.get(first_idx).copied().unwrap_or(-1);
        let actual_last_ts = self.item_timestamps.get(last_idx).copied().unwrap_or(-1);
        assert_eq!(
            actual_first_ts, first_visible_ts,
            "first visible ts mismatch"
        );
        assert_eq!(actual_last_ts, last_visible_ts, "last visible ts mismatch");

        assert_eq!(
            self.scroll_offset, expected_offset,
            "scroll_offset mismatch"
        );

        if let Some(sel) = expected_selection {
            assert_eq!(
                self.sm.current_selection(),
                sel,
                "selection mismatch"
            );
        }

        Ok(vs)
    }
}

// ============================================================================
// Assertion helpers
// ============================================================================

#[derive(Debug)]
pub struct MockRendererError(pub &'static str);

impl std::fmt::Display for MockRendererError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for MockRendererError {}
