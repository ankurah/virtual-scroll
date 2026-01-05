# Virtual Scroll Specification

## Vision

Virtual Scroll is a platform-agnostic Rust library for managing paginated scroll state in reactive applications. It provides a pure state machine that handles bidirectional pagination with timestamp-based continuation, designed to work with any reactive query system (like Ankurah's LiveQuery).

## Problem Statement

Modern chat and feed applications need to:
1. Display large datasets efficiently (virtual scrolling)
2. Paginate in both directions (older/newer content)
3. Maintain scroll position across pagination
4. Integrate with reactive query systems
5. Work across multiple platforms (web, mobile)

Current solutions duplicate pagination logic in TypeScript for each platform (browser, React Native), leading to:
- Inconsistent behavior between platforms
- Difficult-to-test business logic embedded in UI code
- No unit testing without full UI integration

## Solution

A Rust crate that:
1. **Pure state machine** - No external dependencies in core
2. **Platform-agnostic** - Same logic for web (WASM) and mobile (UniFFI)
3. **Selection-string output** - Generates query strings, doesn't manage data
4. **Testable** - Full unit test coverage without UI dependencies
5. **Typed wrappers** - Derive macro generates platform-specific typed wrappers

## Core Concepts

### Selection Strings

The scroll manager outputs **selection strings** that consumers pass to their query system:

```
// Live mode (newest first, real-time updates)
"room = 'abc' AND deleted = false ORDER BY timestamp DESC LIMIT 50"

// Backward pagination (older content)
"room = 'abc' AND deleted = false AND timestamp <= 1704067200000 ORDER BY timestamp DESC LIMIT 50"

// Forward pagination (newer content, returning to live)
"room = 'abc' AND deleted = false AND timestamp >= 1704067200000 ORDER BY timestamp ASC LIMIT 50"
```

### Scroll Modes

1. **Live** - At latest position, receiving real-time updates via DESC query
2. **Backward** - Paginating to older content via DESC query with `timestamp <= anchor`
3. **Forward** - Paginating to newer content via ASC query with `timestamp >= anchor`

### Abstracted Measurements

Instead of platform-specific scroll handling, consumers pass a `ScrollInput`:

```rust
pub struct ScrollInput {
    pub offset: f64,          // Current scroll position
    pub content_height: f64,  // Total scrollable content height
    pub viewport_height: f64, // Visible viewport height
    pub scroll_delta: f64,    // Change since last event
    pub user_initiated: bool, // User dragging vs momentum/programmatic
}
```

This abstraction allows the same scroll manager to work with:
- Browser DOM (`<div>` with `scrollTop`, `scrollHeight`, `clientHeight`)
- React Native FlatList (`contentOffset`, `contentSize`, `layoutMeasurement`)
- Any other scrollable container

### LiveQuery Integration

The scroll manager doesn't manage the data itself - it outputs selection strings that consumers pass to their LiveQuery. The LiveQuery IS the signal:

```
ScrollInput → ScrollManager → Selection String → LiveQuery.updateSelection()
                                                        ↓
                                              LiveQuery subscription notifies React
                                                        ↓
                                              Component re-renders with new items
```

## API Design

### Core (No Dependencies)

```rust
// Create manager with base predicate
let mut manager = ScrollManager::new(
    "room = 'abc' AND deleted = false",
    "timestamp",
    viewport_height
);

// Process scroll events, get selection string if update needed
if let Some(selection) = manager.on_scroll(input) {
    live_query.update_selection(selection);
}

// After results arrive, update boundary state
manager.on_results(count, oldest_ts, newest_ts);

// Jump back to live mode
let selection = manager.jump_to_live();
live_query.update_selection(selection);

// Update filter (e.g., search)
let selection = manager.update_filter("room = 'abc' AND text LIKE '%search%'", true);
live_query.update_selection(selection);
```

### Attribute Macro (Typed Wrappers)

The `generate_scroll_manager!` macro is applied in the **bindings crate** (not the model crate), keeping the model platform-agnostic:

**In wasm-bindings/src/lib.rs:**
```rust
use virtual_scroll_derive::generate_scroll_manager;

// Re-export from model crate
pub use ankurah_template_model::*;

// Generate WASM scroll manager in bindings crate
generate_scroll_manager!(
    Message,           // Model type from model crate
    MessageView,       // View type
    MessageLiveQuery,  // LiveQuery type
    timestamp_field = "timestamp"
);
```

**In rn-bindings/src/lib.rs:**
```rust
use virtual_scroll_derive::generate_scroll_manager;

// Re-export from model crate
pub use ankurah_rn_model::*;

// Generate UniFFI scroll manager in bindings crate
generate_scroll_manager!(
    Message,
    MessageView,
    MessageLiveQuery,
    timestamp_field = "timestamp"
);
```

**Benefits:**
- Model crate stays platform-agnostic (no virtual-scroll dependency)
- Bindings crates choose which models get scroll managers
- Platform features (`wasm` vs `uniffi`) are controlled by bindings crate
- Generated code lives where it belongs (platform-specific bindings)

Generates:
- `MessageScrollManager` for UniFFI (React Native) when `uniffi` feature enabled
- `MessageScrollManager` for WASM (browser) when `wasm` feature enabled

Both hold a reference to the `MessageLiveQuery` and wire scroll events to selection updates.

## Design Principles

1. **No signal duplication** - LiveQuery already has subscription mechanism, don't create another
2. **Consumer controls scrolling** - Manager says "update query", consumer handles actual scroll
3. **Timestamp-based continuation** - Not offset-based; survives data changes
4. **Base predicate + continuation** - Filter changes can preserve or reset position
5. **Order direction locked** - Cannot change DESC/ASC without resetting continuation
