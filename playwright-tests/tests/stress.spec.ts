import { test, expect } from '@playwright/test';
import {
  waitForWasm,
  setupScrollTest,
  getScrollState,
  getItems,
  getItemPositions,
  cleanup,
  showStatus,
  scrollTo,
  scrollBy,
  scrollToTop,
  scrollToBottom,
  triggerOnScroll,
  jumpToLive,
} from './helpers';

/**
 * Stress Tests
 *
 * These tests verify the scroll manager handles extreme and rapid usage:
 * - Rapid scrolling in both directions
 * - Large datasets
 * - Many pagination cycles
 * - Concurrent operations
 *
 * TODO: Add more stress scenarios as we discover edge cases in production:
 * - [ ] Network latency simulation
 * - [ ] Memory pressure testing
 * - [ ] Very large items (images, long text)
 * - [ ] Touch gesture simulation (for mobile)
 */

test.describe('Stress Tests - Rapid Scrolling', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await waitForWasm(page);
  });

  test.afterEach(async ({ page }) => {
    await cleanup(page);
  });

  test('should handle 20 rapid scroll events', async ({ page }) => {
    await showStatus(page, '📋 Test: 20 rapid scrolls');
    await setupScrollTest(page, { count: 500 });

    for (let i = 0; i < 20; i++) {
      const delta = i % 2 === 0 ? -50 : 50;
      await scrollBy(page, delta);
      // Minimal delay to simulate rapid user scrolling
      await page.waitForTimeout(20);
    }

    await page.waitForTimeout(1000);

    const items = await getItems(page);
    expect(items.length).toBeGreaterThan(0);

    // Items should still be in order
    for (let i = 1; i < items.length; i++) {
      expect(items[i].timestamp).toBeGreaterThan(items[i - 1].timestamp);
    }

    await showStatus(page, '✅ Handled 20 rapid scrolls');
  });

  test('should handle 50 rapid direction changes', async ({ page }) => {
    await showStatus(page, '📋 Test: 50 direction changes');
    await setupScrollTest(page, { count: 400 });

    // Rapidly alternate scroll direction
    for (let i = 0; i < 50; i++) {
      await scrollBy(page, i % 2 === 0 ? -30 : 30);
      await page.waitForTimeout(10);
    }

    await page.waitForTimeout(500);

    const state = await getScrollState(page);
    expect(state.itemCount).toBeGreaterThan(0);

    await showStatus(page, '✅ Handled 50 direction changes');
  });

  test('should handle continuous scrolling up', async ({ page }) => {
    await showStatus(page, '📋 Test: Continuous scroll up');
    await setupScrollTest(page, { count: 500 });

    // Scroll down first to have room
    await scrollToBottom(page);
    await page.waitForTimeout(200);

    // Continuous upward scrolling
    for (let i = 0; i < 30; i++) {
      await scrollBy(page, -100);
      await page.waitForTimeout(30);
    }

    await page.waitForTimeout(1000);

    const items = await getItems(page);
    for (let i = 1; i < items.length; i++) {
      expect(items[i].timestamp).toBeGreaterThan(items[i - 1].timestamp);
    }

    await showStatus(page, '✅ Continuous scroll up handled');
  });

  test('should handle continuous scrolling down', async ({ page }) => {
    await showStatus(page, '📋 Test: Continuous scroll down');
    await setupScrollTest(page, { count: 500 });

    // First go into backward mode
    await scrollToTop(page);
    await triggerOnScroll(page);
    await page.waitForTimeout(500);

    // Continuous downward scrolling
    for (let i = 0; i < 30; i++) {
      await scrollBy(page, 100);
      await page.waitForTimeout(30);
    }

    await page.waitForTimeout(1000);

    const items = await getItems(page);
    for (let i = 1; i < items.length; i++) {
      expect(items[i].timestamp).toBeGreaterThan(items[i - 1].timestamp);
    }

    await showStatus(page, '✅ Continuous scroll down handled');
  });
});

test.describe('Stress Tests - Large Datasets', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await waitForWasm(page);
  });

  test.afterEach(async ({ page }) => {
    await cleanup(page);
  });

  test('should handle 1000 items', async ({ page }) => {
    await showStatus(page, '📋 Test: 1000 items');
    await setupScrollTest(page, { count: 1000 });

    const state = await getScrollState(page);
    expect(state.itemCount).toBeGreaterThan(0);
    expect(state.hasMoreOlder).toBe(true);

    // Scroll through the content
    await scrollToTop(page);
    await triggerOnScroll(page);
    await page.waitForTimeout(500);

    await scrollToBottom(page);
    await page.waitForTimeout(200);

    const finalState = await getScrollState(page);
    expect(finalState.itemCount).toBeGreaterThan(0);

    await showStatus(page, '✅ Handled 1000 items');
  });

  test('should paginate through large dataset multiple times', async ({ page }) => {
    await showStatus(page, '📋 Test: Multiple pagination on large dataset');
    await setupScrollTest(page, { count: 1000 });

    let paginationCount = 0;

    // Try to paginate backward multiple times
    for (let i = 0; i < 10; i++) {
      const state = await getScrollState(page);
      if (!state.hasMoreOlder) break;

      await scrollToTop(page);
      await page.waitForTimeout(100);
      const dir = await triggerOnScroll(page);
      if (dir === 'Backward') {
        paginationCount++;
        await page.waitForTimeout(300);
      }
    }

    await showStatus(page, `📊 Completed ${paginationCount} backward paginations`);
    expect(paginationCount).toBeGreaterThan(0);

    await showStatus(page, '✅ Multiple paginations handled');
  });
});

test.describe('Stress Tests - Many Cycles', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await waitForWasm(page);
  });

  test.afterEach(async ({ page }) => {
    await cleanup(page);
  });

  test('should handle backward→forward→backward cycle', async ({ page }) => {
    await showStatus(page, '📋 Test: Backward→Forward→Backward cycle');
    await setupScrollTest(page, { count: 500 });

    // Backward
    await scrollToTop(page);
    await triggerOnScroll(page);
    await page.waitForTimeout(500);

    let state = await getScrollState(page);
    const modeAfterBackward = state.mode;

    // Forward
    await scrollToBottom(page);
    await triggerOnScroll(page);
    await page.waitForTimeout(500);

    state = await getScrollState(page);
    const modeAfterForward = state.mode;

    // Backward again
    await scrollToTop(page);
    await triggerOnScroll(page);
    await page.waitForTimeout(500);

    state = await getScrollState(page);

    // Verify items are still valid
    const items = await getItems(page);
    for (let i = 1; i < items.length; i++) {
      expect(items[i].timestamp).toBeGreaterThan(items[i - 1].timestamp);
    }

    await showStatus(page, '✅ Cycle completed successfully');
  });

  test('should handle 5 complete pagination cycles', async ({ page }) => {
    await showStatus(page, '📋 Test: 5 complete cycles');
    await setupScrollTest(page, { count: 500 });

    for (let cycle = 0; cycle < 5; cycle++) {
      // Go backward
      await scrollToTop(page);
      await page.waitForTimeout(100);
      await triggerOnScroll(page);
      await page.waitForTimeout(300);

      // Go forward / jump to live
      await jumpToLive(page);
      await page.waitForTimeout(200);

      await showStatus(page, `Cycle ${cycle + 1}/5 complete`, 100);
    }

    const state = await getScrollState(page);
    expect(state.mode).toBe('Live');

    await showStatus(page, '✅ 5 cycles completed');
  });

  test('should maintain data integrity after many operations', async ({ page }) => {
    await showStatus(page, '📋 Test: Data integrity after many ops');
    await setupScrollTest(page, { count: 300 });

    // Mix of operations
    const operations = [
      () => scrollToTop(page),
      () => scrollToBottom(page),
      () => scrollBy(page, -100),
      () => scrollBy(page, 100),
      () => triggerOnScroll(page),
      () => jumpToLive(page),
    ];

    for (let i = 0; i < 20; i++) {
      const op = operations[i % operations.length];
      await op();
      await page.waitForTimeout(50);
    }

    await page.waitForTimeout(1000);

    // Verify integrity
    const items = await getItems(page);
    expect(items.length).toBeGreaterThan(0);

    for (let i = 1; i < items.length; i++) {
      expect(items[i].timestamp).toBeGreaterThan(items[i - 1].timestamp);
    }

    const positions = await getItemPositions(page);
    const sorted = [...positions].sort((a, b) => a.top - b.top);
    for (let i = 1; i < sorted.length; i++) {
      expect(sorted[i].top).toBeGreaterThanOrEqual(sorted[i - 1].top);
    }

    await showStatus(page, '✅ Data integrity maintained');
  });
});

test.describe('Stress Tests - Edge Timing', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await waitForWasm(page);
  });

  test.afterEach(async ({ page }) => {
    await cleanup(page);
  });

  test('should handle scroll during state transition', async ({ page }) => {
    await showStatus(page, '📋 Test: Scroll during transition');
    await setupScrollTest(page, { count: 300 });

    // Start backward pagination
    await scrollToTop(page);
    const paginationPromise = triggerOnScroll(page);

    // Immediately scroll more
    await scrollBy(page, -50);
    await scrollBy(page, -50);

    await paginationPromise;
    await page.waitForTimeout(500);

    // Should still work
    const items = await getItems(page);
    expect(items.length).toBeGreaterThan(0);

    await showStatus(page, '✅ Handled scroll during transition');
  });

  test('should handle rapid jumpToLive calls', async ({ page }) => {
    await showStatus(page, '📋 Test: Rapid jumpToLive');
    await setupScrollTest(page, { count: 200 });

    // Enter backward mode
    await scrollToTop(page);
    await triggerOnScroll(page);
    await page.waitForTimeout(500);

    // Rapid jumpToLive calls
    for (let i = 0; i < 10; i++) {
      await jumpToLive(page);
      await page.waitForTimeout(20);
    }

    const state = await getScrollState(page);
    expect(state.mode).toBe('Live');

    await showStatus(page, '✅ Rapid jumpToLive handled');
  });
});
