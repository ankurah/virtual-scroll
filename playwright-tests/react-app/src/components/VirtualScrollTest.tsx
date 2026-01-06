import { useEffect, useRef, useState, useCallback } from 'react'
import type { TestHelpers } from '../App'

// Types for WASM bindings
interface EntityId {
  toString: () => string
}

interface MessageView {
  id: EntityId
  text: string
  timestamp: bigint
  room: string
}

// VisibleSet data returned by visibleSet().get()
interface MessageVisibleSet {
  items: MessageView[]  // property getter, not a method
  intersection: () => { entityId: string; index: number } | null  // returns plain object
  hasMoreOlder: () => boolean
  hasMoreNewer: () => boolean
  shouldAutoScroll: () => boolean
}

// Signal wrapper with .get() method
interface MessageVisibleSetSignal {
  get: () => MessageVisibleSet
}

interface MessageScrollManager {
  start: () => Promise<void>
  onScroll: (topGap: number, bottomGap: number, scrollingUp: boolean) => Promise<string | null>
  visibleSet: () => MessageVisibleSetSignal
  mode: string
  isLoading: () => boolean
  jumpToLive: () => Promise<void>
  updateFilter: (predicate: string, resetPosition: boolean) => Promise<void>
  setViewportHeight: (height: number) => void
}

interface WasmBindings {
  ctx: () => unknown
  seed_test_data: (room: string, count: number, startTimestamp: bigint, variedHeights: boolean) => Promise<void>
  clear_all_messages: () => Promise<void>
  MessageScrollManager: new (ctx: unknown, predicate: string, orderBy: string) => MessageScrollManager
}

// Fixed container height for deterministic test results
const CONTAINER_HEIGHT = 400

export function VirtualScrollTest() {
  const containerRef = useRef<HTMLDivElement>(null)
  const [scrollManager, setScrollManager] = useState<MessageScrollManager | null>(null)
  const [items, setItems] = useState<MessageView[]>([])
  const [intersection, setIntersection] = useState<{ entityId: string; index: number } | null>(null)
  const [mode, setMode] = useState<string>('Live')
  const [loading, setLoading] = useState(false)
  const [hasMoreOlder, setHasMoreOlder] = useState(false)
  const [hasMoreNewer, setHasMoreNewer] = useState(false)
  const [shouldAutoScroll, setShouldAutoScroll] = useState(true)
  const [currentRoom, setCurrentRoom] = useState<string>('room1')
  const [testStatus, setTestStatus] = useState<string>('')
  const lastScrollTop = useRef(0)

  // Get WASM bindings
  const getWasm = useCallback((): WasmBindings => {
    if (!window.wasm) throw new Error('WASM not loaded')
    return window.wasm as unknown as WasmBindings
  }, [])

  // Update state from scroll manager's visible set signal
  const syncState = useCallback(() => {
    if (!scrollManager) return
    const vs = scrollManager.visibleSet().get()
    setItems([...vs.items])
    const inter = vs.intersection()
    setIntersection(inter ? { entityId: inter.entityId, index: inter.index } : null)
    setMode(scrollManager.mode)
    setLoading(scrollManager.isLoading())
    setHasMoreOlder(vs.hasMoreOlder())
    setHasMoreNewer(vs.hasMoreNewer())
    setShouldAutoScroll(vs.shouldAutoScroll())
  }, [scrollManager])

  // Scroll handler
  const handleScroll = useCallback(async () => {
    if (!scrollManager || !containerRef.current) return

    const container = containerRef.current
    const scrollTop = container.scrollTop
    const scrollHeight = container.scrollHeight
    const clientHeight = container.clientHeight

    // Calculate gaps
    const topGap = scrollTop
    const bottomGap = scrollHeight - scrollTop - clientHeight

    // Determine scroll direction
    const scrollingUp = scrollTop < lastScrollTop.current
    lastScrollTop.current = scrollTop

    // Only call onScroll for user-initiated scrolls (not programmatic)
    // The scroll manager will decide if pagination is needed
    const direction = await scrollManager.onScroll(topGap, bottomGap, scrollingUp)

    if (direction) {
      console.log('Pagination triggered:', direction)
    }

    syncState()
  }, [scrollManager, syncState])

  // Create test helpers and expose to window
  useEffect(() => {
    const helpers: TestHelpers = {
      // Test status display
      setTestStatus: (status: string) => setTestStatus(status),
      clearTestStatus: () => setTestStatus(''),

      // Data management
      seedTestData: async (room, count, startTimestamp, variedHeights) => {
        const wasm = getWasm()
        await wasm.seed_test_data(room, count, BigInt(startTimestamp), variedHeights)
      },

      clearAllMessages: async () => {
        const wasm = getWasm()
        await wasm.clear_all_messages()
      },

      // Scroll manager control
      createScrollManager: async (room, viewportHeight) => {
        const wasm = getWasm()
        const ctx = wasm.ctx()
        const predicate = `room = '${room}' AND deleted = false`
        const manager = new wasm.MessageScrollManager(ctx, predicate, 'timestamp DESC')
        // Set viewport height before starting
        manager.setViewportHeight(viewportHeight)
        await manager.start()
        setScrollManager(manager)
        setCurrentRoom(room)

        // Initial sync after a tick
        setTimeout(() => {
          const vs = manager.visibleSet().get()
          setItems([...vs.items])
          const inter = vs.intersection()
          setIntersection(inter ? { entityId: inter.entityId, index: inter.index } : null)
          setMode(manager.mode)
          setHasMoreOlder(vs.hasMoreOlder())
          setHasMoreNewer(vs.hasMoreNewer())
          setShouldAutoScroll(vs.shouldAutoScroll())
        }, 0)
      },

      destroyScrollManager: () => {
        setScrollManager(null)
        setItems([])
        setIntersection(null)
        setMode('Live')
        setHasMoreOlder(false)
        setHasMoreNewer(false)
        setShouldAutoScroll(true)
      },

      jumpToLive: async () => {
        if (!scrollManager) throw new Error('No scroll manager')
        await scrollManager.jumpToLive()
        syncState()
      },

      updateFilter: async (predicate, resetPosition) => {
        if (!scrollManager) throw new Error('No scroll manager')
        await scrollManager.updateFilter(predicate, resetPosition)
        syncState()
      },

      // Scroll control
      setScrollTop: (value) => {
        if (!containerRef.current) return
        containerRef.current.scrollTop = value
      },

      getScrollTop: () => containerRef.current?.scrollTop ?? 0,
      getScrollHeight: () => containerRef.current?.scrollHeight ?? 0,
      getClientHeight: () => containerRef.current?.clientHeight ?? 0,

      scrollBy: (delta) => {
        if (!containerRef.current) return
        containerRef.current.scrollTop += delta
      },

      scrollToTop: () => {
        if (!containerRef.current) return
        containerRef.current.scrollTop = 0
      },

      scrollToBottom: () => {
        if (!containerRef.current) return
        containerRef.current.scrollTop = containerRef.current.scrollHeight
      },

      // State inspection - use local state that's synced from visibleSet
      getItems: () => {
        return items.map(item => ({
          id: item.id.toString(),
          text: item.text,
          timestamp: Number(item.timestamp),
        }))
      },

      getIntersection: () => intersection,
      getMode: () => mode,
      hasMoreOlder: () => hasMoreOlder,
      hasMoreNewer: () => hasMoreNewer,
      shouldAutoScroll: () => shouldAutoScroll,
      isLoading: () => loading,
      getItemCount: () => items.length,

      // Metrics for scroll stability
      getItemPositions: () => {
        if (!containerRef.current) return []
        const container = containerRef.current
        const itemElements = container.querySelectorAll('[data-item-id]')
        const positions: Array<{ id: string; top: number; height: number }> = []

        itemElements.forEach((el) => {
          const id = el.getAttribute('data-item-id')
          if (id) {
            const rect = el.getBoundingClientRect()
            const containerRect = container.getBoundingClientRect()
            positions.push({
              id,
              top: rect.top - containerRect.top + container.scrollTop,
              height: rect.height,
            })
          }
        })

        return positions
      },

      getItemById: (id) => {
        if (!containerRef.current) return null
        const el = containerRef.current.querySelector(`[data-item-id="${id}"]`)
        if (!el) return null

        const rect = el.getBoundingClientRect()
        const containerRect = containerRef.current.getBoundingClientRect()
        return {
          top: rect.top - containerRect.top + containerRef.current.scrollTop,
          height: rect.height,
        }
      },

      // Trigger scroll event manually (for precise testing)
      // Optionally pass scrollingUp to force direction when scrollTop comparison is unreliable
      triggerOnScroll: async (forceScrollingUp?: boolean) => {
        if (!scrollManager || !containerRef.current) return null

        const container = containerRef.current
        const topGap = container.scrollTop
        const bottomGap = container.scrollHeight - container.scrollTop - container.clientHeight

        // Use forced direction if provided, otherwise determine from scroll position
        // Note: scrollingUp comparison can be unreliable when scrollTop is set programmatically
        let scrollingUp: boolean
        if (forceScrollingUp !== undefined) {
          scrollingUp = forceScrollingUp
        } else {
          // Heuristic: if at top (topGap < 50), assume scrolling up intent
          // if at bottom (bottomGap < 50), assume scrolling down intent
          if (topGap < 50) {
            scrollingUp = true
          } else if (bottomGap < 50) {
            scrollingUp = false
          } else {
            scrollingUp = container.scrollTop < lastScrollTop.current
          }
        }

        lastScrollTop.current = container.scrollTop
        const direction = await scrollManager.onScroll(topGap, bottomGap, scrollingUp)
        syncState()
        return direction
      },
    }

    window.testHelpers = helpers

    return () => {
      window.testHelpers = null
    }
  }, [getWasm, scrollManager, items, intersection, mode, loading, hasMoreOlder, hasMoreNewer, shouldAutoScroll, syncState])

  // Auto-scroll to bottom in live mode
  useEffect(() => {
    if (shouldAutoScroll && containerRef.current) {
      containerRef.current.scrollTop = containerRef.current.scrollHeight
    }
  }, [items, shouldAutoScroll])

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', padding: 20 }}>
      {/* Debug info */}
      <div
        data-testid="debug-info"
        style={{
          padding: 10,
          marginBottom: 10,
          background: '#f0f0f0',
          borderRadius: 4,
          fontSize: 12,
          fontFamily: 'monospace',
        }}
      >
        <div>Room: {currentRoom} | Mode: {mode} | Items: {items.length}</div>
        <div>
          Loading: {loading ? 'yes' : 'no'} |
          More older: {hasMoreOlder ? 'yes' : 'no'} |
          More newer: {hasMoreNewer ? 'yes' : 'no'} |
          Auto-scroll: {shouldAutoScroll ? 'yes' : 'no'}
        </div>
        {intersection && (
          <div>Intersection: index={intersection.index}, id={intersection.entityId.slice(-6)}</div>
        )}
      </div>

      {/* Scroll container - fixed height for deterministic tests */}
      <div
        ref={containerRef}
        data-testid="scroll-container"
        onScroll={handleScroll}
        style={{
          height: CONTAINER_HEIGHT,
          overflowY: 'auto',
          border: '1px solid #ccc',
          borderRadius: 4,
        }}
      >
        {/* Loading indicator at top */}
        {loading && (
          <div style={{ padding: 10, textAlign: 'center', color: '#666' }}>
            Loading...
          </div>
        )}

        {/* Messages */}
        {items.map((item, index) => {
          const id = item.id.toString()
          const isIntersection = intersection?.index === index

          return (
            <div
              key={id}
              data-testid="message-item"
              data-item-id={id}
              data-item-index={index}
              data-timestamp={Number(item.timestamp)}
              style={{
                padding: '12px 16px',
                borderBottom: '1px solid #eee',
                background: isIntersection ? '#ffe0e0' : 'white',
              }}
            >
              <div style={{ fontSize: 12, color: '#666', marginBottom: 4 }}>
                #{index} | ts: {Number(item.timestamp)} | id: {id.slice(-6)}
              </div>
              <div>{item.text}</div>
            </div>
          )
        })}

        {/* Empty state */}
        {items.length === 0 && !loading && (
          <div style={{ padding: 40, textAlign: 'center', color: '#999' }}>
            No messages. Use testHelpers to seed data and create scroll manager.
          </div>
        )}
      </div>

      {/* Test status bar - shows current test action */}
      {testStatus && (
        <div
          data-testid="test-status"
          style={{
            marginTop: 10,
            padding: 12,
            background: '#2563eb',
            color: 'white',
            borderRadius: 4,
            fontSize: 14,
            fontWeight: 500,
            textAlign: 'center',
          }}
        >
          {testStatus}
        </div>
      )}
    </div>
  )
}
