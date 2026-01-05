# Virtual Scroll Test Plan

## Overview

This document defines comprehensive tests for the virtual scroll windowing system. Tests are implemented in two phases:

1. **Phase 1: Rust Tests** - Unit/integration tests using `MockRenderer` to simulate scroll behavior
2. **Phase 2: Playwright Tests** - End-to-end browser tests with actual React rendering

Both phases mirror each other, ensuring consistent behavior between the Rust core and the WASM/React integration.

## Test Parameters

Standard test configuration (matches existing tests):
- 60 messages, timestamps 1000-1059, all 50px height
- Viewport: 500px (10 items visible)
- screen_items=10, buffer=20, live_window=30
- Trigger threshold: items_above/below <= 10

### Current Coverage Gaps

The existing tests (1.1, 1.2) use:
- **Uniform heights**: All items are exactly 50px
- **Round scroll amounts**: 400, 200, 250, 100, 300 pixels

Tests 1.9 and 1.11 specifically address these gaps with variable heights and non-aligned scroll positions.

## Phase 1: Rust Tests

Location: `crates/virtual-scroll/tests/`

### 1.1 Basic Round-Trip ✅
**File:** `scroll_manager_tests.rs::test_scroll_live_to_oldest_and_back`

- [x] Initial Live mode: 30 items, auto-scroll to bottom
- [x] Backward pagination: 30 → 40 → 50 items
- [x] Hit oldest edge: has_more_preceding=false
- [x] Forward pagination back to Live mode
- [x] Verify selection predicate at each step

### 1.2 Direction Reversal ✅
**File:** `scroll_manager_tests.rs::test_direction_reversal`

- [x] Scroll backward to trigger pagination
- [x] Scroll forward without triggering (stay in buffer)
- [x] Scroll backward again to trigger next pagination
- [x] Verify correct mode and selection throughout

### 1.3 Edge Boundary Tests ✅
**File:** `edge_boundary_tests.rs::test_edge_boundaries_smaller_than_live_window`

- [x] Start with dataset smaller than `live_window` (25 items) - has_more_preceding=false from start
- [x] Scroll to top - no pagination triggers
- [x] Scroll back to bottom - still no pagination
- [x] Verify has_more_preceding/has_more_following flags correct throughout

### 1.4 Small Dataset Tests ✅
**File:** `edge_boundary_tests.rs::test_edge_boundaries_small_dataset`

- [x] Dataset smaller than live_window (20 items)
- [x] has_more_preceding=false from start
- [x] No pagination triggers when scrolling
- [x] Stays in Live mode throughout

### 1.5 Rapid Scroll Stress Test ✅
**File:** `advanced_scroll_tests.rs`

- [x] Multiple scroll events before pagination completes
- [x] Alternating up/down rapid scrolls
- [x] Verify no panics or inconsistent state
- [x] Verify items remain ordered by timestamp

### 1.6 Intersection Anchoring Tests ✅
**File:** `advanced_scroll_tests.rs`

- [x] Verify intersection item exists in both old and new windows
- [x] Backward: intersection at newest visible (bottom of viewport)
- [x] Forward: intersection at oldest visible (top of viewport)
- [x] Verify scroll offset calculation after window change

### 1.7 Selection Predicate Tests ✅
**File:** `advanced_scroll_tests.rs`

- [x] Verify cursor value matches expected timestamp
- [x] Backward: `timestamp <= cursor ORDER BY DESC`
- [x] Forward: `timestamp >= cursor ORDER BY ASC`
- [x] Forward at oldest edge: cursor=0 to include all items
- [x] LIMIT matches expected window size

### 1.8 Live Mode Behavior ✅
**File:** `advanced_scroll_tests.rs`

- [x] Initial load in Live mode with should_auto_scroll=true
- [ ] New item arrival triggers re-render (requires live data injection)
- [x] Scroll offset updates to show new items
- [x] Scrolling up exits Live mode
- [x] Returning to bottom re-enters Live mode

### 1.9 Variable Height Items ✅
**File:** `variable_height_tests.rs::test_variable_heights`

Test with mixed item heights (50-150px) to verify:
- [x] Items: heights cycling through [50, 75, 100, 125, 150] px
- [x] Trigger conditions work correctly (based on item count, not pixels)
- [x] Intersection anchoring maintains visual stability despite varying heights
- [x] Visible range calculations handle partial visibility correctly
- [x] scroll_offset adjustments after pagination are pixel-accurate

### 1.10 Non-Aligned Scroll Positions ✅
**File:** `non_aligned_scroll_tests.rs`

Test with scroll amounts that don't align to item boundaries:
- [x] Scroll by odd amounts (37px, 123px, 289px)
- [x] Scroll to positions mid-item (item partially visible at top/bottom)
- [x] First/last visible item detection when partially clipped
- [x] Intersection anchoring when anchor item is partially visible
- [x] Combined with variable heights for maximum coverage

### 1.11 Concurrent Operations ✅
**File:** `advanced_scroll_tests.rs`

- [x] Scroll event during pending pagination
- [x] Multiple scroll events queued
- [x] Pagination completes in correct order

## Phase 2: Playwright Tests

Location: `playwright-tests/tests/`

### 2.1 Basic Round-Trip
**File:** `round-trip.spec.ts`

Mirror of Rust test 1.1:
- [ ] Initial Live mode verification
- [ ] Backward scroll to oldest
- [ ] Forward scroll back to Live
- [ ] Visual verification of scroll position stability

### 2.2 Direction Reversal
**File:** `direction-reversal.spec.ts`

Mirror of Rust test 1.2:
- [ ] Backward pagination trigger
- [ ] Forward scroll without trigger
- [ ] Backward again
- [ ] Verify no visual glitches

### 2.3 Scroll Stability
**File:** `scroll-stability.spec.ts`

- [ ] Record item position before pagination
- [ ] Trigger pagination
- [ ] Verify same item at same visual position
- [ ] Test with varied item heights

### 2.4 Edge Boundaries
**File:** `edge-boundaries.spec.ts`

Mirror of Rust test 1.3:
- [ ] Visual verification at oldest edge
- [ ] Visual verification at newest edge (Live mode)
- [ ] Correct UI indicators for has_more flags

### 2.5 Stress Testing
**File:** `stress.spec.ts`

- [ ] Rapid mouse wheel scrolling
- [ ] Touch gesture simulation (swipe scrolling)
- [ ] Programmatic rapid scroll position changes
- [ ] Large dataset (1000+ items)

### 2.6 Live Updates
**File:** `live-updates.spec.ts`

- [ ] New item appears while in Live mode
- [ ] Scroll position maintained at bottom
- [ ] New item appears while in Backward mode (no scroll change)
- [ ] Return to Live shows new items

### 2.7 Error Recovery
**File:** `error-recovery.spec.ts`

- [ ] Query timeout handling
- [ ] Network interruption during pagination
- [ ] Recovery after error

### 2.8 Performance
**File:** `performance.spec.ts`

- [ ] Measure time to initial render
- [ ] Measure pagination latency
- [ ] Verify no memory leaks during extended scrolling
- [ ] Frame rate during scroll (no jank)

## Test Helpers

### Rust: MockRenderer

```rust
// Key methods used in tests:
r.next_render().await?                    // Wait for VisibleSet update
r.up_no_render(px, first_ts, last_ts)     // Scroll up, verify no trigger
r.down_no_render(px, first_ts, last_ts)   // Scroll down, verify no trigger
r.scroll_up_and_expect(...)               // Scroll up, expect pagination
r.scroll_down_and_expect(...)             // Scroll down, expect pagination
r.assert(&vs, items, ts_range, ...)       // Assert VisibleSet state
sm.current_selection()                    // Get current query predicate
sm.mode()                                 // Get current scroll mode
```

### Playwright: testHelpers

```typescript
// Key methods exposed to browser:
await setupScrollTest(page, { count, startTimestamp, viewportHeight })
await getScrollState(page)                // mode, hasMore*, intersection
await getItems(page)                      // Current items array
await scrollTo(page, offset)              // Set scroll position
await scrollBy(page, delta)               // Relative scroll
await triggerOnScroll(page)               // Force scroll event processing
await assertScrollStability(page, itemId, expectedTop)
```

## Assertion Patterns

### Rust Test Pattern
```rust
// Setup
let ctx = durable_sled_setup().await?;
create_messages(&ctx, (0..60).map(|i| (1000 + i, 50))).await?;
let sm = Arc::new(ScrollManager::new(&ctx, "true", "timestamp DESC", 50, 2.0, 500)?);
let mut r = MockRenderer::new(sm.clone(), 500);
tokio::spawn(async move { sm.start().await });

// Initial state
let vs = r.next_render().await?;
r.assert(&vs, 30, 1030..=1059, None, true, false, true, 1050, 1059);

// Scroll and verify
r.up_no_render(400, 1042, 1051).await;
r.scroll_up_and_expect(
    100, 40, 1020..=1059, Some(1049),
    true, true, false, 1040, 1049, 1000,
    "TRUE AND \"timestamp\" <= 1059 ORDER BY timestamp DESC LIMIT 41",
).await?;
```

### Playwright Test Pattern
```typescript
// Setup
await setupScrollTest(page, { count: 60, startTimestamp: 1000, viewportHeight: 500 });

// Initial state
const state = await getScrollState(page);
expect(state.mode).toBe('Live');
expect(state.itemCount).toBe(30);

// Scroll and verify
await scrollBy(page, -500);
await waitForItemCountChange(page, 30);
const newState = await getScrollState(page);
expect(newState.itemCount).toBe(40);
expect(newState.mode).toBe('Backward');
```

## Running Tests

### Rust Tests
```bash
# Run all scroll tests
cargo test -p virtual-scroll --test scroll_manager_tests

# Run with logging
LOG_LEVEL=debug cargo test -p virtual-scroll --test scroll_manager_tests -- --nocapture
```

### Playwright Tests
```bash
cd playwright-tests

# Build WASM bindings first
cd wasm-bindings && wasm-pack build --target web && cd ..

# Run tests
npx playwright test

# Run with UI
npx playwright test --ui

# Run specific test file
npx playwright test tests/round-trip.spec.ts
```

## Implementation Priority

1. ✅ Basic round-trip (Rust) - 1.1
2. ✅ Direction reversal (Rust) - 1.2
3. ✅ Edge boundaries (Rust) - 1.3
4. ✅ Small datasets (Rust) - 1.4
5. ✅ Variable heights (Rust) - 1.9
6. ✅ Non-aligned scrolls (Rust) - 1.10
7. ✅ Selection predicates (Rust) - 1.7
8. ✅ Live mode behavior (Rust) - 1.8
9. ✅ Remaining Rust tests (1.5, 1.6, 1.11)
10. Basic Playwright tests (2.1-2.4)
11. Remaining Playwright tests (2.5-2.8)
