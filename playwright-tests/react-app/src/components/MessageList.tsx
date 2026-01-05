import { useEffect, useRef, useState, useCallback } from 'react'
import type { TestHelpers } from '../App'

// =============================================================================
// Types
// =============================================================================

interface EntityId {
  toString: () => string
}

interface MessageView {
  id: EntityId
  text: string
  timestamp: bigint
  room: string
}

interface MessageVisibleSet {
  items: MessageView[]
  intersection: () => { entityId: string; index: number } | null
  hasMorePreceding: () => boolean
  hasMoreFollowing: () => boolean
  shouldAutoScroll: () => boolean
}

interface MessageVisibleSetSignal {
  get: () => MessageVisibleSet
}

interface MessageScrollManager {
  start: () => Promise<void>
  onScroll: (firstVisible: string, lastVisible: string, scrollingBackward: boolean) => void
  visibleSet: () => MessageVisibleSetSignal
  mode: string
  currentSelection: () => string
}

interface WasmBindings {
  ctx: () => unknown
  seed_test_data: (room: string, count: number, startTimestamp: bigint, variedHeights: boolean) => Promise<void>
  clear_all_messages: () => Promise<void>
  MessageScrollManager: new (
    ctx: unknown,
    predicate: string,
    orderBy: string,
    minimumRowHeight: number,
    bufferFactor: number,
    viewportHeight: number
  ) => MessageScrollManager
}

// =============================================================================
// Constants & Styles
// =============================================================================

const CONTAINER_HEIGHT = 400

const styles = {
  container: {
    display: 'flex',
    flexDirection: 'column' as const,
    height: '100%',
    padding: 24,
    fontFamily: '-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif',
    background: '#f8fafc',
  },
  header: {
    marginBottom: 16,
  },
  title: {
    margin: 0,
    fontSize: 20,
    fontWeight: 600,
    color: '#1e293b',
  },
  subtitle: {
    margin: '4px 0 0',
    fontSize: 13,
    color: '#64748b',
  },
  statusBar: {
    display: 'flex',
    gap: 12,
    padding: '10px 14px',
    marginBottom: 12,
    background: '#fff',
    borderRadius: 8,
    border: '1px solid #e2e8f0',
    fontSize: 13,
    color: '#475569',
  },
  statusItem: {
    display: 'flex',
    alignItems: 'center',
    gap: 6,
  },
  statusLabel: {
    color: '#94a3b8',
    fontWeight: 500,
  },
  statusValue: {
    fontWeight: 600,
    color: '#1e293b',
  },
  badge: (active: boolean, color: string) => ({
    padding: '2px 8px',
    borderRadius: 4,
    fontSize: 11,
    fontWeight: 600,
    background: active ? color : '#f1f5f9',
    color: active ? '#fff' : '#94a3b8',
  }),
  scrollContainer: {
    flex: 1,
    height: CONTAINER_HEIGHT,
    overflowY: 'auto' as const,
    background: '#fff',
    borderRadius: 8,
    border: '1px solid #e2e8f0',
    boxShadow: '0 1px 3px rgba(0,0,0,0.05)',
  },
  messageItem: (isIntersection: boolean) => ({
    padding: '14px 16px',
    borderBottom: '1px solid #f1f5f9',
    background: isIntersection ? '#fef2f2' : '#fff',
    borderLeft: isIntersection ? '3px solid #ef4444' : '3px solid transparent',
    transition: 'background 0.15s',
  }),
  messageMeta: {
    display: 'flex',
    gap: 8,
    marginBottom: 6,
    fontSize: 11,
    color: '#94a3b8',
    fontFamily: 'ui-monospace, monospace',
  },
  messageText: {
    fontSize: 14,
    color: '#334155',
    lineHeight: 1.5,
  },
  emptyState: {
    padding: 48,
    textAlign: 'center' as const,
    color: '#94a3b8',
  },
  emptyIcon: {
    fontSize: 32,
    marginBottom: 12,
  },
  testStatusBar: {
    marginTop: 12,
    padding: '12px 16px',
    background: 'linear-gradient(135deg, #3b82f6, #2563eb)',
    color: '#fff',
    borderRadius: 8,
    fontSize: 13,
    fontWeight: 500,
    textAlign: 'center' as const,
    boxShadow: '0 2px 8px rgba(37, 99, 235, 0.3)',
  },
}

// =============================================================================
// Component
// =============================================================================

export function MessageList() {
  // State
  const containerRef = useRef<HTMLDivElement>(null)
  const scrollManagerRef = useRef<MessageScrollManager | null>(null)
  const lastScrollTop = useRef(0)
  const isPaginating = useRef(false)
  const testModeDisableAutoScroll = useRef(false)

  const [scrollManager, setScrollManager] = useState<MessageScrollManager | null>(null)
  const [items, setItems] = useState<MessageView[]>([])
  const [intersection, setIntersection] = useState<{ entityId: string; index: number } | null>(null)
  const [mode, setMode] = useState<string>('Live')
  const [hasMorePreceding, setHasMorePreceding] = useState(false)
  const [hasMoreFollowing, setHasMoreFollowing] = useState(false)
  const [shouldAutoScroll, setShouldAutoScroll] = useState(true)
  const [currentRoom, setCurrentRoom] = useState<string>('')
  const [testStatus, setTestStatus] = useState<string>('')

  // ---------------------------------------------------------------------------
  // Helpers
  // ---------------------------------------------------------------------------

  const getWasm = useCallback((): WasmBindings => {
    if (!window.wasm) throw new Error('WASM not loaded')
    return window.wasm as unknown as WasmBindings
  }, [])

  const syncState = useCallback(() => {
    const manager = scrollManagerRef.current
    if (!manager) return
    const vs = manager.visibleSet().get()
    setItems([...vs.items])
    const inter = vs.intersection()
    setIntersection(inter ? { entityId: inter.entityId, index: inter.index } : null)
    setMode(manager.mode)
    setHasMorePreceding(vs.hasMorePreceding())
    setHasMoreFollowing(vs.hasMoreFollowing())
    setShouldAutoScroll(vs.shouldAutoScroll())
  }, [])

  const findVisibleItems = useCallback((): { firstId: string; lastId: string } | null => {
    const manager = scrollManagerRef.current
    const hasItems = manager ? manager.visibleSet().get().items.length > 0 : items.length > 0
    if (!containerRef.current || !hasItems) return null

    const container = containerRef.current
    const scrollTop = container.scrollTop
    const viewportBottom = scrollTop + container.clientHeight
    const itemElements = container.querySelectorAll('[data-item-id]')
    if (itemElements.length === 0) return null

    let firstVisibleId: string | null = null
    let lastVisibleId: string | null = null

    itemElements.forEach((el) => {
      const rect = el.getBoundingClientRect()
      const containerRect = container.getBoundingClientRect()
      const itemTop = rect.top - containerRect.top + container.scrollTop
      const itemBottom = itemTop + rect.height

      if (itemBottom > scrollTop && itemTop < viewportBottom) {
        const id = el.getAttribute('data-item-id')
        if (id) {
          if (!firstVisibleId) firstVisibleId = id
          lastVisibleId = id
        }
      }
    })

    return firstVisibleId && lastVisibleId ? { firstId: firstVisibleId, lastId: lastVisibleId } : null
  }, [items.length])

  // ---------------------------------------------------------------------------
  // Event Handlers
  // ---------------------------------------------------------------------------

  const handleScroll = useCallback(() => {
    if (!scrollManager || !containerRef.current || isPaginating.current) return

    const container = containerRef.current
    const scrollTop = container.scrollTop
    const scrollingBackward = scrollTop < lastScrollTop.current
    lastScrollTop.current = scrollTop

    const visible = findVisibleItems()
    if (!visible) return

    isPaginating.current = true
    try {
      scrollManager.onScroll(visible.firstId, visible.lastId, scrollingBackward)
      syncState()
    } finally {
      setTimeout(() => { isPaginating.current = false }, 50)
    }
  }, [scrollManager, syncState, findVisibleItems])

  // ---------------------------------------------------------------------------
  // Test Helpers (exposed to window.testHelpers)
  // ---------------------------------------------------------------------------

  useEffect(() => {
    const helpers: TestHelpers = {
      // Status display
      setTestStatus: (status) => setTestStatus(status),
      clearTestStatus: () => setTestStatus(''),

      // Data management
      seedTestData: async (room, count, startTimestamp, variedHeights) => {
        await getWasm().seed_test_data(room, count, BigInt(startTimestamp), variedHeights)
      },
      clearAllMessages: async () => {
        await getWasm().clear_all_messages()
      },

      // Scroll manager lifecycle
      createScrollManager: async (room, viewportHeight) => {
        const wasm = getWasm()
        const manager = new wasm.MessageScrollManager(
          wasm.ctx(),
          `room = '${room}' AND deleted = false`,
          'timestamp DESC',
          40, 2.0, viewportHeight
        )
        await manager.start()

        const vs = manager.visibleSet().get()
        setItems([...vs.items])
        const inter = vs.intersection()
        setIntersection(inter ? { entityId: inter.entityId, index: inter.index } : null)
        setMode(manager.mode)
        setHasMorePreceding(vs.hasMorePreceding())
        setHasMoreFollowing(vs.hasMoreFollowing())
        setShouldAutoScroll(vs.shouldAutoScroll())

        scrollManagerRef.current = manager
        setScrollManager(manager)
        setCurrentRoom(room)

        return new Promise<void>((resolve) => setTimeout(resolve, 50))
      },
      destroyScrollManager: () => {
        scrollManagerRef.current = null
        setScrollManager(null)
        setItems([])
        setIntersection(null)
        setMode('Live')
        setHasMorePreceding(false)
        setHasMoreFollowing(false)
        setShouldAutoScroll(true)
      },
      jumpToLive: async () => {
        testModeDisableAutoScroll.current = false  // Re-enable auto-scroll in live mode
        if (containerRef.current) {
          containerRef.current.scrollTop = containerRef.current.scrollHeight
        }
      },
      updateFilter: async () => {
        console.warn('updateFilter is not supported in the new ScrollManager API')
      },

      // Scroll control
      setScrollTop: (value) => {
        testModeDisableAutoScroll.current = true
        if (containerRef.current) containerRef.current.scrollTop = value
      },
      getScrollTop: () => containerRef.current?.scrollTop ?? 0,
      getScrollHeight: () => containerRef.current?.scrollHeight ?? 0,
      getClientHeight: () => containerRef.current?.clientHeight ?? 0,
      scrollBy: (delta) => {
        testModeDisableAutoScroll.current = true
        if (containerRef.current) containerRef.current.scrollTop += delta
      },
      scrollToTop: () => {
        testModeDisableAutoScroll.current = true
        const container = containerRef.current || document.querySelector('[data-testid="scroll-container"]') as HTMLElement
        if (container) container.scrollTop = 0
      },
      scrollToBottom: () => {
        const container = containerRef.current || document.querySelector('[data-testid="scroll-container"]') as HTMLElement
        if (container) container.scrollTop = container.scrollHeight
      },

      // State inspection
      getItems: () => {
        const manager = scrollManagerRef.current
        const source = manager ? manager.visibleSet().get().items : items
        return source.map(item => ({
          id: item.id.toString(),
          text: item.text,
          timestamp: Number(item.timestamp),
        }))
      },
      getIntersection: () => {
        const manager = scrollManagerRef.current
        if (manager) {
          const inter = manager.visibleSet().get().intersection()
          return inter ? { entityId: inter.entityId, index: inter.index } : null
        }
        return intersection
      },
      getMode: () => scrollManagerRef.current?.mode ?? mode,
      hasMorePreceding: () => scrollManagerRef.current?.visibleSet().get().hasMorePreceding() ?? hasMorePreceding,
      hasMoreFollowing: () => scrollManagerRef.current?.visibleSet().get().hasMoreFollowing() ?? hasMoreFollowing,
      hasMoreOlder: () => scrollManagerRef.current?.visibleSet().get().hasMorePreceding() ?? hasMorePreceding,
      hasMoreNewer: () => scrollManagerRef.current?.visibleSet().get().hasMoreFollowing() ?? hasMoreFollowing,
      shouldAutoScroll: () => scrollManagerRef.current?.visibleSet().get().shouldAutoScroll() ?? shouldAutoScroll,
      isLoading: () => false,
      getItemCount: () => scrollManagerRef.current?.visibleSet().get().items.length ?? items.length,
      getCurrentSelection: () => scrollManagerRef.current?.currentSelection() ?? '',

      // Position metrics
      getItemPositions: () => {
        if (!containerRef.current) return []
        const container = containerRef.current
        return Array.from(container.querySelectorAll('[data-item-id]')).map((el) => {
          const rect = el.getBoundingClientRect()
          const containerRect = container.getBoundingClientRect()
          return {
            id: el.getAttribute('data-item-id')!,
            top: rect.top - containerRect.top + container.scrollTop,
            height: rect.height,
          }
        })
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

      // Manual scroll trigger
      triggerOnScroll: async (forceScrollingBackward?: boolean) => {
        const manager = scrollManagerRef.current
        if (!manager || !containerRef.current) return null

        const container = containerRef.current
        const topGap = container.scrollTop
        const bottomGap = container.scrollHeight - container.scrollTop - container.clientHeight
        const visible = findVisibleItems()
        if (!visible) return null

        const modeBefore = manager.mode
        let scrollingBackward: boolean
        if (forceScrollingBackward !== undefined) {
          scrollingBackward = forceScrollingBackward
        } else if (bottomGap < topGap) {
          scrollingBackward = false
        } else if (topGap < bottomGap) {
          scrollingBackward = true
        } else {
          scrollingBackward = container.scrollTop < lastScrollTop.current
        }

        lastScrollTop.current = container.scrollTop
        manager.onScroll(visible.firstId, visible.lastId, scrollingBackward)
        syncState()

        const modeAfter = manager.mode
        return modeBefore === 'Live' && modeAfter === 'Live' ? null : modeAfter
      },
    }

    window.testHelpers = helpers
    return () => { window.testHelpers = null }
  }, [getWasm, scrollManager, items, intersection, mode, hasMorePreceding, hasMoreFollowing, shouldAutoScroll, syncState, findVisibleItems])

  // Auto-scroll in live mode
  useEffect(() => {
    if (shouldAutoScroll && containerRef.current && !testModeDisableAutoScroll.current) {
      containerRef.current.scrollTop = containerRef.current.scrollHeight
    }
  }, [items, shouldAutoScroll])

  // ---------------------------------------------------------------------------
  // Render
  // ---------------------------------------------------------------------------

  return (
    <div style={styles.container}>
      {/* Header */}
      <div style={styles.header}>
        <h1 style={styles.title}>Virtual Scroll Test</h1>
        <p style={styles.subtitle}>
          {currentRoom ? `Room: ${currentRoom}` : 'No room selected'} · {items.length} messages
        </p>
      </div>

      {/* Status Bar */}
      <div style={styles.statusBar} data-testid="debug-info">
        <div style={styles.statusItem}>
          <span style={styles.statusLabel}>Mode</span>
          <span style={styles.badge(mode === 'Live', '#22c55e')}>{mode}</span>
        </div>
        <div style={styles.statusItem}>
          <span style={styles.statusLabel}>Older</span>
          <span style={styles.badge(hasMorePreceding, '#3b82f6')}>{hasMorePreceding ? 'more' : 'end'}</span>
        </div>
        <div style={styles.statusItem}>
          <span style={styles.statusLabel}>Newer</span>
          <span style={styles.badge(hasMoreFollowing, '#3b82f6')}>{hasMoreFollowing ? 'more' : 'end'}</span>
        </div>
        <div style={styles.statusItem}>
          <span style={styles.statusLabel}>Auto-scroll</span>
          <span style={styles.badge(shouldAutoScroll, '#8b5cf6')}>{shouldAutoScroll ? 'on' : 'off'}</span>
        </div>
        {intersection && (
          <div style={styles.statusItem}>
            <span style={styles.statusLabel}>Anchor</span>
            <span style={{ ...styles.statusValue, fontFamily: 'ui-monospace, monospace', fontSize: 11 }}>
              #{intersection.index}
            </span>
          </div>
        )}
      </div>

      {/* Message List */}
      <div
        ref={containerRef}
        data-testid="scroll-container"
        onScroll={handleScroll}
        style={styles.scrollContainer}
      >
        {items.length === 0 ? (
          <div style={styles.emptyState}>
            <div style={styles.emptyIcon}>📭</div>
            <div>No messages yet</div>
            <div style={{ fontSize: 12, marginTop: 4 }}>Use testHelpers to seed data</div>
          </div>
        ) : (
          items.map((item, index) => {
            const id = item.id.toString()
            const isIntersection = intersection?.index === index

            return (
              <div
                key={id}
                data-testid="message-item"
                data-item-id={id}
                data-item-index={index}
                data-timestamp={Number(item.timestamp)}
                style={styles.messageItem(isIntersection)}
              >
                <div style={styles.messageMeta}>
                  <span>#{index}</span>
                  <span>·</span>
                  <span>ts:{Number(item.timestamp)}</span>
                  <span>·</span>
                  <span>{id.slice(-8)}</span>
                </div>
                <div style={styles.messageText}>{item.text}</div>
              </div>
            )
          })
        )}
      </div>

      {/* Test Status */}
      {testStatus && (
        <div data-testid="test-status" style={styles.testStatusBar}>
          {testStatus}
        </div>
      )}
    </div>
  )
}
