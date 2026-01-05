import { useEffect, useState, useRef } from 'react'
import { MessageList } from './components/MessageList'

// Import will be from WASM bindings once built
declare global {
  interface Window {
    wasm: typeof import('../../wasm-bindings/pkg') | null
    wasmReady: Promise<void>
    testHelpers: TestHelpers | null
  }
}

export interface TestHelpers {
  // Test status display (for visual debugging)
  setTestStatus: (status: string) => void
  clearTestStatus: () => void

  // Data management
  seedTestData: (room: string, count: number, startTimestamp: number, variedHeights: boolean) => Promise<void>
  clearAllMessages: () => Promise<void>

  // Scroll manager control
  createScrollManager: (room: string, viewportHeight: number) => Promise<void>
  destroyScrollManager: () => void
  jumpToLive: () => Promise<void>
  updateFilter: (predicate: string, resetPosition: boolean) => Promise<void>

  // Scroll control (precise)
  setScrollTop: (value: number) => void
  getScrollTop: () => number
  getScrollHeight: () => number
  getClientHeight: () => number
  scrollBy: (delta: number) => void
  scrollToTop: () => void
  scrollToBottom: () => void

  // State inspection
  getItems: () => Array<{ id: string; text: string; timestamp: number }>
  getIntersection: () => { entityId: string; index: number } | null
  getMode: () => string
  hasMorePreceding: () => boolean
  hasMoreFollowing: () => boolean
  // Legacy names for backwards compatibility
  hasMoreOlder: () => boolean
  hasMoreNewer: () => boolean
  shouldAutoScroll: () => boolean
  isLoading: () => boolean
  getItemCount: () => number
  getCurrentSelection: () => string

  // Metrics for scroll stability testing
  getItemPositions: () => Array<{ id: string; top: number; height: number }>
  getItemById: (id: string) => { top: number; height: number } | null

  // Scroll event triggering
  triggerOnScroll: (forceScrollingUp?: boolean) => Promise<string | null> // returns load direction or null
}

function App() {
  const [wasmLoaded, setWasmLoaded] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const initStarted = useRef(false)

  useEffect(() => {
    // Guard against double initialization in React Strict Mode
    if (initStarted.current) {
      console.log('WASM init already started, skipping...')
      return
    }
    initStarted.current = true

    // Load WASM module
    const loadWasm = async () => {
      try {
        console.log('Loading WASM module...')
        const wasm = await import('../../wasm-bindings/pkg')
        console.log('WASM module imported, calling default()...')
        await wasm.default() // Initialize WASM (calls #[wasm_bindgen(start)])
        console.log('WASM default() done, calling ready()...')
        await wasm.ready() // Wait for node to be fully initialized
        console.log('WASM ready() done, setting window.wasm')
        window.wasm = wasm
        setWasmLoaded(true)
      } catch (e) {
        console.error('Failed to load WASM:', e)
        setError(String(e))
      }
    }

    // Create a promise that resolves when WASM is ready (only call loadWasm once)
    window.wasmReady = loadWasm()
  }, [])

  if (error) {
    return (
      <div style={{ padding: 20, color: 'red' }}>
        <h1>WASM Load Error</h1>
        <pre>{error}</pre>
      </div>
    )
  }

  if (!wasmLoaded) {
    return (
      <div style={{ padding: 20 }}>
        <h1>Loading WASM...</h1>
      </div>
    )
  }

  return <MessageList />
}

export default App
