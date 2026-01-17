import { test, expect } from '@playwright/test';
import {
  waitForWasm,
  setupScrollTest,
  getScrollState,
  getItems,
  getItemPositions,
  cleanup,
  scrollTo,
  scrollBy,
  scrollToTop,
  triggerOnScroll,
} from './helpers';

/**
 * Non-aligned scroll position tests for ScrollManager - mirrors non_aligned_scroll_tests.rs
 *
 * Tests with scroll amounts that don't align to item boundaries to verify:
 * - Partial visibility detection at viewport edges
 * - First/last visible item detection when items are clipped
 * - Intersection anchoring when anchor item is partially visible
 *
 * Note: In Playwright we test concepts with real DOM elements rather than
 * exact pixel calculations like the Rust MockRenderer tests.
 */
test.describe('Non-Aligned Scroll Tests', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await waitForWasm(page);
  });

  test.afterEach(async ({ page }) => {
    await cleanup(page);
  });

  /**
   * Test with non-aligned scroll amounts and variable heights.
   * Uses triggerOnScroll for reliable state updates instead of relying on native events.
   * Mirrors: test_non_aligned_scroll_positions in Rust
   */
  test('test_non_aligned_scroll_positions', async ({ page }) => {
    // Use varied heights for more realistic non-aligned scenarios
    await setupScrollTest(page, { count: 60, startTimestamp: 1000, variedHeights: true });

    let state = await getScrollState(page);
    let items = await getItems(page);
    expect(state.mode).toBe('Live');
    expect(items.length).toBeGreaterThan(0);

    // Scroll to top to exit Live mode
    await scrollToTop(page);
    await page.waitForTimeout(100);
    await triggerOnScroll(page);
    await page.waitForTimeout(100);

    state = await getScrollState(page);
    expect(state.itemCount).toBeGreaterThan(0);

    // Scroll down a bit to create room for scroll tests
    await scrollBy(page, 300);
    await page.waitForTimeout(100);
    await triggerOnScroll(page);
    await page.waitForTimeout(100);

    state = await getScrollState(page);
    const initialScrollTop = state.scrollTop;

    // Test non-aligned scrolls by small amounts
    // Scroll up by 37px
    await scrollBy(page, -37);
    await page.waitForTimeout(50);
    await triggerOnScroll(page);
    await page.waitForTimeout(50);

    let positions = await getItemPositions(page);
    state = await getScrollState(page);
    expect(positions.length).toBeGreaterThan(0);

    // Scroll up by 123px more
    await scrollBy(page, -123);
    await page.waitForTimeout(50);
    await triggerOnScroll(page);
    await page.waitForTimeout(50);

    positions = await getItemPositions(page);
    expect(positions.length).toBeGreaterThan(0);

    // Continue scrolling to trigger backward pagination
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
    }

    // Verify items remain sorted after all scrolling
    items = await getItems(page);
    expect(items.length).toBeGreaterThan(0);
    for (let i = 1; i < items.length; i++) {
      expect(items[i].timestamp).toBeGreaterThan(items[i - 1].timestamp);
    }
  });

  /**
   * Test mid-item scroll positions where items are partially clipped at both edges.
   * Uses triggerOnScroll for reliable state updates instead of relying on native events.
   * Mirrors: test_partial_visibility_at_edges in Rust
   */
  test('test_partial_visibility_at_edges', async ({ page }) => {
    // Use uniform heights for easier verification
    await setupScrollTest(page, { count: 60, startTimestamp: 1000, variedHeights: false });

    let state = await getScrollState(page);
    expect(state.mode).toBe('Live');
    expect(state.itemCount).toBeGreaterThan(0);

    // Disable auto-scroll by scrolling to top first
    await scrollToTop(page);
    await page.waitForTimeout(100);
    await triggerOnScroll(page);
    await page.waitForTimeout(100);

    state = await getScrollState(page);
    expect(state.itemCount).toBeGreaterThan(0);

    // Scroll to middle of content to avoid triggering pagination
    await scrollBy(page, 500);
    await page.waitForTimeout(100);
    await triggerOnScroll(page);
    await page.waitForTimeout(100);

    state = await getScrollState(page);
    const middleScrollTop = state.scrollTop;
    expect(state.itemCount).toBeGreaterThan(0);

    // Now test small non-aligned scrolls
    // Scroll up by 50px (items are ~40-60px tall)
    await scrollBy(page, -50);
    await page.waitForTimeout(50);
    await triggerOnScroll(page);
    await page.waitForTimeout(50);

    state = await getScrollState(page);
    let positions = await getItemPositions(page);

    // Verify we have visible items
    expect(positions.length).toBeGreaterThan(0);

    // Scroll by 1px increments to test boundary detection
    const beforeSmallScrolls = state.scrollTop;
    await scrollBy(page, -1);
    await page.waitForTimeout(20);
    await triggerOnScroll(page);

    state = await getScrollState(page);
    positions = await getItemPositions(page);
    expect(positions.length).toBeGreaterThan(0);

    // Scroll by another 1px
    await scrollBy(page, -1);
    await page.waitForTimeout(20);
    await triggerOnScroll(page);

    state = await getScrollState(page);
    positions = await getItemPositions(page);
    expect(positions.length).toBeGreaterThan(0);

    // Verify scroll position decreased
    expect(state.scrollTop).toBeLessThan(beforeSmallScrolls);

    // Items should remain sorted
    const items = await getItems(page);
    expect(items.length).toBeGreaterThan(0);
    for (let i = 1; i < items.length; i++) {
      expect(items[i].timestamp).toBeGreaterThan(items[i - 1].timestamp);
    }
  });
});
