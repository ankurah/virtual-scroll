# Virtual Scroll Implementation Plan

## Crate Structure

```
virtual-scroll/
├── Cargo.toml                  # Workspace root
├── SPEC.md                     # Overarching vision
├── PLAN.md                     # This file - implementation details
├── TASKS.md                    # Task tracking
├── crates/
│   ├── virtual-scroll/         # Core crate (no dependencies)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs          # Public API
│   │       ├── metrics.rs      # ScrollMode, ScrollInput, ScrollMetrics
│   │       ├── query.rs        # PaginatedSelection builder
│   │       └── manager.rs      # ScrollManager state machine
│   └── virtual-scroll-derive/  # Derive macro crate
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs          # #[derive(VirtualScroll)]
│           ├── uniffi.rs       # UniFFI wrapper generation
│           └── wasm.rs         # WASM wrapper generation
└── tests/                      # Integration tests
    ├── query_tests.rs
    └── pagination_tests.rs
```

## Core Types

### metrics.rs

```rust
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ScrollMode {
    #[default]
    Live,      // At latest, real-time updates
    Backward,  // Paginating to older content
    Forward,   // Paginating to newer content
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LoadDirection {
    Backward,
    Forward,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ScrollInput {
    pub offset: f64,
    pub content_height: f64,
    pub viewport_height: f64,
    pub scroll_delta: f64,
    pub user_initiated: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ScrollMetrics {
    pub top_gap: f64,
    pub bottom_gap: f64,
    pub min_buffer: f64,
    pub result_count: usize,
}
```

### query.rs

```rust
pub struct PaginatedSelection {
    base_predicate: String,
    timestamp_field: String,
    limit: usize,
    continuation: Option<Continuation>,
}

struct Continuation {
    anchor_timestamp: i64,
    direction: LoadDirection,
}

impl PaginatedSelection {
    pub fn new(base: &str, ts_field: &str, limit: usize) -> Self;
    pub fn update_base(&mut self, predicate: &str, reset_continuation: bool);
    pub fn set_continuation(&mut self, ts: i64, dir: LoadDirection);
    pub fn clear_continuation(&mut self);
    pub fn build(&self) -> String;
}
```

### manager.rs

```rust
pub struct ScrollManager {
    mode: ScrollMode,
    loading: Option<LoadDirection>,
    selection: PaginatedSelection,
    metrics: ScrollMetrics,
    at_earliest: bool,
    at_latest: bool,
    // Config
    min_buffer_ratio: f64,
    query_size_ratio: f64,
    estimated_row_height: f64,
    viewport_height: f64,
}

impl ScrollManager {
    pub fn new(base_predicate: &str, ts_field: &str, viewport_height: f64) -> Self;
    pub fn on_scroll(&mut self, input: ScrollInput) -> Option<String>;
    pub fn on_results(&mut self, count: usize, oldest_ts: Option<i64>, newest_ts: Option<i64>);
    pub fn update_filter(&mut self, predicate: &str, reset: bool) -> String;
    pub fn jump_to_live(&mut self) -> String;
    pub fn selection(&self) -> String;
    pub fn should_auto_scroll(&self) -> bool;
    // Getters
    pub fn mode(&self) -> ScrollMode;
    pub fn is_loading(&self) -> bool;
    pub fn metrics(&self) -> &ScrollMetrics;
    pub fn at_earliest(&self) -> bool;
    pub fn at_latest(&self) -> bool;
}
```

## Derive Macro Design

### Usage

```rust
use virtual_scroll_derive::VirtualScroll;

#[derive(Model, VirtualScroll)]
#[virtual_scroll(timestamp_field = "timestamp")]
pub struct Message { ... }
```

### Generated Code (UniFFI)

```rust
#[derive(uniffi::Object)]
pub struct MessageScrollManager {
    core: Mutex<ScrollManager>,
    live_query: Arc<MessageLiveQuery>,
}

#[uniffi::export]
impl MessageScrollManager {
    #[uniffi::constructor]
    pub fn new(live_query: Arc<MessageLiveQuery>, viewport_height: f64) -> Arc<Self>;

    pub fn on_scroll(&self, offset: f64, content_height: f64, viewport_height: f64,
                     scroll_delta: f64, user_initiated: bool);

    pub fn items(&self) -> Vec<Arc<MessageView>>;
    pub fn jump_to_live(&self);
    pub fn update_filter(&self, predicate: String, reset: bool);

    pub fn mode(&self) -> String;
    pub fn should_auto_scroll(&self) -> bool;
}
```

### Generated Code (WASM)

```rust
#[wasm_bindgen]
pub struct MessageScrollManager {
    core: RefCell<ScrollManager>,
    live_query: MessageLiveQuery,
}

#[wasm_bindgen]
impl MessageScrollManager {
    #[wasm_bindgen(constructor)]
    pub fn new(live_query: MessageLiveQuery, viewport_height: f64) -> Self;

    #[wasm_bindgen(js_name = onScroll)]
    pub fn on_scroll(&self, offset: f64, content_height: f64, viewport_height: f64,
                     scroll_delta: f64, user_initiated: bool);

    #[wasm_bindgen(getter)]
    pub fn items(&self) -> Vec<MessageView>;

    pub fn jump_to_live(&self);

    pub fn mode(&self) -> String;
    pub fn should_auto_scroll(&self) -> bool;
}
```

## State Machine Logic

### Pagination Trigger

```
on_scroll(input):
    if !user_initiated: return None

    top_gap = input.offset
    bottom_gap = content_height - offset - viewport_height
    min_buffer = viewport_height * min_buffer_ratio

    if scroll_delta < 0 && top_gap < min_buffer && !at_earliest && !loading:
        loading = Some(Backward)
        selection.set_continuation(oldest_timestamp, Backward)
        return Some(selection.build())

    if scroll_delta > 0 && bottom_gap < min_buffer && !at_latest && !loading:
        loading = Some(Forward)
        selection.set_continuation(newest_timestamp, Forward)
        return Some(selection.build())

    return None
```

### Boundary Detection

```
on_results(count, oldest_ts, newest_ts):
    loading = None

    // Store timestamps for continuation anchors
    self.oldest_timestamp = oldest_ts
    self.newest_timestamp = newest_ts

    // Boundary detection: fewer results than limit = hit boundary
    at_boundary = count < selection.limit

    match mode:
        Live | Backward => at_earliest = at_boundary
        Forward => at_latest = at_boundary

    // Auto-transition to live when reaching latest
    if mode == Forward && at_latest:
        jump_to_live()
```

### Selection String Building

```
build():
    let mut query = base_predicate.clone()

    if let Some(cont) = &continuation:
        let op = match cont.direction:
            Backward => "<="
            Forward => ">="
        query += format!(" AND {} {} {}", timestamp_field, op, cont.anchor_timestamp)

    let order = match continuation.map(|c| c.direction):
        Some(Forward) => "ASC"
        _ => "DESC"

    query += format!(" ORDER BY {} {} LIMIT {}", timestamp_field, order, limit)
    query
```

## Integration Pattern

### TypeScript (React Native)

```typescript
// In Chat.tsx
const liveQuery = await messageOps.query(ctx, scrollManager.selection(), []);
const scrollManager = new MessageScrollManager(liveQuery, viewportHeight);

// FlatList handlers
onScroll={(e) => {
    const { contentOffset, contentSize, layoutMeasurement } = e.nativeEvent;
    scrollManager.onScroll(
        contentOffset.y,
        contentSize.height,
        layoutMeasurement.height,
        contentOffset.y - lastOffset,
        userScrolling
    );
}}

// Render
<FlatList
    data={scrollManager.items()}
    // ...
/>
```

### TypeScript (Browser)

```typescript
// Same API, different platform
const scrollManager = new MessageScrollManager(liveQuery, container.clientHeight);

container.onscroll = () => {
    scrollManager.onScroll(
        container.scrollTop,
        container.scrollHeight,
        container.clientHeight,
        container.scrollTop - lastScrollTop,
        userScrolling
    );
};
```

## Test Strategy

### Unit Tests (Core)

1. **Query construction** - Verify selection strings are built correctly
2. **Pagination triggers** - Test threshold detection
3. **Boundary detection** - Test when count < limit
4. **Mode transitions** - Live → Backward → Forward → Live
5. **Filter updates** - With/without continuation reset

### Integration Tests

1. **Message ordering** - Verify display order is always chronological
2. **Rapid scrolling** - Multiple scroll events don't trigger duplicate loads
3. **Filter change** - Continuation preserved/reset correctly

## Configuration

```rust
impl ScrollManager {
    // Default configuration
    const DEFAULT_MIN_BUFFER_RATIO: f64 = 0.75;  // Trigger at 75% from edge
    const DEFAULT_QUERY_SIZE_RATIO: f64 = 3.0;   // Load 3 viewports worth
    const DEFAULT_ROW_HEIGHT: f64 = 74.0;        // Estimated row height for limit calc

    pub fn with_config(
        base_predicate: &str,
        timestamp_field: &str,
        viewport_height: f64,
        min_buffer_ratio: f64,
        query_size_ratio: f64,
        estimated_row_height: f64,
    ) -> Self;
}
```
