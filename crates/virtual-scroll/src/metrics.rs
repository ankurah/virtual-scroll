//! Scroll metrics and mode types

/// The current scroll/pagination mode
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ScrollMode {
    /// At latest position, receiving real-time updates
    #[default]
    Live,
    /// Paginating to older content (scrolling up)
    Backward,
    /// Paginating to newer content (scrolling down, returning to live)
    Forward,
}

/// Direction to load more content
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LoadDirection {
    /// Load older content
    Backward,
    /// Load newer content
    Forward,
}

/// Input from the scroll container (platform-agnostic)
#[derive(Clone, Copy, Debug, Default)]
pub struct ScrollInput {
    /// Current scroll offset from top
    pub offset: f64,
    /// Total scrollable content height
    pub content_height: f64,
    /// Visible viewport height
    pub viewport_height: f64,
    /// Change in offset since last event (negative = scrolling up)
    pub scroll_delta: f64,
    /// True if user is actively dragging (not momentum/programmatic)
    pub user_initiated: bool,
}

/// Output metrics for debug UI
#[derive(Clone, Copy, Debug, Default)]
pub struct ScrollMetrics {
    /// Distance from viewport top to content start
    pub top_gap: f64,
    /// Distance from viewport bottom to content end
    pub bottom_gap: f64,
    /// Threshold that triggers loading (viewport_height * buffer_ratio)
    pub min_buffer: f64,
    /// Current result count from query
    pub result_count: usize,
}

/// Request from scroll manager for platform to trigger a load
///
/// When `on_scroll` returns this, the platform should:
/// 1. Pick an anchor item at approximately `anchor_offset` pixels from the relevant edge
/// 2. Record the anchor's Y position
/// 3. Call `load_with_anchor(anchor_timestamp, direction)`
#[derive(Clone, Copy, Debug)]
pub struct LoadRequest {
    /// Direction of pagination
    pub direction: LoadDirection,
    /// Suggested offset from viewport edge to pick anchor (in pixels)
    /// For backward: offset from bottom of viewport
    /// For forward: offset from top of viewport
    pub anchor_offset: f64,
}
