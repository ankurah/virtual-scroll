# Virtual Scroll Implementation Plan

## Current State vs Target

### Current Implementation
The current implementation is **partially complete but architecturally flawed**:
- Rust core uses `oldest_timestamp`/`newest_timestamp` directly as continuation points
- This ignores the anchor selection problem - platform layer should pick the anchor
- Scroll position stability is handled ad-hoc in TypeScript wrappers
- No spacer management

### Target Implementation
- **Rust**: Query construction + state tracking + load triggering
- **Platform**: Anchor selection + position measurement + scroll adjustment + spacers
- Clear API boundary between the two

## Crate Structure (Unchanged)

```
virtual-scroll/
├── Cargo.toml                  # Workspace root
├── SPEC.md
├── PLAN.md
├── TASKS.md
├── crates/
│   ├── virtual-scroll/         # Core crate
│   │   └── src/
│   │       ├── lib.rs          # Public API
│   │       ├── metrics.rs      # ScrollMode, ScrollInput, LoadRequest
│   │       ├── query.rs        # PaginatedSelection builder
│   │       └── manager.rs      # ScrollManager state machine
│   └── virtual-scroll-derive/  # Derive macro crate
│       └── src/
│           ├── lib.rs          # generate_scroll_manager! macro
│           ├── uniffi.rs       # UniFFI wrapper generation
│           └── wasm.rs         # WASM wrapper generation
```

## Core API Changes Required

### New Types in metrics.rs

```rust
/// Request from Rust for platform to trigger a load
#[derive(Clone, Copy, Debug)]
pub struct LoadRequest {
    /// Direction of pagination
    pub direction: LoadDirection,
    /// Suggested anchor offset from viewport edge (pixels)
    /// Platform should pick an item at approximately this distance
    pub anchor_offset: f64,
}
```

### manager.rs API Changes

```rust
impl ScrollManager {
    /// Process scroll event.
    /// Returns Some(LoadRequest) if platform should trigger pagination.
    /// Returns None if no action needed.
    /// NOTE: Does NOT block during loads - Ankurah handles concurrency.
    pub fn on_scroll(&mut self, input: ScrollInput) -> Option<LoadRequest>;

    /// Platform selected an anchor and is ready to load.
    /// Returns the selection string to use for the query.
    pub fn load_with_anchor(&mut self, anchor_timestamp: i64, direction: LoadDirection) -> String;
}
```

**Key changes**:
1. `on_scroll` returns `LoadRequest` (not selection string directly)
2. Platform picks anchor, calls `load_with_anchor(timestamp, direction)`
3. `load_with_anchor` returns the selection string
4. **No blocking**: Multiple loads can be in flight. Ankurah's `update_selection` handles concurrency with monotonic application (only latest wins).

### Generated Wrapper Changes

The generated `MessageScrollManager` needs to expose the two-step API:

**WASM:**
```rust
#[wasm_bindgen]
impl MessageScrollManager {
    /// Process scroll event. Returns LoadRequest as JSON if load needed.
    #[wasm_bindgen(js_name = onScroll)]
    pub fn on_scroll(&self, ...) -> JsValue;  // JSON of LoadRequest or null

    /// Platform picked an anchor, execute the load.
    #[wasm_bindgen(js_name = loadWithAnchor)]
    pub async fn load_with_anchor(&self, anchor_timestamp: i64, direction: String);
}
```

**UniFFI:**
```rust
#[uniffi::export]
impl MessageScrollManager {
    pub fn on_scroll(&self, ...) -> Option<LoadRequest>;
    pub fn load_with_anchor(&self, anchor_timestamp: i64, direction: LoadDirection);
}
```

## Platform Integration Changes

### TypeScript ScrollManagerWrapper

The wrapper needs to handle the two-step flow:

```typescript
class ScrollManagerWrapper {
    private pendingAnchor: PendingAnchor | null = null;

    private onScroll() {
        const loadRequest = this.scrollManager.onScroll(
            scrollTop, scrollHeight, clientHeight, delta, userScrolling
        );

        if (loadRequest) {
            this.startLoad(loadRequest);
        }
    }

    private startLoad(request: LoadRequest) {
        // 1. Pick anchor based on rendered positions
        const anchor = this.pickAnchor(request.direction, request.anchorOffset);
        if (!anchor) {
            // No suitable anchor found - skip this load
            // (rare edge case: empty list or all items off-screen)
            return;
        }

        // 2. Save anchor state for scroll adjustment
        this.pendingAnchor = {
            id: anchor.item.id,
            yBefore: this.getAnchorY(anchor.element),
        };

        // 3. Execute load with anchor timestamp
        // NOTE: No blocking - if multiple loads happen, Ankurah handles concurrency
        this.scrollManager.loadWithAnchor(
            anchor.item.timestamp,
            request.direction
        );
    }

    private onLiveQueryChange() {
        // After results render, adjust scroll position
        if (this.pendingAnchor) {
            this.adjustScrollForAnchor();
            this.pendingAnchor = null;
        }

        // Update Rust with result metadata
        this.updateResultState();
    }

    private adjustScrollForAnchor() {
        const anchorEl = this.findAnchorElement(this.pendingAnchor.id);
        if (!anchorEl) return;

        const yAfter = this.getAnchorY(anchorEl);
        const delta = yAfter - this.pendingAnchor.yBefore;

        if (Math.abs(delta) > 0.5) {
            this.container.scrollTop += delta;
        }
    }

    private pickAnchor(direction: LoadDirection, offset: number): Anchor | null {
        const items = this.scrollManager.items;
        if (items.length === 0) return null;

        if (direction === 'backward') {
            // For backward: pick item `offset` pixels BELOW viewport bottom
            // This ensures it exists in both old and new result sets
            return this.findItemAtOffset(items, 'fromBottom', offset);
        } else {
            // For forward: pick item `offset` pixels ABOVE viewport top
            return this.findItemAtOffset(items, 'fromTop', offset);
        }
    }
}
```

## Spacer Implementation

### Browser (CSS-based)
```typescript
// In container, before items:
<div className="leading-spacer" style={{ height: leadingSpacer }} />

// After items:
<div className="trailing-spacer" style={{ height: trailingSpacer }} />
```

Leading spacer height changes:
- Initial: 0 (or small fixed amount)
- After backward load: Adjusted to absorb new item heights (coordinated with scroll adjustment)
- At earliest boundary: Can be 0

Trailing spacer:
- Live mode: 0
- Non-live, not at latest: Small fixed value (e.g., 50px) for "scroll past" affordance

### React Native (FlatList)
- Use `ListHeaderComponent` for leading spacer
- Use `ListFooterComponent` for trailing spacer
- Or use `contentContainerStyle.paddingTop/paddingBottom`

## Test Plan

### Phase 1: Core Rust Tests (Priority)

1. **query_construction_tests.rs**
   - Live mode selection string
   - Backward continuation selection string
   - Forward continuation selection string
   - Filter update preserves/resets continuation

2. **load_trigger_tests.rs**
   - LoadRequest generated when topGap < minBuffer (scrolling up)
   - LoadRequest generated when bottomGap < minBuffer (scrolling down)
   - No LoadRequest when already loading
   - No LoadRequest when at boundary
   - No LoadRequest when not user-initiated scroll

3. **boundary_detection_tests.rs**
   - count < limit sets at_earliest in backward mode
   - count < limit sets at_latest in forward mode
   - at_latest triggers auto-transition to Live mode

4. **anchor_flow_tests.rs**
   - on_scroll returns LoadRequest
   - load_with_anchor builds correct query
   - cancel_load clears loading state

### Phase 2: Platform Integration Tests

Mock platform layer that simulates:
- Anchor selection (returns predefined timestamps)
- Scroll adjustment (records adjustments made)
- Verify full flow works end-to-end

### Phase 3: Visual Tests (Manual)

In both template apps:
1. Load room with many messages
2. Scroll up slowly - verify no jumps
3. Scroll up fast - verify pagination works
4. Scroll to top (at_earliest) - verify no more loading
5. Scroll down (forward pagination) - verify return to live

## Migration Path

### Step 1: Update Rust Core API
- Add `LoadRequest` type
- Change `on_scroll` return type
- Add `load_with_anchor` method
- Add `cancel_load` method
- Keep old API temporarily for compatibility

### Step 2: Update Generated Wrappers
- WASM: Add new methods
- UniFFI: Add new methods

### Step 3: Update Browser Template
- Rewrite ScrollManagerWrapper with two-step flow
- Add pendingAnchor state
- Add pickAnchor function
- Add scroll adjustment on result change
- Add spacer elements

### Step 4: Update React Native Template
- Same changes adapted for FlatList
- Test anchor measurement in RN

### Step 5: Remove Old API
- Remove direct selection return from on_scroll
- Clean up unused code

## Open Design Questions

### Q1: Anchor Not In New Results
What if the anchor item isn't in the new result set? (Rare, but possible with concurrent deletes)
- **Option A**: Fall back to no adjustment (may jump)
- **Option B**: Use nearest available item as backup anchor
- **Likely**: Option A is fine, this is a rare edge case

### Q2: Spacer Implementation
Deferred to platform integration. Options per platform:
- **Browser**: CSS div elements, padding, or `overflow-anchor` CSS
- **React Native**: FlatList `ListHeaderComponent`/`ListFooterComponent` or `contentContainerStyle`

### Q3: Inverted FlatList
Need to test how `inverted={true}` interacts with `maintainVisibleContentPosition`.
Many chat UIs use inverted lists. This is a platform integration test item.
