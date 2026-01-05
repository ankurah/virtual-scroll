//! Core scroll manager state machine

use crate::{LoadDirection, PaginatedSelection, ScrollInput, ScrollMetrics, ScrollMode};

/// Configuration for the scroll manager
#[derive(Clone, Debug)]
pub struct ScrollConfig {
    /// Trigger loading when this fraction of viewport from edge (default: 0.75)
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

/// Pure state machine for managing virtual scroll pagination
///
/// This struct has NO external dependencies. It takes scroll metrics as input
/// and outputs selection strings for query systems like LiveQuery.
#[derive(Debug)]
pub struct ScrollManager {
    // Current state
    mode: ScrollMode,
    loading: Option<LoadDirection>,
    selection: PaginatedSelection,
    metrics: ScrollMetrics,

    // Boundary tracking
    at_earliest: bool,
    at_latest: bool,

    // Timestamp anchors for continuation
    oldest_timestamp: Option<i64>,
    newest_timestamp: Option<i64>,

    // Configuration
    config: ScrollConfig,
    viewport_height: f64,
}

impl ScrollManager {
    /// Create a new scroll manager
    ///
    /// # Arguments
    /// * `base_predicate` - Base filter (e.g., "room = 'abc' AND deleted = false")
    /// * `timestamp_field` - Field to use for ordering (e.g., "timestamp")
    /// * `viewport_height` - Initial viewport height in pixels
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
            loading: None,
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
    /// Returns `Some(selection)` if the LiveQuery should update its selection,
    /// or `None` if no update is needed.
    pub fn on_scroll(&mut self, input: ScrollInput) -> Option<String> {
        // Update viewport height if changed
        if (input.viewport_height - self.viewport_height).abs() > 1.0 {
            self.viewport_height = input.viewport_height;
            let new_limit = Self::compute_limit(self.viewport_height, &self.config);
            self.selection.set_limit(new_limit);
        }

        // Calculate gaps
        let top_gap = input.offset;
        let bottom_gap = input.content_height - input.offset - input.viewport_height;
        let min_buffer = input.viewport_height * self.config.min_buffer_ratio;

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

        // Already loading? Don't trigger another
        if self.loading.is_some() {
            return None;
        }

        // Check for backward pagination (scrolling up, near top)
        if input.scroll_delta < 0.0
            && top_gap < min_buffer
            && !self.at_earliest
        {
            if let Some(oldest_ts) = self.oldest_timestamp {
                self.loading = Some(LoadDirection::Backward);
                self.mode = ScrollMode::Backward;
                self.selection.set_continuation(oldest_ts, LoadDirection::Backward);
                return Some(self.selection.build());
            }
        }

        // Check for forward pagination (scrolling down, near bottom)
        if input.scroll_delta > 0.0
            && bottom_gap < min_buffer
            && !self.at_latest
        {
            if let Some(newest_ts) = self.newest_timestamp {
                self.loading = Some(LoadDirection::Forward);
                self.mode = ScrollMode::Forward;
                self.selection.set_continuation(newest_ts, LoadDirection::Forward);
                return Some(self.selection.build());
            }
        }

        None
    }

    /// Call after query results arrive
    ///
    /// Updates boundary state and stores timestamps for pagination anchors.
    pub fn on_results(
        &mut self,
        count: usize,
        oldest_timestamp: Option<i64>,
        newest_timestamp: Option<i64>,
    ) {
        self.loading = None;
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
    ///
    /// # Arguments
    /// * `predicate` - New base predicate
    /// * `reset_continuation` - If true, resets scroll position to live mode
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
        self.loading = None;
        self.selection.build()
    }

    /// Get the current selection string
    pub fn selection(&self) -> String {
        self.selection.build()
    }

    /// Check if the container should auto-scroll to bottom
    pub fn should_auto_scroll(&self) -> bool {
        self.mode == ScrollMode::Live && self.metrics.bottom_gap < 50.0
    }

    // --- Getters ---

    /// Current scroll mode
    pub fn mode(&self) -> ScrollMode {
        self.mode
    }

    /// Whether a load is in progress
    pub fn is_loading(&self) -> bool {
        self.loading.is_some()
    }

    /// Current loading direction, if any
    pub fn loading_direction(&self) -> Option<LoadDirection> {
        self.loading
    }

    /// Current scroll metrics
    pub fn metrics(&self) -> &ScrollMetrics {
        &self.metrics
    }

    /// Whether we've reached the earliest content
    pub fn at_earliest(&self) -> bool {
        self.at_earliest
    }

    /// Whether we're at the latest content
    pub fn at_latest(&self) -> bool {
        self.at_latest
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state() {
        let manager = ScrollManager::new("room = 'abc'", "timestamp", 600.0);
        assert_eq!(manager.mode(), ScrollMode::Live);
        assert!(!manager.is_loading());
        assert!(!manager.at_earliest());
        assert!(manager.at_latest());
    }

    #[test]
    fn test_backward_pagination_trigger() {
        let mut manager = ScrollManager::new("room = 'abc'", "timestamp", 600.0);

        // Simulate receiving initial results
        manager.on_results(50, Some(1000), Some(2000));

        // Scroll up near the top
        let input = ScrollInput {
            offset: 50.0,
            content_height: 2000.0,
            viewport_height: 600.0,
            scroll_delta: -10.0,
            user_initiated: true,
        };

        let selection = manager.on_scroll(input);
        assert!(selection.is_some());
        assert!(selection.unwrap().contains("timestamp <= 1000"));
        assert_eq!(manager.mode(), ScrollMode::Backward);
        assert!(manager.is_loading());
    }

    #[test]
    fn test_no_trigger_when_loading() {
        let mut manager = ScrollManager::new("room = 'abc'", "timestamp", 600.0);
        manager.on_results(50, Some(1000), Some(2000));

        let input = ScrollInput {
            offset: 50.0,
            content_height: 2000.0,
            viewport_height: 600.0,
            scroll_delta: -10.0,
            user_initiated: true,
        };

        // First scroll triggers load
        let first = manager.on_scroll(input);
        assert!(first.is_some());

        // Second scroll should not trigger another load
        let second = manager.on_scroll(input);
        assert!(second.is_none());
    }

    #[test]
    fn test_boundary_detection() {
        let mut manager = ScrollManager::new("room = 'abc'", "timestamp", 600.0);

        // Results less than limit = at boundary
        manager.on_results(15, Some(1000), Some(2000));
        assert!(manager.at_earliest());
    }

    #[test]
    fn test_auto_transition_to_live() {
        let mut manager = ScrollManager::new("room = 'abc'", "timestamp", 600.0);
        manager.mode = ScrollMode::Forward;

        // Results less than limit while in forward mode = at latest
        manager.on_results(15, Some(1000), Some(2000));

        assert!(manager.at_latest());
        assert_eq!(manager.mode(), ScrollMode::Live);
    }

    #[test]
    fn test_jump_to_live() {
        let mut manager = ScrollManager::new("room = 'abc'", "timestamp", 600.0);
        manager.mode = ScrollMode::Backward;
        manager.selection.set_continuation(1000, LoadDirection::Backward);

        let selection = manager.jump_to_live();

        assert_eq!(manager.mode(), ScrollMode::Live);
        assert!(!selection.contains("1000"));
        assert!(selection.contains("ORDER BY timestamp DESC"));
    }

    #[test]
    fn test_filter_update_resets() {
        let mut manager = ScrollManager::new("room = 'abc'", "timestamp", 600.0);
        manager.mode = ScrollMode::Backward;
        manager.on_results(50, Some(1000), Some(2000));

        let selection = manager.update_filter("room = 'xyz'", true);

        assert_eq!(manager.mode(), ScrollMode::Live);
        assert!(selection.contains("room = 'xyz'"));
        assert!(!selection.contains("1000"));
    }

    #[test]
    fn test_filter_update_preserves() {
        let mut manager = ScrollManager::new("room = 'abc'", "timestamp", 600.0);
        manager.on_results(50, Some(1000), Some(2000));
        manager.selection.set_continuation(1000, LoadDirection::Backward);
        manager.mode = ScrollMode::Backward;

        let selection = manager.update_filter("room = 'abc' AND author = 'user1'", false);

        assert_eq!(manager.mode(), ScrollMode::Backward);
        assert!(selection.contains("timestamp <= 1000"));
        assert!(selection.contains("author = 'user1'"));
    }
}
