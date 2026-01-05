import { test, expect } from '@playwright/test';
import {
  waitForWasm,
  setupScrollTest,
  getScrollState,
  getItems,
  getItemPositions,
  cleanup,
  scrollToTop,
  scrollBy,
  triggerOnScroll,
} from './helpers';

/**
 * Variable height item tests for ScrollManager - mirrors variable_height_tests.rs
 *
 * Tests with items of varying heights to verify:
 * - Trigger conditions work correctly (based on item count, not pixels)
 * - Intersection anchoring maintains visual stability with varying heights
 * - Visible range calculations handle varying item sizes
 */
test.describe('Variable Height Tests', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await waitForWasm(page);
  });

  test.afterEach(async ({ page }) => {
    await cleanup(page);
  });

  /**
   * Test with variable height items.
   * Heights vary based on message content length (short, medium, long, very long).
   * Mirrors: test_variable_heights in Rust
   */
  test('test_variable_heights', async ({ page }) => {
    // Create 60 items with varied heights
    await setupScrollTest(page, { count: 60, startTimestamp: 1000, variedHeights: true });

    // Initial: Live mode
    let state = await getScrollState(page);
    let items = await getItems(page);
    expect(state.mode).toBe('Live');
    expect(state.shouldAutoScroll).toBe(true);

    // Verify we have items with different heights
    let positions = await getItemPositions(page);
    const heights = new Set(positions.map((p) => Math.round(p.height)));
    // With varied heights, we should have more than one height value
    expect(heights.size).toBeGreaterThan(1);

    // Initial state should be auto-scrolled to bottom
    const distanceFromBottom = state.scrollHeight - state.scrollTop - state.clientHeight;
    expect(distanceFromBottom).toBeLessThan(50);

    // Scroll up to trigger backward pagination
    // With varied heights, the trigger is based on item count, not pixels
    await scrollToTop(page);
    await page.waitForTimeout(100);

    const direction = await triggerOnScroll(page);

    if (direction === 'Backward') {
      await page.waitForTimeout(500);

      state = await getScrollState(page);
      items = await getItems(page);

      // Should have loaded older items
      expect(state.mode).toBe('Backward');

      // Intersection should be set for scroll stability
      expect(state.intersection).not.toBeNull();

      // Verify intersection item exists in the current window
      const intersectionIndex = state.intersection!.index;
      expect(intersectionIndex).toBeGreaterThanOrEqual(0);
      expect(intersectionIndex).toBeLessThan(items.length);

      // Items should still be sorted by timestamp
      for (let i = 1; i < items.length; i++) {
        expect(items[i].timestamp).toBeGreaterThan(items[i - 1].timestamp);
      }

      // Heights should still vary in the new window
      positions = await getItemPositions(page);
      const newHeights = new Set(positions.map((p) => Math.round(p.height)));
      expect(newHeights.size).toBeGreaterThan(1);
    }
  });
});
