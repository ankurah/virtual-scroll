# Virtual Scroll Specification

## Vision

Virtual Scroll is a platform-agnostic Rust library for managing paginated scroll state in reactive applications. It provides a pure state machine that handles bidirectional pagination with timestamp-based continuation, designed to work with any reactive query system (like Ankurah's LiveQuery).

## Problem Statement

Modern chat and feed applications need to:
1. Display large datasets efficiently (virtual scrolling)
2. Paginate in both directions (older/newer content)
3. **Maintain pixel-perfect scroll position stability across pagination**
4. Integrate with reactive query systems
5. Work across multiple platforms (web, mobile)

Current solutions duplicate pagination logic in TypeScript for each platform, leading to:
- Inconsistent behavior between platforms
- Difficult-to-test business logic embedded in UI code
- No unit testing without full UI integration

## The Hard Problem: Scroll Position Stability

When loading new items during pagination, the visible items must NOT jump. If the user is looking at messages [A, B, C, D, E] and scrolls up to trigger loading older messages [X, Y, Z], after the load completes they should still see [A, B, C, D, E] in exactly the same positions, with [X, Y, Z] above (off-screen).

This requires:
1. **Anchor-based continuation**: Pick an anchor item that exists in BOTH the old and new result sets
2. **Position measurement**: Record anchor's Y position BEFORE query update
3. **Scroll adjustment**: After new items render, measure anchor's new Y position, adjust scrollTop by delta

### Why This Is Hard

- **Variable item heights**: Items can have different heights (text length, images, etc.)
- **Platform-specific measurement**: Only the DOM/FlatList knows rendered positions
- **Timing**: Measurement must happen before/after render at correct moments
- **Overlap guarantee**: The anchor item MUST exist in the new result set

## Architecture: Separation of Concerns

### Rust Core Responsibilities
- Query construction (base predicate + continuation + ordering + limit)
- Mode tracking (Live / Backward / Forward)
- Boundary detection (at earliest/latest based on result count vs limit)
- Providing configuration (thresholds, limits)

### Platform Layer Responsibilities (TypeScript)
- DOM/FlatList binding and scroll event handling
- **Anchor selection**: Measuring DOM to pick anchor item for continuation
- **Position measurement**: Recording anchor position before/after query update
- **Scroll adjustment**: Applying scrollTop correction to maintain stability
- Spacer management (leading/trailing padding)

### Key Insight

**Rust does NOT pick the anchor.** The platform layer picks the anchor based on rendered positions, then tells Rust which timestamp to use for the continuation query.

## Spacer Management

### Leading Spacer (Top)
The leading spacer pushes content down in the container. It:
- Allows scrolling "above" the loaded items
- Creates room for items that will load on backward pagination
- **Must be variable**: Adjusted when items are added above to maintain stability

When items are added above the viewport, the leading spacer is REDUCED by the height of those items (plus scroll adjustment). This keeps the visible items stationary.

### Trailing Spacer (Bottom)
- **Live mode**: No trailing spacer needed (always at latest)
- **Non-live mode**: Small fixed spacer to allow scrolling past loaded items
- Only needed when not at the dataset boundary AND not in live mode

### Edge Case: At Dataset Boundary
When at the earliest item (no more older content):
- Leading spacer can be zero or minimal
- No backward loading possible

When at the latest item (no more newer content) while not in live mode:
- Should transition to live mode automatically
- Trailing spacer removed

## Core Types

### ScrollMode
```rust
pub enum ScrollMode {
    Live,      // At latest, receiving real-time updates (DESC query)
    Backward,  // Paginating to older content (DESC query with timestamp <= anchor)
    Forward,   // Paginating to newer content (ASC query with timestamp >= anchor)
}
```

### LoadRequest (Rust → Platform)
When `on_scroll` triggers a load, Rust returns information for the platform:
```rust
pub struct LoadRequest {
    /// Direction of pagination
    pub direction: LoadDirection,
    /// How far (in pixels) from the viewport edge to pick the anchor
    /// Platform should find an item at this offset and use its timestamp
    pub anchor_offset: f64,
}
```

### AnchorResult (Platform → Rust)
After platform picks an anchor, it calls Rust with:
```rust
pub struct AnchorResult {
    /// Timestamp of the chosen anchor item
    pub anchor_timestamp: i64,
    /// Anchor's Y position relative to container (before query update)
    pub anchor_y_before: f64,
}
```

### ScrollAdjustment (Rust → Platform after results)
After results arrive:
```rust
pub struct ScrollAdjustment {
    /// Change scrollTop by this amount to maintain anchor position
    pub scroll_delta: f64,
    /// New leading spacer height
    pub leading_spacer: f64,
    /// New trailing spacer height (usually 0 in live mode)
    pub trailing_spacer: f64,
}
```

**Note**: The exact API design for `ScrollAdjustment` needs refinement. Since Rust doesn't know rendered heights, the platform layer may need to compute scroll_delta itself based on anchor position delta.

## Pagination Flow

### Backward Pagination (Scrolling Up)
```
1. User scrolls up → scroll position approaches top
2. Rust on_scroll(): topGap < minBuffer? Return LoadRequest{direction: Backward, anchor_offset}
3. Platform: Find anchor item at anchor_offset BELOW viewport bottom
4. Platform: Record anchor.y position
5. Platform: Call Rust load_with_anchor(anchor_timestamp)
6. Rust: Build query "... AND timestamp <= {anchor_ts} ORDER BY DESC LIMIT N"
7. Rust: Return selection string → Platform updates LiveQuery
8. LiveQuery fires subscription → new items render (anchor still exists, but shifted down)
9. Platform: Find anchor again, measure new anchor.y
10. Platform: scrollTop += (anchor.y_after - anchor.y_before)
11. Result: Visible items stay in place, new items appear above
```

### Forward Pagination (Scrolling Down from non-live mode)
```
1. User scrolls down → scroll position approaches bottom
2. Rust on_scroll(): bottomGap < minBuffer? Return LoadRequest{direction: Forward, anchor_offset}
3. Platform: Find anchor item at anchor_offset ABOVE viewport top
4. Platform: Record anchor.y position
5. Platform: Call Rust load_with_anchor(anchor_timestamp)
6. Rust: Build query "... AND timestamp >= {anchor_ts} ORDER BY ASC LIMIT N"
7. Rust: Return selection string → Platform updates LiveQuery
8. LiveQuery fires → new items render
9. Platform: Measure anchor delta, adjust scroll
10. If at_latest → Rust auto-transitions to Live mode
```

## Configuration

```rust
pub struct ScrollConfig {
    /// Trigger loading when this fraction of viewport from edge (default: 0.75)
    /// 0.75 = trigger when 75% of a viewport height from the edge
    pub min_buffer_ratio: f64,

    /// How far inside the viewport to pick the anchor (default: 0.75)
    /// This ensures overlap between old and new result sets
    pub anchor_offset_ratio: f64,

    /// Load this many viewports worth of content (default: 3.0)
    pub query_size_ratio: f64,

    /// Estimated row height for limit calculation (default: 74.0)
    pub estimated_row_height: f64,
}
```

## API Design

### Core Manager
```rust
impl ScrollManager {
    pub fn new(base_predicate: &str, timestamp_field: &str, viewport_height: f64) -> Self;

    /// Process scroll event. Returns LoadRequest if pagination should trigger.
    pub fn on_scroll(&mut self, input: ScrollInput) -> Option<LoadRequest>;

    /// Platform picked an anchor, build the continuation query.
    /// Returns the new selection string.
    pub fn load_with_anchor(&mut self, anchor_timestamp: i64, direction: LoadDirection) -> String;

    /// After results arrive, update boundary state.
    pub fn on_results(&mut self, count: usize, oldest_ts: Option<i64>, newest_ts: Option<i64>);

    /// Jump to live mode. Returns new selection string.
    pub fn jump_to_live(&mut self) -> String;

    /// Update base predicate (filter change). Returns new selection string.
    pub fn update_filter(&mut self, predicate: &str, reset_continuation: bool) -> String;

    // State getters
    pub fn mode(&self) -> ScrollMode;
    pub fn at_earliest(&self) -> bool;
    pub fn at_latest(&self) -> bool;
    pub fn should_auto_scroll(&self) -> bool;
    pub fn should_reverse_for_display(&self) -> bool;
}
```

### Platform Integration Pattern

```typescript
class ScrollManagerWrapper {
    private scrollManager: MessageScrollManager;
    private liveQuery: MessageLiveQuery;
    private pendingAnchor: { id: string, yBefore: number } | null = null;

    onScroll(event) {
        const loadRequest = this.scrollManager.onScroll({
            offset: container.scrollTop,
            content_height: container.scrollHeight,
            viewport_height: container.clientHeight,
            scroll_delta: delta,
            user_initiated: isUserScroll
        });

        if (loadRequest) {
            // Pick anchor based on rendered positions
            const anchor = this.pickAnchor(loadRequest.direction, loadRequest.anchor_offset);
            if (anchor) {
                this.pendingAnchor = {
                    id: anchor.item.id,
                    yBefore: anchor.element.getBoundingClientRect().top
                };

                // Tell Rust to build query with this anchor
                const selection = this.scrollManager.loadWithAnchor(
                    anchor.item.timestamp,
                    loadRequest.direction
                );
                this.liveQuery.updateSelection(selection);
            }
        }
    }

    onLiveQueryChange() {
        // After render, adjust scroll to maintain anchor position
        if (this.pendingAnchor) {
            const anchorEl = document.querySelector(`[data-id="${this.pendingAnchor.id}"]`);
            if (anchorEl) {
                const yAfter = anchorEl.getBoundingClientRect().top;
                const delta = yAfter - this.pendingAnchor.yBefore;
                container.scrollTop += delta;
            }
            this.pendingAnchor = null;
        }

        // Update boundary state
        const items = this.scrollManager.items;
        // ... extract oldest/newest timestamps ...
        this.scrollManager.onResults(items.length, oldest, newest);
    }

    pickAnchor(direction: 'backward' | 'forward', offset: number) {
        // For backward: find item `offset` pixels below viewport bottom
        // For forward: find item `offset` pixels above viewport top
        // ... DOM measurement logic ...
    }
}
```

## Test Strategy

### Unit Tests (Rust Core)
1. **Query construction**: Verify selection strings for all modes
2. **Mode transitions**: Live → Backward → Forward → Live
3. **Boundary detection**: count < limit detection
4. **LoadRequest generation**: Correct thresholds trigger loads
5. **No double-loading**: on_scroll returns None while loading

### CRITICAL: Pixel-Perfect Boundary Tests

These tests validate the core promise of scroll position stability. They MUST pass before any template integration.

**Test: Backward Pagination at Exact Threshold**
```
1. Mock viewport with items at known Y positions [A@100, B@200, C@300, D@400, E@500]
2. Configure trigger threshold at topGap < 150px
3. Scroll to topGap = 151px (1 pixel BEFORE trigger)
4. Record exact Y position of each item
5. Scroll 1 more pixel (topGap = 150px, triggers backward load)
6. New items [X, Y, Z] load and render
7. Scroll adjustment applied
8. ASSERT: Item A is now at Y = 101 (exactly 1 pixel difference)
9. ASSERT: Item B is now at Y = 201 (exactly 1 pixel difference)
10. ASSERT: No visible item jumped more than 1 pixel
```

**Test: Forward Pagination at Exact Threshold**
```
Same structure but for forward (scrolling down) pagination.
```

**Test: Variable Height Items**
```
Items with varying heights (50px, 120px, 74px, 200px) must still
maintain pixel-perfect stability using anchor-based adjustment.
```

### Integration Tests (With Mock Platform)
1. **Anchor-based pagination**: Verify timestamp is used correctly
2. **Display ordering**: Items always in chronological order
3. **Filter changes**: Continuation preserved/reset correctly

### Platform Tests (TypeScript)
1. **Anchor selection**: Correct element picked for direction
2. **Scroll adjustment**: Position maintained after pagination
3. **Spacer updates**: Leading spacer changes correctly

## Platform-Specific Approaches (Resolved)

### React Native
React Native has native `maintainVisibleContentPosition` prop on ScrollView/FlatList that handles scroll stability automatically. We do NOT need anchor-based logic in the RN wrapper.

```jsx
<FlatList
  maintainVisibleContentPosition={{
    minIndexForVisible: 0,
    autoscrollToTopThreshold: 10,
  }}
/>
```

**Known issues:**
- Rapid updates (<200ms) may cause jumps in RN 0.81.1+
- Animated.View interactions can cause issues
- May behave differently with `inverted={true}`

### Browser
Must implement anchor-based scroll adjustment manually:
1. Platform picks anchor element before query update
2. Records anchor Y position
3. After render, measures new anchor Y position
4. Applies scrollTop adjustment

## Design Decisions (Resolved)

### Rapid Scrolling / Concurrent Loads
**Decision**: Do NOT block loads. Issue `update_selection()` as needed.

Ankurah's `update_selection` handles rapid updates intelligently:
- Sends updated predicates to server immediately
- Applies results monotonically (only latest wins)
- Concurrent requests are safe

The scroll manager should:
- Issue `update_selection` when thresholds are crossed
- Design thresholds to avoid gratuitous triggers (not every pixel)
- Let Ankurah handle concurrency - don't implement blocking/queueing

### Browser Scroll Anchoring (CSS overflow-anchor)
**Decision**: Not a core design question.

The manual anchor-based approach works regardless of CSS support. `overflow-anchor` could be an optimization/fallback but the core implementation shouldn't depend on it. Whether to leverage it is a platform integration detail.

### Spacers
**Decision**: Defer until platform integration.

Focus on core query/state management first. Spacer implementation will be determined during platform integration based on what works best for each platform (CSS divs, padding, FlatList header/footer components, etc.)

## Open Questions

1. **Inverted FlatList**: Chat UIs often use `inverted={true}`. How does this interact with `maintainVisibleContentPosition`? Need to test during RN integration.

2. **Threshold tuning**: What values for `min_buffer_ratio` and `anchor_offset_ratio` avoid gratuitous triggers while maintaining smooth UX? May need empirical testing.
