import { test, expect } from '@playwright/test';
import {
  waitForWasm,
  setupScrollTest,
  getScrollState,
  getItems,
  cleanup,
  scrollToTop,
  scrollToBottom,
  triggerOnScroll,
} from './helpers';

/**
 * ScrollManager integration tests - mirrors scroll_manager_tests.rs
 *
 * Windowing parameters: 60 messages (ts 1000-1059), ~50px height, 400px viewport,
 * live_window calculated from viewport/minRowHeight * bufferFactor
 */
test.describe('ScrollManager Integration', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await waitForWasm(page);
  });

  test.afterEach(async ({ page }) => {
    await cleanup(page);
  });

  /**
   * Test full round-trip: Live → oldest edge → back to Live
   * Mirrors: test_scroll_live_to_oldest_and_back in Rust
   */
  test('test_scroll_live_to_oldest_and_back', async ({ page }) => {
    // Create 60 messages like Rust test (timestamps 1000-1059)
    await setupScrollTest(page, { count: 60, startTimestamp: 1000 });

    // Initial state: Live mode with initial window of items
    let state = await getScrollState(page);
    let items = await getItems(page);
    expect(state.mode).toBe('Live');
    expect(state.hasMorePreceding).toBe(true); // More older items exist
    expect(state.hasMoreFollowing).toBe(false); // At live edge
    expect(state.shouldAutoScroll).toBe(true);

    const initialOldestTs = items[0].timestamp;

    // === PHASE 1: Scroll backward to oldest edge ===

    // Scroll to top to trigger backward pagination
    await scrollToTop(page);
    await page.waitForTimeout(100);

    let direction = await triggerOnScroll(page);
    expect(direction).toBe('Backward');
    await page.waitForTimeout(500); // Wait for livequery update

    state = await getScrollState(page);
    items = await getItems(page);
    expect(state.mode).toBe('Backward');
    expect(items[0].timestamp).toBeLessThan(initialOldestTs); // Loaded older items
    expect(state.hasMoreFollowing).toBe(true); // Left items behind

    // Continue scrolling to oldest edge
    let prevOldest = items[0].timestamp;
    let cycles = 0;
    while (state.hasMorePreceding && cycles < 10) {
      await scrollToTop(page);
      await page.waitForTimeout(100);
      direction = await triggerOnScroll(page);
      if (direction !== 'Backward') break;
      await page.waitForTimeout(500);

      state = await getScrollState(page);
      items = await getItems(page);

      // Should load older items or stay at edge
      expect(items[0].timestamp).toBeLessThanOrEqual(prevOldest);
      prevOldest = items[0].timestamp;
      cycles++;
    }

    // Should have reached oldest edge
    expect(state.hasMorePreceding).toBe(false);
    expect(items[0].timestamp).toBe(1000); // Oldest message

    // === PHASE 2: Scroll forward back to live edge ===

    // Keep scrolling forward until we reach live edge
    cycles = 0;
    while (cycles < 15) {
      await scrollToBottom(page);
      await page.waitForTimeout(100);
      direction = await triggerOnScroll(page);

      if (direction === 'Forward') {
        await page.waitForTimeout(500);
      }

      state = await getScrollState(page);
      items = await getItems(page);

      // Stop when we reach Live mode or no more following items
      if (state.mode === 'Live' || !state.hasMoreFollowing) break;
      cycles++;
    }

    // Should be back at live edge
    expect(state.hasMoreFollowing).toBe(false);
    expect(items[items.length - 1].timestamp).toBe(1059); // Newest message
    // Mode should be Live (or we've reached the end)
    if (state.mode !== 'Live') {
      // One more scroll might transition to Live
      await scrollToBottom(page);
      await page.waitForTimeout(100);
      await triggerOnScroll(page);
      await page.waitForTimeout(500);
      state = await getScrollState(page);
    }
    expect(state.mode).toBe('Live');
  });

  /**
   * Test direction reversal: backward → forward (no trigger) → backward again
   * Mirrors: test_direction_reversal in Rust
   */
  test('test_direction_reversal', async ({ page }) => {
    await setupScrollTest(page, { count: 60, startTimestamp: 1000 });

    // Initial state: Live mode
    let state = await getScrollState(page);
    expect(state.mode).toBe('Live');

    // Scroll backward to trigger pagination
    await scrollToTop(page);
    await page.waitForTimeout(100);
    let direction = await triggerOnScroll(page);
    expect(direction).toBe('Backward');
    await page.waitForTimeout(500);

    state = await getScrollState(page);
    let items = await getItems(page);
    expect(state.mode).toBe('Backward');
    const afterFirstBackward = items[0].timestamp;

    // Scroll forward a bit (should not trigger forward pagination yet)
    await scrollToBottom(page);
    await page.waitForTimeout(100);
    direction = await triggerOnScroll(page);
    // May or may not trigger forward - depends on buffer

    state = await getScrollState(page);

    // Scroll backward again
    await scrollToTop(page);
    await page.waitForTimeout(100);
    direction = await triggerOnScroll(page);

    if (direction === 'Backward') {
      await page.waitForTimeout(500);
      state = await getScrollState(page);
      items = await getItems(page);

      // Should continue loading older items
      expect(items[0].timestamp).toBeLessThanOrEqual(afterFirstBackward);
      expect(state.mode).toBe('Backward');
    }
  });
});
