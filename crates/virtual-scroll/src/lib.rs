//! Virtual Scroll - Ankurah-integrated virtual scroll state machine

pub mod windowing;

use ankql::ast::{
    ComparisonOperator, Expr, Literal, OrderByItem, OrderDirection, PathExpr, Predicate, Selection,
};
use ankurah::changes::ChangeSet;
use ankurah::core::selection::filter::Filterable;
use ankurah::core::value::Value;
use ankurah::{model::View, Context, LiveQuery};
use ankurah_proto::EntityId;
use ankurah_signals::{Mut, Peek, Read, Subscribe};

// Re-export key types
pub use ankql::ast::{OrderByItem as OrderBy, Predicate as Filter};
pub use ankurah_proto::EntityId as Id;
pub use ankurah_signals;

// ============================================================================
// Core Types
// ============================================================================

/// The visible set of items exposed to the renderer
#[derive(Clone, Debug)]
pub struct VisibleSet<V> {
    /// Items in display_order (first item at index 0)
    pub items: Vec<V>,
    /// Anchor item for scroll stability when items change
    pub intersection: Option<Intersection>,
    /// True if there are items preceding the current window (earlier in display_order)
    pub has_more_preceding: bool,
    /// True if there are items following the current window (later in display_order)
    pub has_more_following: bool,
    /// True if renderer should auto-scroll to end when items change
    pub should_auto_scroll: bool,
    /// Error if intersection calculation failed (continuation item not found in result)
    pub error: Option<String>,
}

impl<V> Default for VisibleSet<V> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            intersection: None,
            has_more_preceding: true,
            has_more_following: false,
            should_auto_scroll: true,
            error: None,
        }
    }
}

/// Identifies an item that exists in both the old and new result sets
#[derive(Clone, Debug)]
pub struct Intersection {
    pub entity_id: EntityId,
    pub index: usize,
    pub direction: LoadDirection,
}

/// Direction for loading more items, relative to display_order.
///
/// The display_order is set on the ScrollManager constructor and can be any valid
/// ORDER BY clause (e.g., "timestamp DESC", "priority ASC, created_at DESC").
///
/// - `Backward`: Load items that appear earlier in display_order (preceding items)
/// - `Forward`: Load items that appear later in display_order (following items)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadDirection {
    /// Load items preceding current window in display_order
    Backward,
    /// Load items following current window in display_order
    Forward,
}

/// Pending window slide operation
#[derive(Clone, Debug)]
struct PendingSlide {
    /// Entity to anchor scroll position after slide
    continuation: EntityId,
    /// Expected result count (request limit+1 to detect has_more)
    limit: usize,
    /// Direction of the slide
    direction: LoadDirection,
    /// Whether ORDER BY is reversed (for forward slides)
    reversed_order: bool,
}

/// Current scroll mode
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollMode {
    Live,     // At newest, receiving real-time updates
    Backward, // User scrolled up, loading older items
    Forward,  // User scrolling back toward live
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Convert an Ankurah Value to an AnkQL Literal for predicate construction
fn value_to_literal(value: &Value) -> Literal {
    match value {
        Value::I16(v) => Literal::I16(*v),
        Value::I32(v) => Literal::I32(*v),
        Value::I64(v) => Literal::I64(*v),
        Value::F64(v) => Literal::F64(*v),
        Value::Bool(v) => Literal::Bool(*v),
        Value::String(v) => Literal::String(v.clone()),
        // For other types, convert to string representation
        _ => Literal::String(format!("{:?}", value)),
    }
}

// ============================================================================
// ScrollManager
// ============================================================================

/// Virtual scroll manager with Ankurah LiveQuery integration
pub struct ScrollManager<V: View + Clone + Send + Sync + 'static> {
    livequery: LiveQuery<V>,
    predicate: Predicate,
    display_order: Vec<OrderByItem>,
    visible_set: Mut<VisibleSet<V>>,
    mode: Mut<ScrollMode>,
    /// Pending slide operation (set before query, consumed in callback)
    pending: Mut<Option<PendingSlide>>,
    minimum_row_height: u32,
    buffer_factor: f64,
    viewport_height: u32,
    _subscription: ankurah_signals::SubscriptionGuard,
}

impl<V: View + Clone + Send + Sync + 'static> ScrollManager<V> {
    /// Create a new scroll manager
    ///
    /// # Arguments
    /// * `ctx` - Ankurah context
    /// * `predicate` - Filter predicate (e.g., `"room_id = 'abc'"`)
    /// * `display_order` - Visual order (e.g., `"timestamp DESC"` for chat)
    /// * `minimum_row_height` - Guaranteed minimum item height in pixels
    /// * `buffer_factor` - Buffer as multiple of viewport (2.0 = 2x viewport buffer)
    /// * `viewport_height` - Viewport height in pixels
    pub fn new(
        ctx: &Context,
        predicate: impl TryInto<Predicate, Error = impl std::fmt::Debug>,
        display_order: impl IntoOrderBy,
        minimum_row_height: u32,
        buffer_factor: f64,
        viewport_height: u32,
    ) -> Result<Self, ankurah::error::RetrievalError> {
        let predicate = predicate.try_into().expect("Failed to parse predicate");
        let display_order = display_order
            .into_order_by()
            .expect("Failed to parse order");
        let buffer_factor = buffer_factor.max(2.0);

        // Compute initial limit
        let screen_items = windowing::screen_items(viewport_height, minimum_row_height);
        let threshold = buffer_factor / 2.0;
        let limit = windowing::live_window_size(screen_items, threshold);

        // Create livequery with initial selection
        let selection = Selection {
            predicate: predicate.clone(),
            order_by: Some(display_order.clone()),
            limit: Some(limit as u64),
        };
        let livequery: LiveQuery<V> = ctx.query(selection)?;

        // Create signals
        let visible_set: Mut<VisibleSet<V>> = Mut::new(VisibleSet::default());
        let pending: Mut<Option<PendingSlide>> = Mut::new(None);
        let mode: Mut<ScrollMode> = Mut::new(ScrollMode::Live);

        // Determine if we need to reverse results for display
        let is_desc = display_order
            .first()
            .map(|o| o.direction == OrderDirection::Desc)
            .unwrap_or(false);

        // Subscribe to livequery changes (for updates after initialization)
        let visible_set_clone = visible_set.clone();
        let pending_clone = pending.clone();
        let mode_clone = mode.clone();
        let subscription = livequery.subscribe(move |changeset: ChangeSet<V>| {
            let current = visible_set_clone.peek();
            // Skip if not yet initialized (start() will handle initial set)
            if current.items.is_empty() && !changeset.resultset.peek().is_empty() {
                return;
            }
            let mut items: Vec<V> = changeset.resultset.peek();

            // Consume pending slide state
            let slide = pending_clone.peek();
            pending_clone.set(None);

            // Normally, DESC order needs reversal to get oldest-first display order
            // But if we used reversed order (ASC for forward), items are already oldest-first
            let used_reversed_order = slide.as_ref().map(|s| s.reversed_order).unwrap_or(false);
            if is_desc && !used_reversed_order {
                items.reverse();
            }

            // Process result based on pending slide direction
            let (has_more_preceding, has_more_following, intersection, error) = if let Some(ref slide) = slide {
                // Detect end of data: we requested limit+1, so len > limit means more exist
                let (has_more_preceding, has_more_following) = match slide.direction {
                    LoadDirection::Backward => {
                        let more_older = if items.len() > slide.limit {
                            items.remove(0); // Remove extra oldest item
                            true
                        } else {
                            false
                        };
                        (more_older, true) // Backward slide means we left live edge
                    }
                    LoadDirection::Forward => {
                        let more_newer = if items.len() > slide.limit {
                            items.pop(); // Remove extra newest item
                            true
                        } else {
                            // Reached live edge - transition back to Live mode
                            mode_clone.set(ScrollMode::Live);
                            false
                        };
                        // Detect if we left items behind
                        let more_older = current.has_more_preceding ||
                            current.items.first().map(|old| items.first().map(|new|
                                old.entity().id() != new.entity().id()
                            ).unwrap_or(false)).unwrap_or(false);
                        (more_older, more_newer)
                    }
                };

                // Find intersection item for scroll anchoring
                let (intersection, error) = match items.iter().position(|item| item.entity().id() == slide.continuation) {
                    Some(index) => (
                        Some(Intersection {
                            entity_id: slide.continuation,
                            index,
                            direction: slide.direction,
                        }),
                        None
                    ),
                    None => {
                        if slide.direction == LoadDirection::Forward {
                            tracing::debug!("Forward slide: no overlap, jumping to live");
                            (None, None)
                        } else {
                            (None, Some(format!(
                                "Intersection failed: {} not found in result",
                                slide.continuation
                            )))
                        }
                    }
                };

                (has_more_preceding, has_more_following, intersection, error)
            } else {
                (current.has_more_preceding, current.has_more_following, None, None)
            };

            visible_set_clone.set(VisibleSet {
                items,
                intersection,
                has_more_preceding,
                has_more_following,
                should_auto_scroll: mode_clone.peek() == ScrollMode::Live,
                error,
            });
        });

        Ok(Self {
            livequery,
            predicate,
            display_order,
            visible_set,
            mode,
            pending,
            minimum_row_height,
            buffer_factor,
            viewport_height,
            _subscription: subscription,
        })
    }

    /// Initialize the scroll manager (waits for initial query results)
    /// generally this should be backgrounded and not awaited on.
    pub async fn start(&self) {
        self.livequery.wait_initialized().await;

        let mut items: Vec<V> = self.livequery.peek();

        let is_desc = self
            .display_order
            .first()
            .map(|o| o.direction == OrderDirection::Desc)
            .unwrap_or(false);
        if is_desc {
            items.reverse();
        }

        let live_window = self.live_window_size();
        let has_more_preceding = items.len() >= live_window;

        self.visible_set.set(VisibleSet {
            items,
            intersection: None,
            has_more_preceding,
            has_more_following: false,
            should_auto_scroll: true,
            error: None,
        });
    }

    // Computed properties
    fn threshold(&self) -> f64 {
        self.buffer_factor / 2.0
    }

    fn screen_items(&self) -> usize {
        windowing::screen_items(self.viewport_height, self.minimum_row_height)
    }

    fn live_window_size(&self) -> usize {
        windowing::live_window_size(self.screen_items(), self.threshold())
    }

    // Accessors
    pub fn visible_set(&self) -> Read<VisibleSet<V>> {
        self.visible_set.read()
    }

    pub fn mode(&self) -> ScrollMode {
        self.mode.peek()
    }

    /// Get the current selection (predicate + order by) as a string.
    pub fn current_selection(&self) -> String {
        let (selection, _version) = self.livequery.selection().peek();
        format!("{}", selection)
    }

    /// Notify the scroll manager of visible item changes
    ///
    /// # Arguments
    /// * `first_visible` - EntityId of the first (oldest) visible item
    /// * `last_visible` - EntityId of the last (newest) visible item
    /// * `scrolling_backward` - True if user is scrolling toward older items
    pub fn on_scroll(&self, first_visible: EntityId, last_visible: EntityId, scrolling_backward: bool) {
        let current = self.visible_set.peek();
        let screen = self.screen_items();

        // Find indices of visible items in current window
        let first_idx = current.items.iter().position(|item| item.entity().id() == first_visible);
        let last_idx = current.items.iter().position(|item| item.entity().id() == last_visible);

        let (first_visible_index, last_visible_index) = match (first_idx, last_idx) {
            (Some(f), Some(l)) => (f, l),
            _ => return, // Visible items not found in window - shouldn't happen
        };

        let items_above = first_visible_index;
        let items_below = current.items.len().saturating_sub(last_visible_index + 1);

        tracing::debug!(
            "on_scroll: first={}, last={}, items_above={}, items_below={}, screen={}, scrolling_backward={}, has_more_preceding={}",
            first_visible_index, last_visible_index, items_above, items_below, screen, scrolling_backward, current.has_more_preceding
        );

        // Trigger when buffer is at or below S items (one screenful remaining)
        if scrolling_backward && items_above <= screen && current.has_more_preceding {
            self.mode.set(ScrollMode::Backward);
            self.slide_window(&current, first_visible_index, last_visible_index, LoadDirection::Backward);
        } else if !scrolling_backward && items_below <= screen && current.has_more_following {
            self.mode.set(ScrollMode::Forward);
            self.slide_window(&current, first_visible_index, last_visible_index, LoadDirection::Forward);
        }
    }

    /// Slide the window in the given direction
    ///
    /// - Backward: anchor on newest_visible, cursor B items newer, query older items
    /// - Forward: anchor on oldest_visible, cursor B items older, query newer items (reversed ORDER BY)
    fn slide_window(
        &self,
        current: &VisibleSet<V>,
        oldest_visible_index: usize,
        newest_visible_index: usize,
        direction: LoadDirection,
    ) {
        let buffer = 2 * self.screen_items(); // B = 2S
        let max_index = current.items.len().saturating_sub(1);

        // Direction-specific: cursor position, intersection anchor, and comparison operator
        let (cursor_index, intersection_index, operator, reversed_order) = match direction {
            LoadDirection::Backward => (
                (newest_visible_index + buffer).min(max_index),
                newest_visible_index,
                ComparisonOperator::LessThanOrEqual,
                false,
            ),
            LoadDirection::Forward => (
                // If at oldest edge, start from beginning to include all items
                if current.has_more_preceding {
                    oldest_visible_index.saturating_sub(buffer)
                } else {
                    0
                },
                oldest_visible_index,
                ComparisonOperator::GreaterThanOrEqual,
                true,
            ),
        };

        // Dynamic limit: items from cursor to far visible edge + buffer
        let limit = (cursor_index.max(newest_visible_index) - cursor_index.min(oldest_visible_index) + 1) + buffer;

        tracing::debug!(
            "slide_window({:?}): visible=[{},{}], cursor={}, limit={}",
            direction, oldest_visible_index, newest_visible_index, cursor_index, limit
        );

        // Set pending state for callback
        let continuation = current.items.get(intersection_index)
            .map(|item| item.entity().id())
            .expect("intersection item must exist");

        self.pending.set(Some(PendingSlide {
            continuation,
            limit,
            direction,
            reversed_order,
        }));

        // Build cursor-constrained predicate
        let predicate = self.build_cursor_predicate(current, cursor_index, operator);

        // Build ORDER BY (reversed for forward pagination)
        let order_by = if reversed_order {
            self.display_order.iter().map(|item| OrderByItem {
                direction: match item.direction {
                    OrderDirection::Asc => OrderDirection::Desc,
                    OrderDirection::Desc => OrderDirection::Asc,
                },
                ..item.clone()
            }).collect()
        } else {
            self.display_order.clone()
        };

        let selection = Selection {
            predicate,
            order_by: Some(order_by),
            limit: Some((limit + 1) as u64), // +1 to detect has_more
        };

        if let Err(e) = self.livequery.update_selection(selection) {
            tracing::error!("Failed to update selection for {:?} slide: {}", direction, e);
        }
    }

    /// Build a predicate constrained by cursor: `base AND field OP cursor_value`
    fn build_cursor_predicate(
        &self,
        current: &VisibleSet<V>,
        cursor_index: usize,
        operator: ComparisonOperator,
    ) -> Predicate {
        let Some(cursor_item) = current.items.get(cursor_index) else {
            return self.predicate.clone();
        };
        let Some(order_item) = self.display_order.first() else {
            return self.predicate.clone();
        };
        let field_name = order_item.path.first();
        let Some(cursor_value) = cursor_item.entity().value(field_name) else {
            return self.predicate.clone();
        };

        let cursor_predicate = Predicate::Comparison {
            left: Box::new(Expr::Path(PathExpr::simple(field_name))),
            operator,
            right: Box::new(Expr::Literal(value_to_literal(&cursor_value))),
        };

        Predicate::And(
            Box::new(self.predicate.clone()),
            Box::new(cursor_predicate),
        )
    }
}

// ============================================================================
// Parsing Helpers
// ============================================================================

pub fn parse_order_by(s: &str) -> Result<Vec<OrderByItem>, String> {
    use ankql::parser::parse_selection;
    let selection_str = format!("true ORDER BY {}", s);
    let selection =
        parse_selection(&selection_str).map_err(|e| format!("Failed to parse ORDER BY: {}", e))?;
    selection
        .order_by
        .ok_or_else(|| "No ORDER BY parsed".to_string())
}

pub trait IntoOrderBy {
    fn into_order_by(self) -> Result<Vec<OrderByItem>, String>;
}

impl IntoOrderBy for &str {
    fn into_order_by(self) -> Result<Vec<OrderByItem>, String> {
        parse_order_by(self)
    }
}

impl IntoOrderBy for Vec<OrderByItem> {
    fn into_order_by(self) -> Result<Vec<OrderByItem>, String> {
        Ok(self)
    }
}

pub use ankurah_virtual_scroll_derive::generate_scroll_manager;
