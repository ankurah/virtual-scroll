//! Windowing Math Module
//!
//! Pure functions for virtual scroll windowing calculations.
//! See specs/windowing.md for the full specification.

// ============================================================================
// Core Formulas
// ============================================================================

/// Maximum items that could be visible on one screen (worst case with minimum heights)
pub fn screen_items(viewport_height: u32, minimum_row_height: u32) -> usize {
    let items = (viewport_height as f64 / minimum_row_height as f64).ceil() as usize;
    items.max(1) // At least 1 item per screen
}

/// Window size for live mode: (2N + 1) * screen_items
pub fn live_window_size(screen_items: usize, threshold_screens: f64) -> usize {
    ((2.0 * threshold_screens + 1.0) * screen_items as f64).ceil() as usize
}

/// Window size for pagination: (4N + 1) * screen_items
pub fn full_window_size(screen_items: usize, threshold_screens: f64) -> usize {
    ((4.0 * threshold_screens + 1.0) * screen_items as f64).ceil() as usize
}

/// Trigger threshold in pixels: N * viewport_height
pub fn trigger_threshold_px(viewport_height: u32, threshold_screens: f64) -> u32 {
    (threshold_screens * viewport_height as f64).ceil() as u32
}

/// Continuation offset from passing-side end: N * screen_items
pub fn continuation_offset(screen_items: usize, threshold_screens: f64) -> usize {
    (threshold_screens * screen_items as f64).ceil() as usize
}

/// Minimum buffer before pagination triggers: N * screen_items
pub fn min_buffer(screen_items: usize, threshold_screens: f64) -> usize {
    (threshold_screens * screen_items as f64).ceil() as usize
}

// ============================================================================
// Trigger Logic
// ============================================================================

/// Direction of scroll/pagination
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Scrolling up toward older items
    Backward,
    /// Scrolling down toward newer items
    Forward,
}

/// Result of checking if pagination should trigger
#[derive(Debug, Clone, PartialEq)]
pub enum TriggerCheck {
    /// No pagination needed
    None,
    /// Should trigger pagination in the given direction
    Trigger(Direction),
}

/// Check if pagination should trigger based on scroll position
pub fn check_trigger(
    trigger_threshold_px: u32,
    top_gap_px: u32,
    bottom_gap_px: u32,
    scrolling_up: bool,
    at_earliest: bool,
    at_latest: bool,
) -> TriggerCheck {
    // Backward pagination: scrolling up, near top, not at earliest
    if scrolling_up && top_gap_px < trigger_threshold_px && !at_earliest {
        return TriggerCheck::Trigger(Direction::Backward);
    }

    // Forward pagination: scrolling down, near bottom, not at latest
    if !scrolling_up && bottom_gap_px < trigger_threshold_px && !at_latest {
        return TriggerCheck::Trigger(Direction::Forward);
    }

    TriggerCheck::None
}

/// Calculate the continuation index for pagination
///
/// Returns the index in the current item set where the continuation item should be selected.
pub fn continuation_index(
    continuation_offset: usize,
    current_len: usize,
    direction: Direction,
) -> usize {
    match direction {
        Direction::Backward => {
            // N screens from the bottom (newest end)
            current_len.saturating_sub(continuation_offset)
        }
        Direction::Forward => {
            // N screens from the top (oldest end), but as an index (0-based)
            continuation_offset.saturating_sub(1).min(current_len.saturating_sub(1))
        }
    }
}

// ============================================================================
// Test Utilities
// ============================================================================

/// Computed windowing parameters (for testing)
///
/// Groups all derived values for easy assertion in tests.
#[derive(Debug, Clone, PartialEq)]
pub struct WindowingParams {
    pub screen_items: usize,
    pub live_window_size: usize,
    pub full_window_size: usize,
    pub min_buffer: usize,
    pub trigger_threshold_px: u32,
    pub continuation_offset: usize,
}

impl WindowingParams {
    /// Compute all windowing parameters from raw inputs
    pub fn compute(
        viewport_height: u32,
        minimum_row_height: u32,
        threshold_screens: f64,
    ) -> Self {
        let screen_items = screen_items(viewport_height, minimum_row_height);
        Self {
            screen_items,
            live_window_size: live_window_size(screen_items, threshold_screens),
            full_window_size: full_window_size(screen_items, threshold_screens),
            min_buffer: min_buffer(screen_items, threshold_screens),
            trigger_threshold_px: trigger_threshold_px(viewport_height, threshold_screens),
            continuation_offset: continuation_offset(screen_items, threshold_screens),
        }
    }
}

/// Simulate what the new result set would look like after pagination
///
/// Given current items as (start_id, end_id) range and a continuation_id,
/// returns the expected (new_start_id, new_end_id) range.
///
/// This is for testing the math - assumes items are sequential IDs.
pub fn simulate_pagination(
    full_window_size: usize,
    _current_start_id: i64,
    _current_end_id: i64,
    continuation_id: i64,
    direction: Direction,
    total_items_in_db: i64,
) -> (i64, i64) {
    let window_size = full_window_size as i64;

    match direction {
        Direction::Backward => {
            // Query: id <= continuation_id ORDER BY id DESC LIMIT window_size
            let new_end = continuation_id;
            let new_start = (continuation_id - window_size + 1).max(0);
            (new_start, new_end)
        }
        Direction::Forward => {
            // Query: id >= continuation_id ORDER BY id ASC LIMIT window_size
            let new_start = continuation_id;
            let new_end = (continuation_id + window_size - 1).min(total_items_in_db - 1);
            (new_start, new_end)
        }
    }
}

/// Find the intersection between two ID ranges
///
/// Returns (intersection_start, intersection_end) or None if no overlap
pub fn find_intersection_range(
    old_start: i64,
    old_end: i64,
    new_start: i64,
    new_end: i64,
) -> Option<(i64, i64)> {
    let inter_start = old_start.max(new_start);
    let inter_end = old_end.min(new_end);

    if inter_start <= inter_end {
        Some((inter_start, inter_end))
    } else {
        None
    }
}

/// Calculate which item should be the anchor for scroll stability
///
/// Returns the ID of the anchor item (first or last of intersection based on direction)
pub fn select_anchor_id(
    intersection_start: i64,
    intersection_end: i64,
    direction: Direction,
) -> i64 {
    match direction {
        // For backward, use first (oldest) intersecting item
        Direction::Backward => intersection_start,
        // For forward, use last (newest) intersecting item
        Direction::Forward => intersection_end,
    }
}

/// Verify that visible items are preserved in the new set
pub fn visible_items_preserved(
    visible_start_id: i64,
    visible_end_id: i64,
    new_start_id: i64,
    new_end_id: i64,
) -> bool {
    visible_start_id >= new_start_id && visible_end_id <= new_end_id
}

/// Calculate buffer sizes after pagination
///
/// Returns (items_above_visible, items_below_visible)
pub fn calculate_buffers(
    visible_start_id: i64,
    visible_end_id: i64,
    new_start_id: i64,
    new_end_id: i64,
) -> (i64, i64) {
    let above = visible_start_id - new_start_id;
    let below = new_end_id - visible_end_id;
    (above.max(0), below.max(0))
}
