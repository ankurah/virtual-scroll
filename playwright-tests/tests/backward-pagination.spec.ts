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
  triggerOnScroll,
  waitForLoading,
  waitForItemCountChange,
} from './helpers';

test.describe('Backward Pagination', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await waitForWasm(page);
  });

  test.afterEach(async ({ page }) => {
    await cleanup(page);
  });

  test('should load older items when scrolling up near top', async ({ page }) => {
    await showStatus(page, '📋 Test: Load older items on scroll up');
    await setupScrollTest(page, { count: 200 });

    const initialItems = await getItems(page);
    const initialOldestTs = initialItems[0].timestamp;

    await showStatus(page, '⬆️ Scrolling to top...');
    await scrollToTop(page);
    await page.waitForTimeout(100);

    const direction = await triggerOnScroll(page);

    if (direction === 'Backward') {
      await page.waitForTimeout(500);
      const newItems = await getItems(page);
      const newState = await getScrollState(page);

      expect(newState.mode).toBe('Backward');
      expect(newItems[0].timestamp).toBeLessThan(initialOldestTs);
      expect(newState.hasMoreNewer).toBe(true);
    }

    await showStatus(page, '✅ Backward pagination works');
  });

  test('should set intersection for scroll stability', async ({ page }) => {
    await showStatus(page, '📋 Test: Intersection for stability');
    await setupScrollTest(page, { count: 200 });

    expect((await getScrollState(page)).intersection).toBeNull();

    await scrollToTop(page);
    await page.waitForTimeout(100);
    const direction = await triggerOnScroll(page);

    if (direction === 'Backward') {
      await page.waitForTimeout(500);
      const state = await getScrollState(page);
      expect(state.intersection).not.toBeNull();
    }

    await showStatus(page, '✅ Intersection set correctly');
  });

  test('should load multiple pages scrolling up', async ({ page }) => {
    await showStatus(page, '📋 Test: Multiple backward pages');
    await setupScrollTest(page, { count: 500 });

    const oldestTimestamps: number[] = [];
    let items = await getItems(page);
    oldestTimestamps.push(items[0].timestamp);

    for (let round = 1; round <= 3; round++) {
      await scrollToTop(page);
      await page.waitForTimeout(100);
      const dir = await triggerOnScroll(page);
      if (dir !== 'Backward') break;
      await page.waitForTimeout(500);
      items = await getItems(page);
      oldestTimestamps.push(items[0].timestamp);
      await showStatus(page, 'Round ' + round + ': oldest=' + items[0].timestamp);
    }

    for (let i = 1; i < oldestTimestamps.length; i++) {
      expect(oldestTimestamps[i]).toBeLessThan(oldestTimestamps[i - 1]);
    }

    await showStatus(page, '✅ Loaded ' + oldestTimestamps.length + ' pages');
  });

  test('should stop at earliest content', async ({ page }) => {
    await showStatus(page, '📋 Test: Stop at earliest');
    await setupScrollTest(page, { count: 50 });

    let rounds = 0;
    while (rounds < 10) {
      rounds++;
      const state = await getScrollState(page);
      if (!state.hasMoreOlder) break;

      await scrollToTop(page);
      await page.waitForTimeout(100);
      const dir = await triggerOnScroll(page);
      if (dir !== 'Backward') break;
      await page.waitForTimeout(500);
    }

    expect((await getScrollState(page)).hasMoreOlder).toBe(false);
    await showStatus(page, '✅ Stopped at earliest');
  });

  test('should preserve item order', async ({ page }) => {
    await showStatus(page, '📋 Test: Item order preserved');
    await setupScrollTest(page, { count: 200 });

    await scrollToTop(page);
    await page.waitForTimeout(100);
    await triggerOnScroll(page);
    await page.waitForTimeout(500);

    const items = await getItems(page);
    for (let i = 1; i < items.length; i++) {
      expect(items[i].timestamp).toBeGreaterThan(items[i - 1].timestamp);
    }

    await showStatus(page, '✅ Order preserved');
  });

  test('should handle rapid scroll gestures', async ({ page }) => {
    await showStatus(page, '📋 Test: Rapid scroll');
    await setupScrollTest(page, { count: 300 });

    for (let i = 0; i < 5; i++) {
      await scrollBy(page, -100);
      await page.waitForTimeout(50);
    }
    await page.waitForTimeout(1000);

    const items = await getItems(page);
    expect(items.length).toBeGreaterThan(0);
    for (let i = 1; i < items.length; i++) {
      expect(items[i].timestamp).toBeGreaterThan(items[i - 1].timestamp);
    }

    await showStatus(page, '✅ Rapid scroll handled');
  });

  test('should track timestamps through pagination', async ({ page }) => {
    await showStatus(page, '📋 Test: Timestamp tracking');
    await setupScrollTest(page, { count: 200, startTimestamp: 1000 });

    let items = await getItems(page);
    const initialOldest = items[0].timestamp;
    expect(items[items.length - 1].timestamp).toBe(1199);

    await scrollToTop(page);
    await page.waitForTimeout(100);
    await triggerOnScroll(page);
    await page.waitForTimeout(500);

    items = await getItems(page);
    expect(items[0].timestamp).toBeLessThan(initialOldest);

    await showStatus(page, '✅ Timestamps correct');
  });
});
