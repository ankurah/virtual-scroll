# ankurah-virtual-scroll Technical Specification

## Vision

A platform-agnostic Rust library for managing paginated scroll state in reactive applications. It provides a pure state machine that handles bidirectional pagination with timestamp-based continuation, designed to work with Ankurah's LiveQuery.

## Problem Statement

Modern chat and feed applications need to:
1. Display large datasets efficiently (virtual scrolling)
2. Paginate in both directions (older/newer content)
3. Maintain pixel-perfect scroll position stability across pagination
4. Integrate with reactive query systems
5. Work across multiple platforms (web, mobile)

## The Hard Problem: Scroll Position Stability

When loading new items during pagination, the visible items must NOT jump. If the user is looking at messages [A, B, C, D, E] and scrolls up to trigger loading older messages [X, Y, Z], after the load completes they should still see [A, B, C, D, E] in exactly the same positions, with [X, Y, Z] above (off-screen).

This requires:
1. **Anchor-based continuation**: Pick an anchor item that exists in BOTH the old and new result sets
2. **Position measurement**: Record anchor's Y position BEFORE query update
3. **Scroll adjustment**: After new items render, measure anchor's new Y position, adjust scrollTop by delta

---

## Windowing Algorithm

### Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `viewport_height` | i32 | Pixel height of the visible scroll area |
| `min_row_height` | i32 | Guaranteed minimum height of any item |
| `buffer_factor` | f64 | Multiplier for buffer size (default 2.0) |

### Derived Values

```
screen_items = viewport_height / min_row_height    // max items that fit on screen
buffer = screen_items * buffer_factor              // items to keep beyond visible
threshold = screen_items                           // trigger when buffer hits this
live_window = screen_items + buffer                // initial window size
```

**Example** (viewport=500px, min_row_height=50px, buffer_factor=2.0):
- screen_items = 10
- buffer = 20
- threshold = 10
- live_window = 30

### Modes

**Live Mode**: At the newest edge, receiving real-time updates. Window contains the newest `live_window` items. Auto-scrolls to bottom on new items.

**Backward Mode**: User scrolled toward older items. Window expands/slides to load older content. No auto-scroll.

**Forward Mode**: User scrolling back toward newer items. When reaching the live edge, transitions back to Live mode.

### Trigger Conditions

Pagination triggers when the buffer on the scroll-toward side drops to one screenful:

```
backward_trigger: items_above <= screen_items
forward_trigger:  items_below <= screen_items
```

### Query Construction

**Backward (loading older)**:
```sql
WHERE timestamp <= cursor_value ORDER BY timestamp DESC LIMIT window_size
```
- Cursor: timestamp of the item `buffer` positions from the newest visible
- Intersection anchor: the newest visible item

**Forward (loading newer)**:
```sql
WHERE timestamp >= cursor_value ORDER BY timestamp ASC LIMIT window_size
```
- Cursor: timestamp of the item `buffer` positions from the oldest visible
- Intersection anchor: the oldest visible item

---

## Architecture: Separation of Concerns

### Rust Core Responsibilities
- Query construction (base predicate + continuation + ordering + limit)
- Mode tracking (Live / Backward / Forward)
- Boundary detection (at earliest/latest based on result count vs limit)
- Providing configuration (thresholds, limits)

### Platform Layer Responsibilities (TypeScript/Swift/Kotlin)
- DOM/FlatList binding and scroll event handling
- **Anchor selection**: Measuring rendered positions to pick anchor item
- **Position measurement**: Recording anchor position before/after query update
- **Scroll adjustment**: Applying scrollTop correction to maintain stability
- Spacer management (leading/trailing padding)

### Key Insight

**Rust does NOT pick the anchor.** The platform layer picks the anchor based on rendered positions, then tells Rust which timestamp to use for the continuation query.

---

## Core Types

### ScrollMode
```rust
pub enum ScrollMode {
    Live,      // At latest, receiving real-time updates
    Backward,  // Paginating to older content
    Forward,   // Paginating to newer content
}
```

### LoadDirection
```rust
pub enum LoadDirection {
    Backward,  // Load items preceding current window
    Forward,   // Load items following current window
}
```

### VisibleSet
```rust
pub struct VisibleSet<V> {
    pub items: Vec<V>,
    pub intersection: Option<Intersection>,
    pub has_more_preceding: bool,
    pub has_more_following: bool,
    pub should_auto_scroll: bool,
    pub error: Option<String>,
}
```

### Intersection
```rust
pub struct Intersection {
    pub entity_id: EntityId,
    pub index: usize,
    pub direction: LoadDirection,
}
```

---

## Intersection Anchoring

To maintain scroll stability when the window changes:

1. **Before update**: Record the intersection item (newest-visible for backward, oldest-visible for forward)
2. **After update**: Find the same item in the new window by ID
3. **Adjust scroll**: Position viewport so the anchor item appears at the same relative position

---

## Platform-Specific Approaches

### React Native
React Native has native `maintainVisibleContentPosition` prop on ScrollView/FlatList that handles scroll stability automatically. The RN wrapper is simpler - just enable this prop.

```jsx
<FlatList
  maintainVisibleContentPosition={{
    minIndexForVisible: 0,
    autoscrollToTopThreshold: 10,
  }}
/>
```

### Browser
Must implement anchor-based scroll adjustment manually:
1. Platform picks anchor element before query update
2. Records anchor Y position
3. After render, measures new anchor Y position
4. Applies scrollTop adjustment

---

## API Design

### ScrollManager

```rust
impl ScrollManager<V> {
    pub fn new(
        ctx: &Context,
        predicate: impl TryInto<Predicate>,
        display_order: impl IntoOrderBy,
        minimum_row_height: u32,
        buffer_factor: f64,
        viewport_height: u32,
    ) -> Result<Self, RetrievalError>;

    pub async fn start(&self);

    pub fn visible_set(&self) -> Read<VisibleSet<V>>;
    pub fn mode(&self) -> ScrollMode;
    pub fn current_selection(&self) -> String;

    pub fn on_scroll(
        &self,
        first_visible: EntityId,
        last_visible: EntityId,
        scrolling_backward: bool,
    );
}
```

### Generated Wrapper (via macro)

```rust
generate_scroll_manager!(
    Message,           // Model type
    MessageView,       // View type
    MessageLiveQuery,  // LiveQuery type
    timestamp_field = "timestamp"
);
```

This generates `MessageScrollManager` with platform-specific bindings (WASM or UniFFI).

---

## Window Sizing

| Scenario | Window Size |
|----------|-------------|
| Live mode | `screen + buffer` (e.g., 30) |
| After backward pagination | `screen + 2*buffer` (e.g., 50) |
| Maximum (middle of history) | `screen + 2*buffer` (e.g., 50) |

## Edge Behavior

### At Oldest Edge
- `has_more_preceding = false`
- Backward pagination stops
- Forward pagination can still trigger

### At Newest Edge
- `has_more_following = false`
- Transitions to Live mode
- `should_auto_scroll = true`

---

## Design Decisions

### Rapid Scrolling / Concurrent Loads
**Decision**: Do NOT block loads. Issue `update_selection()` as needed.

Ankurah's `update_selection` handles rapid updates intelligently:
- Sends updated predicates to server immediately
- Applies results monotonically (only latest wins)
- Concurrent requests are safe

### Browser Scroll Anchoring (CSS overflow-anchor)
**Decision**: Not a core design concern.

The manual anchor-based approach works regardless of CSS support. `overflow-anchor` could be an optimization but the core implementation doesn't depend on it.

### Spacers
**Decision**: Implemented per platform.

Leading spacer creates room for backward-loaded items. Trailing spacer (non-live mode) allows scrolling past loaded items. Implementation varies by platform (CSS divs, FlatList headers, etc.).
