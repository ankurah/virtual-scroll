//! Selection string builder for paginated queries

use crate::LoadDirection;

/// Continuation state for pagination
#[derive(Clone, Debug)]
struct Continuation {
    /// Timestamp anchor for pagination
    anchor_timestamp: i64,
    /// Direction of pagination
    direction: LoadDirection,
}

/// Builds selection strings for paginated queries
#[derive(Clone, Debug)]
pub struct PaginatedSelection {
    /// Base predicate (e.g., "room = 'abc' AND deleted = false")
    base_predicate: String,
    /// Timestamp field name (e.g., "timestamp")
    timestamp_field: String,
    /// Maximum results per query
    limit: usize,
    /// Current pagination state
    continuation: Option<Continuation>,
}

impl PaginatedSelection {
    /// Create a new paginated selection
    pub fn new(base_predicate: &str, timestamp_field: &str, limit: usize) -> Self {
        Self {
            base_predicate: base_predicate.to_string(),
            timestamp_field: timestamp_field.to_string(),
            limit,
            continuation: None,
        }
    }

    /// Get the current limit
    pub fn limit(&self) -> usize {
        self.limit
    }

    /// Set the limit
    pub fn set_limit(&mut self, limit: usize) {
        self.limit = limit;
    }

    /// Update the base predicate
    ///
    /// If `reset_continuation` is true, clears any pagination state.
    /// If false, preserves the current scroll position.
    pub fn update_base(&mut self, predicate: &str, reset_continuation: bool) {
        self.base_predicate = predicate.to_string();
        if reset_continuation {
            self.continuation = None;
        }
    }

    /// Set pagination continuation
    pub fn set_continuation(&mut self, timestamp: i64, direction: LoadDirection) {
        self.continuation = Some(Continuation {
            anchor_timestamp: timestamp,
            direction,
        });
    }

    /// Clear pagination (return to live mode)
    pub fn clear_continuation(&mut self) {
        self.continuation = None;
    }

    /// Check if there's an active continuation
    pub fn has_continuation(&self) -> bool {
        self.continuation.is_some()
    }

    /// Build the full selection string
    ///
    /// Returns a selection string like:
    /// - Live: `"{base} ORDER BY {ts} DESC LIMIT {limit}"`
    /// - Backward: `"{base} AND {ts} <= {anchor} ORDER BY {ts} DESC LIMIT {limit}"`
    /// - Forward: `"{base} AND {ts} >= {anchor} ORDER BY {ts} ASC LIMIT {limit}"`
    pub fn build(&self) -> String {
        let mut query = self.base_predicate.clone();

        // Add continuation clause if present
        if let Some(ref cont) = self.continuation {
            let op = match cont.direction {
                LoadDirection::Backward => "<=",
                LoadDirection::Forward => ">=",
            };
            query.push_str(&format!(
                " AND {} {} {}",
                self.timestamp_field, op, cont.anchor_timestamp
            ));
        }

        // Add ORDER BY clause
        let order = match self.continuation.as_ref().map(|c| c.direction) {
            Some(LoadDirection::Forward) => "ASC",
            _ => "DESC", // Live and Backward use DESC
        };
        query.push_str(&format!(
            " ORDER BY {} {} LIMIT {}",
            self.timestamp_field, order, self.limit
        ));

        query
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_live_mode_selection() {
        let sel = PaginatedSelection::new("room = 'abc' AND deleted = false", "timestamp", 50);
        assert_eq!(
            sel.build(),
            "room = 'abc' AND deleted = false ORDER BY timestamp DESC LIMIT 50"
        );
    }

    #[test]
    fn test_backward_pagination() {
        let mut sel = PaginatedSelection::new("room = 'abc'", "timestamp", 50);
        sel.set_continuation(1704067200000, LoadDirection::Backward);
        assert_eq!(
            sel.build(),
            "room = 'abc' AND timestamp <= 1704067200000 ORDER BY timestamp DESC LIMIT 50"
        );
    }

    #[test]
    fn test_forward_pagination() {
        let mut sel = PaginatedSelection::new("room = 'abc'", "timestamp", 50);
        sel.set_continuation(1704067200000, LoadDirection::Forward);
        assert_eq!(
            sel.build(),
            "room = 'abc' AND timestamp >= 1704067200000 ORDER BY timestamp ASC LIMIT 50"
        );
    }

    #[test]
    fn test_update_base_preserves_continuation() {
        let mut sel = PaginatedSelection::new("room = 'abc'", "timestamp", 50);
        sel.set_continuation(1704067200000, LoadDirection::Backward);
        sel.update_base("room = 'abc' AND author = 'user1'", false);

        let result = sel.build();
        assert!(result.contains("timestamp <= 1704067200000"));
        assert!(result.contains("author = 'user1'"));
    }

    #[test]
    fn test_update_base_resets_continuation() {
        let mut sel = PaginatedSelection::new("room = 'abc'", "timestamp", 50);
        sel.set_continuation(1704067200000, LoadDirection::Backward);
        sel.update_base("room = 'xyz'", true);

        let result = sel.build();
        assert!(!result.contains("1704067200000"));
        assert!(result.contains("room = 'xyz'"));
    }
}
