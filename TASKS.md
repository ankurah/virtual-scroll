# Virtual Scroll Tasks

## Key Finding: Platform-Native Scroll Stability

**React Native**: Has native `maintainVisibleContentPosition` prop that automatically handles scroll position stability when items are added. We do NOT need anchor-based logic for RN.

**Browser**: Must implement anchor-based scroll adjustment manually (as in original ChatScrollManager.ts).

This means:
- Rust core handles: Query construction, mode tracking, boundary detection, load triggering
- Browser wrapper: Anchor selection, position measurement, scroll adjustment
- RN wrapper: Just enables `maintainVisibleContentPosition` prop

**UniFFI Note**: The UniFFI flavor of the derive macro will need to create signal wrappers for reactive properties. It may be able to use the existing signal macro in Ankurah.

---

## Current Progress (January 2026)

### Completed ✅
- Core Rust implementation (`crates/virtual-scroll/src/lib.rs`)
- WASM bindings (`playwright-tests/wasm-bindings/`)
- React test harness (`playwright-tests/react-app/`)
- Fixed item ordering for chat-style display (oldest at top, newest at bottom)
- Fixed WASM type exports (properties vs methods)
- Deterministic test environment (800x600 viewport, 400px container)
- Visual debugging infrastructure (status bar, test messages)

### Playwright Test Suites Created
- `initial-load.spec.ts` ✅ - 8 tests for initial load and live mode
- `threshold.spec.ts` ✅ - 6 tests for exact pixel threshold behavior
- `backward-pagination.spec.ts` ✅ - 7 tests for backward pagination
- `forward-pagination.spec.ts` ✅ - 8 tests for forward pagination
- `scroll-stability.spec.ts` ✅ - Pixel-perfect position stability tests
- `edge-cases.spec.ts` ✅ - Boundary conditions and unusual scenarios
- `varied-heights.spec.ts` 🚧 - Tests with variable item heights (in progress)
- `stress.spec.ts` 📋 - Rapid scrolling and stress tests (planned)

### Next Steps
1. Finish remaining test files
2. Run all tests and fix any failures
3. Integrate into react-native-template app
4. Manual iOS testing
5. UniFFI wrapper generation (needs signal wrapper consideration)

---

## Phase 1: Fresh Start ✅ (COMPLETED)

### Clean Slate
- [x] Delete current `manager.rs` implementation (keep skeleton)
- [x] Quarantine derive macro (keep for reference - monomorphization still needed)
- [x] Keep `metrics.rs` and `query.rs` types (review for compatibility)

### Faithful TypeScript Port Reference
Study original ChatScrollManager.ts (preserved in this doc):
- Anchor selection via DOM measurement
- Position recording before/after query
- Scroll adjustment via delta

---

## Phase 2: Core Rust Implementation (Test-Driven)

### Query Builder Tests
- [ ] `test_live_mode_selection` - DESC, no continuation
- [ ] `test_backward_continuation` - DESC, timestamp <= anchor
- [ ] `test_forward_continuation` - ASC, timestamp >= anchor
- [ ] `test_filter_update_preserves_continuation`
- [ ] `test_filter_update_resets_continuation`

### Load Trigger Tests
- [ ] `test_backward_trigger_near_top` - Returns LoadRequest when scrolling up near top
- [ ] `test_forward_trigger_near_bottom` - Returns LoadRequest when scrolling down near bottom
- [ ] `test_trigger_allowed_during_load` - Does NOT block (Ankurah handles concurrency)
- [ ] `test_no_trigger_at_boundary` - Doesn't trigger when at earliest/latest
- [ ] `test_no_trigger_momentum_scroll` - Only triggers on user-initiated scroll
- [ ] `test_no_gratuitous_triggers` - Thresholds prevent excessive triggers
- [ ] `test_anchor_offset_calculation` - LoadRequest contains correct anchor offset

### Boundary Detection Tests
- [ ] `test_at_earliest_backward_mode` - count < limit sets at_earliest
- [ ] `test_at_latest_forward_mode` - count < limit sets at_latest
- [ ] `test_auto_transition_to_live` - Forward mode → Live when at_latest
- [ ] `test_live_mode_always_at_latest` - Live mode is always at_latest

### Mode Transition Tests
- [ ] `test_live_to_backward` - Scroll up transitions to Backward
- [ ] `test_backward_to_forward` - (via manual filter or jump)
- [ ] `test_forward_to_live` - Auto-transition when reaching latest
- [ ] `test_jump_to_live` - Explicit jump clears continuation

### Anchor Flow Tests (Two-Step API)
- [ ] `test_on_scroll_returns_load_request` - Not selection string
- [ ] `test_load_with_anchor_builds_query` - Uses provided timestamp
- [ ] `test_multiple_loads_allowed` - No blocking, Ankurah handles concurrency

### Display Order Tests
- [ ] `test_display_order_live_mode` - DESC results reversed for display
- [ ] `test_display_order_backward_mode` - DESC results reversed for display
- [ ] `test_display_order_forward_mode` - ASC results NOT reversed

### CRITICAL: Pixel-Perfect Boundary Tests ✅
These tests are now covered by Playwright tests in `scroll-stability.spec.ts`:

- [x] `test_backward_pixel_perfect_boundary` → scroll-stability.spec.ts
  ```
  Setup:
  1. Create mock viewport with items at known Y positions [A@100, B@200, C@300, D@400, E@500]
  2. Configure trigger threshold at topGap < 150px
  3. Scroll to topGap = 151px (1 pixel BEFORE trigger)
  4. Record exact Y position of each item

  Execute:
  5. Scroll 1 more pixel (topGap = 150px, triggers backward load)
  6. Load completes, new items [X, Y, Z] prepended
  7. Scroll adjustment applied

  Assert:
  8. Item A is now at Y = 101 (exactly 1 pixel different)
  9. Item B is now at Y = 201 (exactly 1 pixel different)
  10. Items X, Y, Z are above viewport (off-screen)
  11. No item moved more than 1 pixel from its pre-scroll position
  ```

- [ ] `test_forward_pixel_perfect_boundary`
  ```
  Setup:
  1. Create mock viewport in non-live mode with items [A@100, B@200, C@300, D@400, E@500]
  2. Configure trigger threshold at bottomGap < 150px
  3. Scroll to bottomGap = 151px (1 pixel BEFORE trigger)
  4. Record exact Y position of each item

  Execute:
  5. Scroll 1 more pixel (bottomGap = 150px, triggers forward load)
  6. Load completes, new items [F, G, H] appended
  7. Scroll adjustment applied

  Assert:
  8. Item E is now at Y = 499 (exactly 1 pixel different)
  9. All visible items moved exactly 1 pixel
  10. New items F, G, H are below viewport
  ```

- [ ] `test_boundary_no_jump_with_variable_heights`
  ```
  Same as above but with items of varying heights:
  [A: 50px tall, B: 120px tall, C: 74px tall, D: 200px tall]
  Verifies that anchor-based adjustment works regardless of item sizes
  ```

---

## Phase 3: Core Implementation

After tests are written, implement:
- [ ] `metrics.rs` - ScrollMode, LoadDirection, ScrollInput, LoadRequest
- [ ] `query.rs` - PaginatedSelection with build()
- [ ] `manager.rs` - ScrollManager with two-step API

Make tests pass.

---

## Phase 4: Integration Tests (Simulated Platform)

- [ ] Full backward pagination flow simulation
- [ ] Full forward pagination flow simulation
- [ ] Jump to live flow
- [ ] Filter change flow
- [ ] Rapid scroll handling

---

## Phase 5: Derive Macro (After Core is Solid)

- [ ] Generate WASM wrapper with two-step API
- [ ] Generate UniFFI wrapper with two-step API
- [ ] Test generated code compiles

---

## Phase 6: Platform Integration (Last)

### Browser Template
- [ ] Port ScrollManagerWrapper with anchor logic
- [ ] Test in browser

### React Native Template
- [ ] Simpler wrapper using `maintainVisibleContentPosition`
- [ ] Test in iOS simulator

---

## Reference: Original ChatScrollManager.ts Key Methods

```typescript
// Anchor selection - finds item at stepBack offset from opposite edge
private getContinuationAnchor(direction: 'backward' | 'forward', messageList: MessageView[]) {
    // For backward: pick item stepBack BELOW viewport bottom
    // For forward: pick item stepBack ABOVE viewport top
    // Returns { el: HTMLElement, msg: MessageView }
}

// Load execution with scroll stability
async loadMore(direction: 'backward' | 'forward') {
    const anchor = this.getContinuationAnchor(direction, messageList);
    const { y: yBefore } = offsetToParent(anchor.el);

    await this.messages.updateSelection(/* query using anchor.msg.timestamp */);

    const { y: yAfter } = offsetToParent(anchor.el);
    const delta = yAfter - yBefore;
    this.container.scrollTop += delta;  // Pixel-perfect adjustment
}
```

---

## Design Decisions (Resolved)

1. **Anchor Selection**: Platform picks anchor (not Rust)
2. **Scroll Adjustment**: Platform computes and applies (not Rust)
3. **Spacers**: Defer to platform integration
4. **RN Stability**: Use native `maintainVisibleContentPosition` prop
5. **Two-Step API**: `on_scroll()` → `LoadRequest` → platform picks anchor → `load_with_anchor(timestamp)`
6. **No Blocking**: Do NOT block loads. Ankurah's `update_selection` handles concurrency (monotonic application)
7. **Threshold Design**: Avoid gratuitous triggers via proper threshold values, not by blocking

---

## Open Questions

1. **RN maintainVisibleContentPosition bugs**: Rapid updates (<200ms) may cause issues. Need to test.
2. **Inverted FlatList**: Chat UIs often use `inverted={true}`. Does this affect maintainVisibleContentPosition?
3. **Spacers in RN**: How to implement leading/trailing spacers with FlatList?

---

## Sources

- [React Native ScrollView maintainVisibleContentPosition](https://reactnative.dev/docs/scrollview)
- [GetStream flat-list-mvcp](https://github.com/GetStream/flat-list-mvcp) - Android polyfill for older RN
- [Known bug: rapid updates](https://github.com/facebook/react-native/issues/53542) - RN 0.81.1 issue
