//! Core scroll manager state machine
//!
//! This implements a two-step API for scroll pagination:
//! 1. `on_scroll()` returns `Option<LoadRequest>` when pagination should trigger
//! 2. Platform picks an anchor element, records its position
//! 3. Platform calls `load_with_anchor(timestamp, direction)` to get selection string
//! 4. After results arrive, platform measures anchor's new position and adjusts scroll

use crate::{LoadDirection, LoadRequest, PaginatedSelection, ScrollInput, ScrollMetrics, ScrollMode};

/// Configuration for the scroll manager
#[derive(Clone, Debug)]
pub struct ScrollConfig {
    /// Trigger loading when this fraction of viewport from edge (default: 0.75)
    pub min_buffer_ratio: f64,
    /// How far inside viewport to suggest anchor position (default: 0.75 viewport)
    pub anchor_offset_ratio: f64,
    /// Load this many viewports worth of content (default: 3.0)
    pub query_size_ratio: f64,
    /// Estimated row height for limit calculation (default: 74.0)
    pub estimated_row_height: f64,
}

impl Default for ScrollConfig {
    fn default() -> Self {
        Self {
            min_buffer_ratio: 0.75,
            anchor_offset_ratio: 0.75,
            query_size_ratio: 3.0,
            estimated_row_height: 74.0,
        }
    }
}

/// Pure state machine for managing virtual scroll pagination
///
/// Key design principles:
/// - NO blocking: Multiple loads can be in flight (Ankurah handles concurrency)
/// - Two-step API: on_scroll returns LoadRequest, platform picks anchor, then load_with_anchor
/// - Platform handles scroll adjustment (Rust doesn't know rendered heights)
#[derive(Debug)]
pub struct ScrollManager {
    // Current state
    mode: ScrollMode,
    selection: PaginatedSelection,
    metrics: ScrollMetrics,

    // Boundary tracking
    at_earliest: bool,
    at_latest: bool,

    // Timestamp anchors (from most recent results)
    oldest_timestamp: Option<i64>,
    newest_timestamp: Option<i64>,

    // Configuration
    config: ScrollConfig,
    viewport_height: f64,
}

impl ScrollManager {
    /// Create a new scroll manager
    pub fn new(base_predicate: &str, timestamp_field: &str, viewport_height: f64) -> Self {
        Self::with_config(base_predicate, timestamp_field, viewport_height, ScrollConfig::default())
    }

    /// Create a scroll manager with custom configuration
    pub fn with_config(
        base_predicate: &str,
        timestamp_field: &str,
        viewport_height: f64,
        config: ScrollConfig,
    ) -> Self {
        let limit = Self::compute_limit(viewport_height, &config);
        Self {
            mode: ScrollMode::Live,
            selection: PaginatedSelection::new(base_predicate, timestamp_field, limit),
            metrics: ScrollMetrics::default(),
            at_earliest: false,
            at_latest: true, // Start in live mode = at latest
            oldest_timestamp: None,
            newest_timestamp: None,
            config,
            viewport_height,
        }
    }

    /// Compute query limit based on viewport size
    fn compute_limit(viewport_height: f64, config: &ScrollConfig) -> usize {
        let query_height = viewport_height * config.query_size_ratio;
        let limit = (query_height / config.estimated_row_height).ceil() as usize;
        limit.max(20) // Minimum of 20 items
    }

    /// Process a scroll event
    ///
    /// Returns `Some(LoadRequest)` if the platform should trigger pagination.
    /// Platform should then pick an anchor and call `load_with_anchor`.
    ///
    /// NOTE: Does NOT block during loads - multiple requests are allowed.
    /// Ankurah handles concurrency with monotonic application.
    pub fn on_scroll(&mut self, input: ScrollInput) -> Option<LoadRequest> {
        // Update viewport height if changed significantly
        if (input.viewport_height - self.viewport_height).abs() > 1.0 {
            self.viewport_height = input.viewport_height;
            let new_limit = Self::compute_limit(self.viewport_height, &self.config);
            self.selection.set_limit(new_limit);
        }

        // Calculate gaps
        let top_gap = input.offset;
        let bottom_gap = input.content_height - input.offset - input.viewport_height;
        let min_buffer = input.viewport_height * self.config.min_buffer_ratio;
        let anchor_offset = input.viewport_height * self.config.anchor_offset_ratio;

        // Update metrics
        self.metrics = ScrollMetrics {
            top_gap,
            bottom_gap,
            min_buffer,
            result_count: self.metrics.result_count,
        };

        // Only trigger loads on user-initiated scrolls
        if !input.user_initiated {
            return None;
        }

        // Check for backward pagination (scrolling up, near top)
        if input.scroll_delta < 0.0 && top_gap < min_buffer && !self.at_earliest {
            return Some(LoadRequest {
                direction: LoadDirection::Backward,
                anchor_offset,
            });
        }

        // Check for forward pagination (scrolling down, near bottom, not in live mode)
        if input.scroll_delta > 0.0 && bottom_gap < min_buffer && !self.at_latest {
            return Some(LoadRequest {
                direction: LoadDirection::Forward,
                anchor_offset,
            });
        }

        None
    }

    /// Platform picked an anchor, build the continuation query
    ///
    /// Returns the selection string to pass to LiveQuery.updateSelection()
    pub fn load_with_anchor(&mut self, anchor_timestamp: i64, direction: LoadDirection) -> String {
        self.mode = match direction {
            LoadDirection::Backward => ScrollMode::Backward,
            LoadDirection::Forward => ScrollMode::Forward,
        };
        self.selection.set_continuation(anchor_timestamp, direction);
        self.selection.build()
    }

    /// Call after query results arrive
    ///
    /// Updates boundary state based on result count vs limit.
    pub fn on_results(
        &mut self,
        count: usize,
        oldest_timestamp: Option<i64>,
        newest_timestamp: Option<i64>,
    ) {
        self.oldest_timestamp = oldest_timestamp;
        self.newest_timestamp = newest_timestamp;
        self.metrics.result_count = count;

        // Boundary detection: fewer results than limit means we hit a boundary
        let at_boundary = count < self.selection.limit();

        match self.mode {
            ScrollMode::Live | ScrollMode::Backward => {
                self.at_earliest = at_boundary;
            }
            ScrollMode::Forward => {
                self.at_latest = at_boundary;
                // Auto-transition to live when we reach the latest
                if self.at_latest {
                    self.mode = ScrollMode::Live;
                    self.selection.clear_continuation();
                }
            }
        }
    }

    /// Update the base predicate (e.g., for search/filter)
    ///
    /// Returns the new selection string.
    pub fn update_filter(&mut self, predicate: &str, reset_continuation: bool) -> String {
        self.selection.update_base(predicate, reset_continuation);
        if reset_continuation {
            self.mode = ScrollMode::Live;
            self.at_earliest = false;
            self.at_latest = true;
            self.oldest_timestamp = None;
            self.newest_timestamp = None;
        }
        self.selection.build()
    }

    /// Jump to live mode (most recent content)
    ///
    /// Returns the new selection string.
    pub fn jump_to_live(&mut self) -> String {
        self.mode = ScrollMode::Live;
        self.selection.clear_continuation();
        self.at_earliest = false;
        self.at_latest = true;
        self.selection.build()
    }

    /// Get the current selection string (for initial query setup)
    pub fn selection(&self) -> String {
        self.selection.build()
    }

    /// Check if the container should auto-scroll to bottom
    pub fn should_auto_scroll(&self) -> bool {
        self.mode == ScrollMode::Live && self.metrics.bottom_gap < 50.0
    }

    /// Whether items should be reversed for chronological display order
    ///
    /// LiveQuery items come in query order:
    /// - Live/Backward mode: DESC (newest first) → reverse for display
    /// - Forward mode: ASC (oldest first) → no reverse needed
    pub fn should_reverse_for_display(&self) -> bool {
        self.mode != ScrollMode::Forward
    }

    // --- Getters ---

    pub fn mode(&self) -> ScrollMode {
        self.mode
    }

    pub fn metrics(&self) -> &ScrollMetrics {
        &self.metrics
    }

    pub fn at_earliest(&self) -> bool {
        self.at_earliest
    }

    pub fn at_latest(&self) -> bool {
        self.at_latest
    }

    pub fn config(&self) -> &ScrollConfig {
        &self.config
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // Initial State Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_initial_state() {
        let manager = ScrollManager::new("room = 'abc'", "timestamp", 600.0);
        assert_eq!(manager.mode(), ScrollMode::Live);
        assert!(!manager.at_earliest());
        assert!(manager.at_latest());
    }

    #[test]
    fn test_initial_selection_is_live_mode() {
        let manager = ScrollManager::new("room = 'abc' AND deleted = false", "timestamp", 600.0);
        let selection = manager.selection();
        assert!(selection.contains("ORDER BY timestamp DESC"));
        assert!(!selection.contains("<=")); // No continuation
        assert!(!selection.contains(">=")); // No continuation
    }

    // -------------------------------------------------------------------------
    // Load Trigger Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_backward_trigger_near_top() {
        let mut manager = ScrollManager::new("room = 'abc'", "timestamp", 600.0);
        // min_buffer = 600 * 0.75 = 450

        let input = ScrollInput {
            offset: 100.0, // top_gap = 100, which is < 450
            content_height: 2000.0,
            viewport_height: 600.0,
            scroll_delta: -10.0, // scrolling up
            user_initiated: true,
        };

        let request = manager.on_scroll(input);
        assert!(request.is_some());
        let req = request.unwrap();
        assert_eq!(req.direction, LoadDirection::Backward);
        assert!(req.anchor_offset > 0.0);
    }

    #[test]
    fn test_forward_trigger_near_bottom() {
        let mut manager = ScrollManager::new("room = 'abc'", "timestamp", 600.0);
        // Put in non-live mode first
        manager.mode = ScrollMode::Backward;
        manager.at_latest = false;

        // min_buffer = 450
        // bottom_gap = 2000 - 1500 - 600 = -100 (near bottom)
        let input = ScrollInput {
            offset: 1500.0,
            content_height: 2000.0,
            viewport_height: 600.0,
            scroll_delta: 10.0, // scrolling down
            user_initiated: true,
        };

        let request = manager.on_scroll(input);
        assert!(request.is_some());
        assert_eq!(request.unwrap().direction, LoadDirection::Forward);
    }

    #[test]
    fn test_no_trigger_at_earliest_boundary() {
        let mut manager = ScrollManager::new("room = 'abc'", "timestamp", 600.0);
        manager.at_earliest = true;

        let input = ScrollInput {
            offset: 50.0,
            content_height: 2000.0,
            viewport_height: 600.0,
            scroll_delta: -10.0,
            user_initiated: true,
        };

        let request = manager.on_scroll(input);
        assert!(request.is_none()); // At boundary, shouldn't trigger
    }

    #[test]
    fn test_no_trigger_at_latest_boundary() {
        let mut manager = ScrollManager::new("room = 'abc'", "timestamp", 600.0);
        // Live mode is always at_latest

        let input = ScrollInput {
            offset: 1400.0,
            content_height: 2000.0,
            viewport_height: 600.0,
            scroll_delta: 10.0,
            user_initiated: true,
        };

        let request = manager.on_scroll(input);
        assert!(request.is_none()); // In live mode, shouldn't trigger forward
    }

    #[test]
    fn test_no_trigger_momentum_scroll() {
        let mut manager = ScrollManager::new("room = 'abc'", "timestamp", 600.0);

        let input = ScrollInput {
            offset: 50.0,
            content_height: 2000.0,
            viewport_height: 600.0,
            scroll_delta: -10.0,
            user_initiated: false, // Not user initiated
        };

        let request = manager.on_scroll(input);
        assert!(request.is_none());
    }

    #[test]
    fn test_trigger_allowed_multiple_times() {
        // No blocking - multiple triggers allowed (Ankurah handles concurrency)
        let mut manager = ScrollManager::new("room = 'abc'", "timestamp", 600.0);

        let input = ScrollInput {
            offset: 50.0,
            content_height: 2000.0,
            viewport_height: 600.0,
            scroll_delta: -10.0,
            user_initiated: true,
        };

        // First trigger
        let first = manager.on_scroll(input);
        assert!(first.is_some());

        // Second trigger (should also work - no blocking)
        let second = manager.on_scroll(input);
        assert!(second.is_some());
    }

    // -------------------------------------------------------------------------
    // Two-Step API Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_load_with_anchor_backward() {
        let mut manager = ScrollManager::new("room = 'abc'", "timestamp", 600.0);

        let selection = manager.load_with_anchor(1704067200000, LoadDirection::Backward);

        assert!(selection.contains("timestamp <= 1704067200000"));
        assert!(selection.contains("ORDER BY timestamp DESC"));
        assert_eq!(manager.mode(), ScrollMode::Backward);
    }

    #[test]
    fn test_load_with_anchor_forward() {
        let mut manager = ScrollManager::new("room = 'abc'", "timestamp", 600.0);

        let selection = manager.load_with_anchor(1704067200000, LoadDirection::Forward);

        assert!(selection.contains("timestamp >= 1704067200000"));
        assert!(selection.contains("ORDER BY timestamp ASC"));
        assert_eq!(manager.mode(), ScrollMode::Forward);
    }

    // -------------------------------------------------------------------------
    // Boundary Detection Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_at_earliest_when_count_less_than_limit() {
        let mut manager = ScrollManager::new("room = 'abc'", "timestamp", 600.0);
        manager.mode = ScrollMode::Backward;
        let limit = manager.selection.limit();

        // Fewer results than limit = at boundary
        manager.on_results(limit - 1, Some(1000), Some(2000));

        assert!(manager.at_earliest());
    }

    #[test]
    fn test_not_at_earliest_when_count_equals_limit() {
        let mut manager = ScrollManager::new("room = 'abc'", "timestamp", 600.0);
        manager.mode = ScrollMode::Backward;
        let limit = manager.selection.limit();

        manager.on_results(limit, Some(1000), Some(2000));

        assert!(!manager.at_earliest());
    }

    #[test]
    fn test_forward_auto_transitions_to_live() {
        let mut manager = ScrollManager::new("room = 'abc'", "timestamp", 600.0);
        manager.mode = ScrollMode::Forward;
        manager.at_latest = false;
        let limit = manager.selection.limit();

        // Fewer results in forward mode = reached latest
        manager.on_results(limit - 1, Some(1000), Some(2000));

        assert!(manager.at_latest());
        assert_eq!(manager.mode(), ScrollMode::Live);
    }

    // -------------------------------------------------------------------------
    // Mode Transition Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_jump_to_live() {
        let mut manager = ScrollManager::new("room = 'abc'", "timestamp", 600.0);
        manager.mode = ScrollMode::Backward;
        manager.selection.set_continuation(1000, LoadDirection::Backward);

        let selection = manager.jump_to_live();

        assert_eq!(manager.mode(), ScrollMode::Live);
        assert!(!selection.contains("1000")); // Continuation cleared
        assert!(selection.contains("ORDER BY timestamp DESC"));
    }

    #[test]
    fn test_filter_update_resets_to_live() {
        let mut manager = ScrollManager::new("room = 'abc'", "timestamp", 600.0);
        manager.mode = ScrollMode::Backward;
        manager.at_earliest = true;

        let selection = manager.update_filter("room = 'xyz'", true);

        assert_eq!(manager.mode(), ScrollMode::Live);
        assert!(!manager.at_earliest());
        assert!(manager.at_latest());
        assert!(selection.contains("room = 'xyz'"));
    }

    #[test]
    fn test_filter_update_preserves_continuation() {
        let mut manager = ScrollManager::new("room = 'abc'", "timestamp", 600.0);
        manager.load_with_anchor(1000, LoadDirection::Backward);

        let selection = manager.update_filter("room = 'abc' AND author = 'user1'", false);

        assert_eq!(manager.mode(), ScrollMode::Backward);
        assert!(selection.contains("timestamp <= 1000"));
        assert!(selection.contains("author = 'user1'"));
    }

    // -------------------------------------------------------------------------
    // Display Order Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_display_order_live_mode() {
        let manager = ScrollManager::new("room = 'abc'", "timestamp", 600.0);
        assert!(manager.should_reverse_for_display()); // DESC needs reverse
    }

    #[test]
    fn test_display_order_backward_mode() {
        let mut manager = ScrollManager::new("room = 'abc'", "timestamp", 600.0);
        manager.mode = ScrollMode::Backward;
        assert!(manager.should_reverse_for_display()); // DESC needs reverse
    }

    #[test]
    fn test_display_order_forward_mode() {
        let mut manager = ScrollManager::new("room = 'abc'", "timestamp", 600.0);
        manager.mode = ScrollMode::Forward;
        assert!(!manager.should_reverse_for_display()); // ASC, no reverse
    }

    // -------------------------------------------------------------------------
    // Auto-Scroll Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_should_auto_scroll_live_near_bottom() {
        let mut manager = ScrollManager::new("room = 'abc'", "timestamp", 600.0);
        manager.metrics.bottom_gap = 30.0;
        assert!(manager.should_auto_scroll());
    }

    #[test]
    fn test_should_not_auto_scroll_when_scrolled_up() {
        let mut manager = ScrollManager::new("room = 'abc'", "timestamp", 600.0);
        manager.metrics.bottom_gap = 200.0;
        assert!(!manager.should_auto_scroll());
    }

    #[test]
    fn test_should_not_auto_scroll_in_backward_mode() {
        let mut manager = ScrollManager::new("room = 'abc'", "timestamp", 600.0);
        manager.mode = ScrollMode::Backward;
        manager.metrics.bottom_gap = 30.0;
        assert!(!manager.should_auto_scroll());
    }
}
