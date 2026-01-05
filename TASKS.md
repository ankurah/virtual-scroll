# Virtual Scroll Tasks

## Phase 1: Core Crate Setup ✅
- [x] Create workspace structure
- [x] Create `virtual-scroll` crate
- [x] Create `virtual-scroll-derive` crate (stub)
- [x] Write SPEC.md
- [x] Write PLAN.md
- [x] Write TASKS.md

## Phase 2: Core Implementation ✅
- [x] Implement `metrics.rs`
  - [x] `ScrollMode` enum
  - [x] `LoadDirection` enum
  - [x] `ScrollInput` struct
  - [x] `ScrollMetrics` struct
- [x] Implement `query.rs`
  - [x] `PaginatedSelection` struct
  - [x] `Continuation` struct
  - [x] `build()` method for selection strings
  - [x] `update_base()` with reset option
- [x] Implement `manager.rs`
  - [x] `ScrollManager` struct
  - [x] `new()` constructor
  - [x] `on_scroll()` - pagination trigger logic
  - [x] `on_results()` - boundary detection
  - [x] `update_filter()` - base predicate change
  - [x] `jump_to_live()` - return to live mode
  - [x] Configuration constants

## Phase 3: Unit Tests ✅ (inline in modules)
- [x] Query tests (in `query.rs`)
  - [x] Live mode selection string
  - [x] Backward pagination selection string
  - [x] Forward pagination selection string
  - [x] Filter update preserves continuation
  - [x] Filter update resets continuation
- [x] Pagination tests (in `manager.rs`)
  - [x] Backward trigger when near top
  - [x] No trigger when already loading
  - [x] Boundary detection (count < limit)
  - [x] Auto-transition to live when at_latest
  - [x] Jump to live
  - [x] Filter update (preserve and reset)

## Phase 4: Derive Macro
- [ ] Parse `#[virtual_scroll(timestamp_field = "...")]` attribute
- [ ] Implement UniFFI wrapper generation
  - [ ] Generate `{Model}ScrollManager` struct
  - [ ] Generate `#[uniffi::export]` impl block
  - [ ] Wire to LiveQuery
- [ ] Implement WASM wrapper generation
  - [ ] Generate `{Model}ScrollManager` struct
  - [ ] Generate `#[wasm_bindgen]` impl block
  - [ ] Wire to LiveQuery

## Phase 5: Integration
- [ ] Add dependency to `ankurah-react-native-template/model`
- [ ] Add `#[derive(VirtualScroll)]` to Message model
- [ ] Update React Native `Chat.tsx` to use `MessageScrollManager`
- [ ] Remove TypeScript `ChatScrollManager.ts`
- [ ] Test in iOS simulator
- [ ] Add dependency to `ankurah-react-sled-template/model`
- [ ] Add `#[derive(VirtualScroll)]` to Message model
- [ ] Update browser `Chat.tsx` to use `MessageScrollManager`
- [ ] Remove TypeScript `ChatScrollManager.ts`
- [ ] Test in browser

## Phase 6: Polish
- [ ] Add debug metrics output
- [ ] Document public API
- [ ] Add CI workflow
- [ ] Publish to crates.io (optional)

---

## Current Focus

**Next**: Phase 4 - Derive Macro

The core crate is complete with all unit tests passing (13 tests). Next step is implementing the derive macro to generate typed `{Model}ScrollManager` wrappers for UniFFI and WASM.
