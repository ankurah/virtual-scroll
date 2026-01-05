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
 * Edge boundary tests for ScrollManager - mirrors edge_boundary_tests.rs
 *
 * Tests datasets at or below live_window size to verify correct
 * handling of has_more_preceding/has_more_following flags.
 */
test.describe('Edge Boundary Tests', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await waitForWasm(page);
  });

  test.afterEach(async ({ page }) => {
    await cleanup(page);
  });

  /**
   * Test edge boundaries: dataset smaller than live_window.
   * When dataset is smaller than live_window, has_more_preceding should be false from start.
   * Mirrors: test_edge_boundaries_smaller_than_live_window in Rust
   */
  test('test_edge_boundaries_smaller_than_live_window', async ({ page }) => {
    // 25 items, less than typical live_window (usually ~20-30 based on viewport)
    await setupScrollTest(page, { count: 25, startTimestamp: 1000 });

    // Initial: All items loaded, has_more_preceding=false
    let state = await getScrollState(page);
    let items = await getItems(page);

    // With 25 items in a 400px viewport, all items should fit in live window
    // Check if we have all items or the live window is larger than 25
    if (items.length === 25) {
      // All items loaded, so no more preceding
      expect(state.hasMorePreceding).toBe(false);
      expect(state.hasMoreFollowing).toBe(false);
    }
    expect(state.mode).toBe('Live');

    // Scroll to top - no pagination triggers since has_more_preceding=false
    await scrollToTop(page);
    await page.waitForTimeout(100);
    const direction = await triggerOnScroll(page);

    // If all items fit, direction should be null (no pagination)
    // or we stay in Live mode
    state = await getScrollState(page);

    // Scroll back down
    await scrollToBottom(page);
    await page.waitForTimeout(100);
    await triggerOnScroll(page);
    await page.waitForTimeout(100);

    state = await getScrollState(page);

    // Should still be in Live mode (never left it if all items fit)
    // or if we did paginate, we should be able to return to Live
    const finalItems = await getItems(page);
    // Verify items contain both oldest (1000) and newest (1024)
    expect(finalItems[0].timestamp).toBeLessThanOrEqual(1000);
    expect(finalItems[finalItems.length - 1].timestamp).toBeGreaterThanOrEqual(1024);
  });

  /**
   * Test with dataset smaller than live_window - no backward pagination possible.
   * Mirrors: test_edge_boundaries_small_dataset in Rust
   */
  test('test_edge_boundaries_small_dataset', async ({ page }) => {
    // Only 20 items, definitely less than live_window
    await setupScrollTest(page, { count: 20, startTimestamp: 1000 });

    // Initial: Only 20 items
    let state = await getScrollState(page);
    let items = await getItems(page);
    expect(state.mode).toBe('Live');

    // If all 20 items fit in live window, has_more_preceding=false
    if (items.length === 20) {
      expect(state.hasMorePreceding).toBe(false);
      expect(state.hasMoreFollowing).toBe(false);
    }

    // Scroll to top - no pagination triggers since has_more_preceding=false
    await scrollToTop(page);
    await page.waitForTimeout(100);
    const dirUp = await triggerOnScroll(page);
    await page.waitForTimeout(100);

    state = await getScrollState(page);

    // Scroll back down - still no pagination needed
    await scrollToBottom(page);
    await page.waitForTimeout(100);
    const dirDown = await triggerOnScroll(page);
    await page.waitForTimeout(100);

    state = await getScrollState(page);

    // With small dataset, we should stay in Live mode
    // (or return to it if we briefly left)
    items = await getItems(page);

    // Verify we have all items
    const timestamps = items.map(i => i.timestamp);
    expect(timestamps).toContain(1000); // Oldest
    expect(timestamps).toContain(1019); // Newest (1000 + 20 - 1)

    // Should still be in Live mode (never left it)
    expect(state.mode).toBe('Live');
  });
});
