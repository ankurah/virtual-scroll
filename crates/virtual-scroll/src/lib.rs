//! Virtual Scroll - Ankurah-integrated virtual scroll state machine
//!
//! This crate provides a scroll manager that integrates with Ankurah's LiveQuery
//! to handle infinite scroll pagination with pixel-perfect scroll stability.
//!
//! # Key Concepts
//!
//! - **VisibleSet**: The current set of items in display order, with scroll state
//!   for the renderer including intersection item and boundary indicators.
//!
//! - **Intersection**: When pagination occurs, this identifies the item that exists
//!   in both the previous and new result sets, enabling scroll position adjustment.
//!
//! - **Display Order**: Items are always presented in a consistent display order
//!   (e.g., timestamp DESC for chat), regardless of the underlying query order.

use ankql::ast::{ComparisonOperator, Expr, Literal, OrderByItem, Predicate, Selection};
use ankurah::changes::ChangeSet;
use ankurah::core::selection::filter::Filterable;
use ankurah::{model::View, Context, LiveQuery};
use ankurah_proto::EntityId;
use ankurah_signals::{Mut, Peek, Read, Subscribe};

// Re-export key types for convenience
pub use ankql::ast::{OrderByItem as OrderBy, OrderDirection, PathExpr, Predicate as Filter};
pub use ankurah_proto::EntityId as Id;

// Re-export for derive macro generated code
pub use ankurah_signals;

// ============================================================================
// Core Types
// ============================================================================

/// The visible set of items exposed via a signal
///
/// Contains everything the renderer needs for a single atomic update:
/// items, scroll stability anchor, and state flags.
#[derive(Clone, Debug)]
pub struct VisibleSet<V> {
    /// Items in display order
    pub items: Vec<V>,
    /// Item that spans both old and new sets during pagination (for scroll stability)
    pub intersection: Option<Intersection>,
    /// More older content exists (can paginate backward)
    pub has_more_older: bool,
    /// More newer content exists (not at live edge)
    pub has_more_newer: bool,
    /// Renderer should auto-scroll to bottom when items change
    pub should_auto_scroll: bool,
}

impl<V> Default for VisibleSet<V> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            intersection: None,
            has_more_older: true,     // Assume more until we know otherwise
            has_more_newer: false,    // Start in live mode
            should_auto_scroll: true, // Live mode = auto-scroll
        }
    }
}

/// Identifies the item that exists in both old and new result sets
#[derive(Clone, Debug)]
pub struct Intersection {
    /// Entity ID of the intersection item
    pub entity_id: EntityId,
    /// Index of this item in the VisibleSet.items vec
    pub index: usize,
}

/// Pagination direction
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadDirection {
    /// Load older items (scroll up toward history)
    Backward,
    /// Load newer items (scroll down, returning to live)
    Forward,
}

/// Current scroll mode
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollMode {
    /// At latest, receiving real-time updates
    Live,
    /// Paginating backward (loading older items)
    Backward,
    /// Paginating forward (returning to live)
    Forward,
}

/// Configuration for the scroll manager
#[derive(Clone, Copy, Debug)]
pub struct ScrollConfig {
    /// Trigger loading when gap is less than this fraction of viewport (default: 0.75)
    pub min_buffer_ratio: f64,
    /// Load this many viewports worth of content (default: 3.0)
    pub query_size_ratio: f64,
    /// Estimated row height for limit calculation (default: 74.0)
    pub estimated_row_height: f64,
}

impl Default for ScrollConfig {
    fn default() -> Self {
        Self {
            min_buffer_ratio: 0.75,
            query_size_ratio: 3.0,
            estimated_row_height: 74.0,
        }
    }
}

// ============================================================================
// ScrollManager
// ============================================================================

/// Virtual scroll manager with Ankurah LiveQuery integration
///
/// Manages pagination state and exposes items via a reactive signal.
pub struct ScrollManager<V: View + Clone + Send + Sync + 'static> {
    /// The underlying LiveQuery
    livequery: LiveQuery<V>,
    /// Display order (visual presentation order, e.g. timestamp DESC)
    display_order: Vec<OrderByItem>,
    /// Base predicate (without continuation)
    base_predicate: Predicate,
    /// Current scroll mode
    mode: ScrollMode,
    /// Boundary tracking
    at_earliest: bool,
    at_latest: bool,
    /// Loading state
    is_loading: bool,
    /// Current anchor value (for computing intersection after query update)
    current_anchor: Option<i64>,
    /// Current continuation direction
    current_direction: Option<LoadDirection>,
    /// The visible set signal
    visible_set: Mut<VisibleSet<V>>,
    /// Loading state signal (separate from VisibleSet)
    loading_signal: Mut<bool>,
    /// Configuration
    config: ScrollConfig,
    viewport_height: f64,
    /// Query limit
    limit: usize,
    /// Subscription guard (kept alive for livequery subscription)
    _subscription: ankurah_signals::SubscriptionGuard,
}

/// Default viewport height used when none is provided
const DEFAULT_VIEWPORT_HEIGHT: f64 = 600.0;

impl<V: View + Clone + Send + Sync + 'static> ScrollManager<V> {
    /// Create a new scroll manager (not yet initialized)
    ///
    /// Call `start()` to initialize the query and populate items.
    /// Call `set_viewport_height()` once the actual viewport dimensions are known.
    ///
    /// # Arguments
    /// * `ctx` - Ankurah context
    /// * `predicate` - Base filter predicate (e.g., `"room = 'abc' AND deleted = false"`)
    /// * `display_order` - Visual presentation order (e.g., `[timestamp DESC]` for chat)
    pub fn new(
        ctx: &Context,
        predicate: impl TryInto<Predicate, Error = impl std::fmt::Debug>,
        display_order: impl Into<Vec<OrderByItem>>,
    ) -> Result<Self, ankurah::error::RetrievalError> {
        Self::with_config(
            ctx,
            predicate,
            display_order,
            ScrollConfig::default(),
        )
    }

    /// Create a scroll manager with custom configuration (not yet initialized)
    ///
    /// Call `start()` to initialize the query and populate items.
    /// Call `set_viewport_height()` once the actual viewport dimensions are known.
    pub fn with_config(
        ctx: &Context,
        predicate: impl TryInto<Predicate, Error = impl std::fmt::Debug>,
        display_order: impl Into<Vec<OrderByItem>>,
        config: ScrollConfig,
    ) -> Result<Self, ankurah::error::RetrievalError> {
        let base_predicate = predicate.try_into().expect("Failed to parse predicate");
        let display_order = display_order.into();
        let viewport_height = DEFAULT_VIEWPORT_HEIGHT;
        let limit = Self::compute_limit(viewport_height, &config);

        // Build initial selection (live mode = display order)
        let initial_selection = Selection {
            predicate: base_predicate.clone(),
            order_by: Some(display_order.clone()),
            limit: Some(limit as u64),
        };

        // Create the LiveQuery (not yet initialized)
        let livequery: LiveQuery<V> = ctx.query(initial_selection)?;

        // Create signals
        let visible_set: Mut<VisibleSet<V>> = Mut::new(VisibleSet::default());
        let loading_signal: Mut<bool> = Mut::new(false);

        // Check if display order is DESC (meaning we need to reverse for oldest-at-top display)
        let is_desc = display_order
            .first()
            .map(|o| o.direction == OrderDirection::Desc)
            .unwrap_or(false);

        // Subscribe to livequery changes for live mode updates
        let visible_set_clone = visible_set.clone();
        let subscription = livequery.subscribe(move |changeset: ChangeSet<V>| {
            // Get items from the changeset's resultset
            let mut items: Vec<V> = changeset.resultset.peek();
            // Reverse DESC query results for oldest-at-top display
            if is_desc {
                items.reverse();
            }
            let current = visible_set_clone.peek();
            // Preserve flags, just update items (live mode updates)
            visible_set_clone.set(VisibleSet {
                items,
                intersection: None, // No intersection for live updates
                has_more_older: current.has_more_older,
                has_more_newer: current.has_more_newer,
                should_auto_scroll: current.should_auto_scroll,
            });
        });

        Ok(Self {
            livequery,
            display_order,
            base_predicate,
            mode: ScrollMode::Live,
            at_earliest: false,
            at_latest: true,
            is_loading: false,
            current_anchor: None,
            current_direction: None,
            visible_set,
            loading_signal,
            config,
            viewport_height,
            limit,
            _subscription: subscription,
        })
    }

    /// Initialize the scroll manager and populate items
    ///
    /// Must be called after construction before accessing items.
    pub async fn start(&mut self) {
        // Wait for the LiveQuery to initialize
        self.livequery.wait_initialized().await;

        // Set initial items
        let mut initial_items: Vec<V> = self.livequery.peek();

        // Reverse DESC query results for oldest-at-top display (e.g., chat style)
        let is_desc = self
            .display_order
            .first()
            .map(|o| o.direction == OrderDirection::Desc)
            .unwrap_or(false);
        if is_desc {
            initial_items.reverse();
        }

        self.at_earliest = initial_items.len() < self.limit;
        self.visible_set.set(VisibleSet {
            items: initial_items,
            intersection: None,
            has_more_older: !self.at_earliest,
            has_more_newer: false,    // Live mode
            should_auto_scroll: true, // Live mode
        });
    }

    /// Compute query limit based on viewport size
    fn compute_limit(viewport_height: f64, config: &ScrollConfig) -> usize {
        let query_height = viewport_height * config.query_size_ratio;
        let limit = (query_height / config.estimated_row_height).ceil() as usize;
        limit.max(20)
    }

    /// Process a scroll event (only call for user-initiated scrolls)
    ///
    /// Returns `Some(LoadDirection)` if pagination was triggered.
    /// When triggered, the scroll manager updates the query and `visible_set` signal
    /// will emit with the new items and `intersection` set for scroll stability.
    ///
    /// Platform should:
    /// 1. Before calling, measure the position of items near the scroll edge
    /// 2. Call this method
    /// 3. If it returns Some, watch for visible_set update
    /// 4. Find the intersection item and measure its new position
    /// 5. Adjust scrollTop by the delta
    pub async fn on_scroll(
        &mut self,
        top_gap: f64,
        bottom_gap: f64,
        scrolling_up: bool,
    ) -> Option<LoadDirection> {
        // Don't trigger if already loading
        if self.is_loading {
            return None;
        }

        let min_buffer = self.viewport_height * self.config.min_buffer_ratio;

        // Backward pagination (scrolling up, near top, not at earliest)
        if scrolling_up && top_gap < min_buffer && !self.at_earliest {
            self.load_direction(LoadDirection::Backward).await;
            return Some(LoadDirection::Backward);
        }

        // Forward pagination (scrolling down, near bottom, not at latest/live)
        if !scrolling_up && bottom_gap < min_buffer && !self.at_latest {
            self.load_direction(LoadDirection::Forward).await;
            return Some(LoadDirection::Forward);
        }

        None
    }

    /// Update viewport height (call when container resizes)
    pub fn set_viewport_height(&mut self, height: f64) {
        if (height - self.viewport_height).abs() > 1.0 {
            self.viewport_height = height;
            self.limit = Self::compute_limit(self.viewport_height, &self.config);
        }
    }

    /// Trigger pagination in the specified direction
    ///
    /// Automatically picks the anchor from current items based on direction:
    /// - Backward: uses the oldest item's timestamp (last in DESC order)
    /// - Forward: uses the newest item's timestamp (first in DESC order)
    async fn load_direction(&mut self, direction: LoadDirection) {
        use ankurah::core::value::Value;

        // Get current items to find anchor
        let current_set = self.visible_set.peek();
        if current_set.items.is_empty() {
            return;
        }

        // Get the ordering field name
        let field_name = match self.display_order.first() {
            Some(order) => order.path.first(),
            None => return,
        };

        // Pick anchor based on direction
        // After reversal for chat display: first item is OLDEST, last item is NEWEST
        // Backward = want older items, anchor = oldest current = first item
        // Forward = want newer items, anchor = newest current = last item
        let anchor_item = match direction {
            LoadDirection::Backward => current_set.items.first(),
            LoadDirection::Forward => current_set.items.last(),
        };

        let anchor_value = match anchor_item {
            Some(item) => match item.entity().value(field_name) {
                Some(Value::I64(v)) => v,
                Some(Value::I32(v)) => v as i64,
                Some(Value::I16(v)) => v as i64,
                _ => return,
            },
            None => return,
        };

        self.is_loading = true;
        self.loading_signal.set(true);
        self.current_anchor = Some(anchor_value);
        self.current_direction = Some(direction);
        self.mode = match direction {
            LoadDirection::Backward => ScrollMode::Backward,
            LoadDirection::Forward => ScrollMode::Forward,
        };

        // Build the continuation selection
        let selection = self.build_selection(Some((anchor_value, direction)));

        // Update the LiveQuery and wait for results
        if let Err(e) = self.livequery.update_selection_wait(selection).await {
            tracing::error!("Failed to update selection: {:?}", e);
            self.is_loading = false;
            self.loading_signal.set(false);
            return;
        }

        // Compute the new visible set with intersection
        self.compute_visible_set_with_intersection(anchor_value, direction);

        self.is_loading = false;
        self.loading_signal.set(false);
    }

    /// Build a Selection for the current state
    fn build_selection(&self, continuation: Option<(i64, LoadDirection)>) -> Selection {
        let mut predicate = self.base_predicate.clone();

        // Determine query order based on continuation direction
        // Live/Backward: use display order (e.g., DESC for chat)
        // Forward: reverse display order (ASC) to get newer items
        let query_order = match continuation.map(|(_, dir)| dir) {
            Some(LoadDirection::Forward) => self
                .display_order
                .iter()
                .map(|item| OrderByItem {
                    path: item.path.clone(),
                    direction: match item.direction {
                        OrderDirection::Asc => OrderDirection::Desc,
                        OrderDirection::Desc => OrderDirection::Asc,
                    },
                })
                .collect(),
            _ => self.display_order.clone(),
        };

        // Add continuation clause if present
        if let Some((anchor_value, direction)) = continuation {
            if let Some(first_order) = self.display_order.first() {
                let field_expr = Expr::Path(first_order.path.clone());
                let anchor_expr = Expr::Literal(Literal::I64(anchor_value));

                // Determine comparison operator:
                // DESC display + Backward = loading older = timestamp <= anchor
                // DESC display + Forward = loading newer = timestamp >= anchor
                // ASC display + Backward = loading older = timestamp >= anchor
                // ASC display + Forward = loading newer = timestamp <= anchor
                let operator = match (first_order.direction.clone(), direction) {
                    (OrderDirection::Desc, LoadDirection::Backward) => {
                        ComparisonOperator::LessThanOrEqual
                    }
                    (OrderDirection::Desc, LoadDirection::Forward) => {
                        ComparisonOperator::GreaterThanOrEqual
                    }
                    (OrderDirection::Asc, LoadDirection::Backward) => {
                        ComparisonOperator::GreaterThanOrEqual
                    }
                    (OrderDirection::Asc, LoadDirection::Forward) => {
                        ComparisonOperator::LessThanOrEqual
                    }
                };

                let continuation_pred = Predicate::Comparison {
                    left: Box::new(field_expr),
                    operator,
                    right: Box::new(anchor_expr),
                };

                predicate = Predicate::And(Box::new(predicate), Box::new(continuation_pred));
            }
        }

        Selection {
            predicate,
            order_by: Some(query_order),
            limit: Some(self.limit as u64),
        }
    }

    /// Compute visible set with intersection after a load
    fn compute_visible_set_with_intersection(
        &mut self,
        anchor_value: i64,
        direction: LoadDirection,
    ) {
        let mut items: Vec<V> = self.livequery.peek();
        let count = items.len();
        let at_boundary = count < self.limit;

        // Check if display order is DESC
        let is_desc = self
            .display_order
            .first()
            .map(|o| o.direction == OrderDirection::Desc)
            .unwrap_or(false);

        // Reverse items based on direction and display order
        // - DESC display + Backward: query is DESC, need to reverse for oldest-at-top
        // - DESC display + Forward: query is ASC, need to reverse for oldest-at-top
        // - ASC display: no reversal needed
        if is_desc {
            items.reverse();
        }

        // Find the intersection item by anchor value
        let intersection = self.find_intersection_item(&items, anchor_value);

        // Update boundary state based on result count (at_boundary computed above)
        match direction {
            LoadDirection::Backward => {
                self.at_earliest = at_boundary;
                // We're no longer at the live edge when paginating backward
                self.at_latest = false;
            }
            LoadDirection::Forward => {
                self.at_latest = at_boundary;
                if self.at_latest {
                    // Auto-transition to live when we reach the latest
                    self.mode = ScrollMode::Live;
                    self.current_anchor = None;
                    self.current_direction = None;
                }
            }
        }

        self.visible_set.set(VisibleSet {
            items,
            intersection,
            has_more_older: !self.at_earliest,
            has_more_newer: !self.at_latest,
            should_auto_scroll: self.mode == ScrollMode::Live,
        });
    }

    /// Find the intersection item in the items list by matching anchor value
    fn find_intersection_item(&self, items: &[V], anchor_value: i64) -> Option<Intersection> {
        use ankurah::core::value::Value;

        let field_path = self.display_order.first()?.path.clone();
        let field_name = field_path.first(); // Get the field name from the path

        for (index, item) in items.iter().enumerate() {
            let entity = item.entity();
            if let Some(value) = entity.value(field_name) {
                let item_value = match value {
                    Value::I64(v) => Some(v),
                    Value::I32(v) => Some(v as i64),
                    Value::I16(v) => Some(v as i64),
                    _ => None,
                };
                if item_value == Some(anchor_value) {
                    return Some(Intersection {
                        entity_id: entity.id(),
                        index,
                    });
                }
            }
        }
        None
    }

    /// Jump to live mode (most recent content)
    pub async fn jump_to_live(&mut self) {
        self.mode = ScrollMode::Live;
        self.current_anchor = None;
        self.current_direction = None;
        self.at_earliest = false;
        self.at_latest = true;

        let selection = self.build_selection(None);
        if let Err(e) = self.livequery.update_selection_wait(selection).await {
            tracing::error!("Failed to jump to live: {:?}", e);
            return;
        }

        let mut items: Vec<V> = self.livequery.peek();

        // Reverse DESC query results for oldest-at-top display
        let is_desc = self
            .display_order
            .first()
            .map(|o| o.direction == OrderDirection::Desc)
            .unwrap_or(false);
        if is_desc {
            items.reverse();
        }

        self.at_earliest = items.len() < self.limit;

        self.visible_set.set(VisibleSet {
            items,
            intersection: None,
            has_more_older: !self.at_earliest,
            has_more_newer: false,
            should_auto_scroll: true,
        });
    }

    /// Update the filter predicate
    ///
    /// If `reset_position` is true, returns to live mode.
    /// If false, attempts to maintain current scroll position.
    pub async fn update_filter(
        &mut self,
        predicate: impl TryInto<Predicate, Error = impl std::fmt::Debug>,
        reset_position: bool,
    ) {
        self.base_predicate = predicate.try_into().expect("Failed to parse predicate");

        if reset_position {
            self.mode = ScrollMode::Live;
            self.current_anchor = None;
            self.current_direction = None;
            self.at_earliest = false;
            self.at_latest = true;
        }

        let continuation = if reset_position {
            None
        } else {
            self.current_anchor.zip(self.current_direction)
        };

        let selection = self.build_selection(continuation);
        if let Err(e) = self.livequery.update_selection_wait(selection).await {
            tracing::error!("Failed to update filter: {:?}", e);
            return;
        }

        let mut items: Vec<V> = self.livequery.peek();

        // Reverse DESC query results for oldest-at-top display
        let is_desc = self
            .display_order
            .first()
            .map(|o| o.direction == OrderDirection::Desc)
            .unwrap_or(false);
        if is_desc {
            items.reverse();
        }

        let at_boundary = items.len() < self.limit;

        if reset_position {
            self.at_earliest = at_boundary;
        }

        self.visible_set.set(VisibleSet {
            items,
            intersection: None,
            has_more_older: !self.at_earliest,
            has_more_newer: !self.at_latest,
            should_auto_scroll: self.mode == ScrollMode::Live,
        });
    }

    /// Get the visible set signal
    ///
    /// Subscribe to this for reactive updates to items and scroll state.
    pub fn visible_set(&self) -> Read<VisibleSet<V>> {
        self.visible_set.read()
    }

    /// Get the loading state signal
    pub fn is_loading(&self) -> Read<bool> {
        self.loading_signal.read()
    }

    /// Get current scroll mode
    pub fn mode(&self) -> ScrollMode {
        self.mode
    }

    /// Get current configuration
    pub fn config(&self) -> &ScrollConfig {
        &self.config
    }
}

// Re-export derive macro
pub use virtual_scroll_derive::generate_scroll_manager;

// ============================================================================
// Parsing Helpers
// ============================================================================

/// Parse an ORDER BY string into a Vec<OrderByItem>
///
/// Accepts formats like:
/// - "timestamp DESC"
/// - "timestamp DESC, room ASC"
/// - "timestamp" (defaults to ASC)
pub fn parse_order_by(order_by_str: &str) -> Result<Vec<OrderByItem>, String> {
    use ankql::parser::parse_selection;

    // TODO: move this to ankql - and maybe create a parse_order_by function there

    // Wrap in a minimal selection to use ankql's parser
    let selection_str = format!("true ORDER BY {}", order_by_str);

    let selection = parse_selection(&selection_str)
        .map_err(|e| format!("Failed to parse ORDER BY '{}': {}", order_by_str, e))?;

    selection
        .order_by
        .ok_or_else(|| format!("No ORDER BY clause parsed from '{}'", order_by_str))
}
