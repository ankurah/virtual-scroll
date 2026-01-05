# ankurah-virtual-scroll

Platform-agnostic virtual scroll state machine with pagination for Ankurah.

## Overview

`ankurah-virtual-scroll` provides smooth infinite scrolling through database-backed lists without loading everything into memory. It maintains a sliding window of items, expanding or sliding the window as the user scrolls, while preserving scroll position stability through intersection anchoring.

## Features

- **Bidirectional pagination**: Load older and newer content seamlessly
- **Scroll position stability**: Maintain pixel-perfect scroll position when loading new items
- **Reactive integration**: Works with Ankurah's LiveQuery for real-time updates
- **Platform-agnostic**: Core logic in Rust with WASM and UniFFI bindings
- **Variable item heights**: Handles items of different sizes correctly

## Installation

```toml
[dependencies]
ankurah-virtual-scroll = "0.7"
```

## Usage

### Leptos / Dioxus (Pure Rust)

Use `ScrollManager<V>` directly - no macro needed:

```rust
use ankurah_virtual_scroll::ScrollManager;

let scroll_manager = ScrollManager::<MessageView>::new(
    &ctx,
    "room = 'general'",  // Filter predicate
    "timestamp DESC",     // Display order
)?;

// Start the scroll manager (fire and forget - runs initial query in background)
scroll_manager.start();

// Subscribe to visible set updates
let visible_set = scroll_manager.visible_set();

// Notify on scroll events
scroll_manager.on_scroll(top_gap, bottom_gap, scrolling_up);
```

### React Web (WASM) / React Native (UniFFI)

For JavaScript/TypeScript frontends, use the `generate_scroll_manager!` macro in your bindings crate to generate platform-specific wrappers:

```rust
use ankurah_virtual_scroll::generate_scroll_manager;

// Generate MessageScrollManager with WASM or UniFFI bindings
generate_scroll_manager!(
    Message,           // Model type
    MessageView,       // View type
    MessageLiveQuery,  // LiveQuery type
    timestamp_field = "timestamp"
);
```

This generates `MessageScrollManager` with the appropriate bindings based on feature flags:
- `wasm` feature: generates `#[wasm_bindgen]` bindings for React web apps
- `uniffi` feature: generates UniFFI bindings for React Native apps

#### React Component Example

```tsx
import { useMemo, useCallback, useRef } from 'react'
import { signalObserver } from './utils'
import { MessageScrollManager, ctx } from './generated/bindings'

export const MessageList = signalObserver(function MessageList({ roomId }: { roomId: string }) {
  const containerRef = useRef<HTMLDivElement>(null)
  const lastScrollTopRef = useRef(0)

  // Create scroll manager once per room
  const manager = useMemo(() => {
    const m = new MessageScrollManager(ctx(), `room = '${roomId}'`, 'timestamp DESC')
    m.start() // Fire and forget
    return m
  }, [roomId])

  // Get visible set signal (memoized)
  const visibleSetSignal = useMemo(() => manager.visibleSet(), [manager])

  // Call .get() inside signalObserver to auto-track reactivity
  const visibleSet = visibleSetSignal.get()
  const messages = visibleSet.items()

  const handleScroll = useCallback((e: React.UIEvent<HTMLDivElement>) => {
    const el = e.currentTarget
    const scrollingUp = el.scrollTop < lastScrollTopRef.current
    lastScrollTopRef.current = el.scrollTop

    const topGap = el.scrollTop
    const bottomGap = el.scrollHeight - el.scrollTop - el.clientHeight

    manager.onScroll(topGap, bottomGap, scrollingUp)
  }, [manager])

  return (
    <div ref={containerRef} onScroll={handleScroll} style={{ height: '100%', overflowY: 'auto' }}>
      {messages.map(msg => (
        <div key={msg.id().toString()}>{msg.content()}</div>
      ))}
    </div>
  )
})
```

## Modes

- **Live**: At the newest edge, receiving real-time updates with auto-scroll
- **Backward**: User scrolled toward older items, loading historical content
- **Forward**: User scrolling back toward newer items, transitions to Live when reaching the edge

## Architecture

The scroll manager handles:
- Query construction (predicate + cursor + ordering + limit)
- Mode tracking (Live / Backward / Forward)
- Boundary detection (at earliest/latest based on result count)
- Intersection anchoring for scroll stability

Platform layers handle:
- DOM/FlatList binding and scroll events
- Scroll position measurement and adjustment
- Spacer management

## Crates

- `ankurah-virtual-scroll` - Core scroll manager implementation
- `ankurah-virtual-scroll-derive` - Derive macro for generating typed scroll managers

## Version Compatibility

Minor versions align with ankurah (e.g., 0.7.x works with ankurah 0.7.x). Patch versions are independent.

## License

MIT OR Apache-2.0
