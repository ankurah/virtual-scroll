import { test, expect } from '@playwright/test';
import {
  waitForWasm,
  setupScrollTest,
  getScrollState,
  getItems,
  cleanup,
  scrollToBottom,
  showStatus,
  clearStatus,
} from './helpers';

test.describe('Initial Load and Live Mode', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await waitForWasm(page);
  });

  test.afterEach(async ({ page }) => {
    await cleanup(page);
  });

  test('should load initial items in live mode', async ({ page }) => {
    await showStatus(page, '📋 Test: Loading initial items in live mode');

    await showStatus(page, '🔄 Seeding 50 messages...');
    await setupScrollTest(page, { count: 50 });

    await showStatus(page, '✅ Checking live mode state...');
    const state = await getScrollState(page);
    expect(state.mode).toBe('Live');
    expect(state.itemCount).toBeGreaterThan(0);
    expect(state.shouldAutoScroll).toBe(true);
    // In live mode at the latest, there shouldn't be more newer items
    expect(state.hasMoreNewer).toBe(false);

    await showStatus(page, '✅ Test passed: Initial load works correctly');
  });

  test('should show items in chronological order (oldest first)', async ({
    page,
  }) => {
    await showStatus(page, '📋 Test: Checking chronological order');

    await showStatus(page, '🔄 Seeding 50 messages...');
    await setupScrollTest(page, { count: 50, startTimestamp: 1000 });

    await showStatus(page, '🔍 Verifying timestamp order...');
    const items = await getItems(page);
    expect(items.length).toBeGreaterThan(0);

    // Items should be in ascending timestamp order (oldest at top)
    for (let i = 1; i < items.length; i++) {
      expect(items[i].timestamp).toBeGreaterThan(items[i - 1].timestamp);
    }

    await showStatus(page, '✅ Test passed: Items are in chronological order');
  });

  test('should auto-scroll to bottom in live mode', async ({ page }) => {
    await showStatus(page, '📋 Test: Auto-scroll to bottom');

    await showStatus(page, '🔄 Seeding 50 messages...');
    await setupScrollTest(page, { count: 50 });

    await showStatus(page, '📏 Measuring scroll position...');
    const state = await getScrollState(page);
    const distanceFromBottom =
      state.scrollHeight - state.scrollTop - state.clientHeight;

    // Should be at or very near the bottom (allowing for some padding)
    expect(distanceFromBottom).toBeLessThan(50);

    await showStatus(page, '✅ Test passed: Auto-scrolled to bottom');
  });

  test('should have hasMoreOlder true when there are more items', async ({
    page,
  }) => {
    await showStatus(page, '📋 Test: hasMoreOlder flag');

    await showStatus(page, '🔄 Seeding 200 messages...');
    await setupScrollTest(page, { count: 200 });

    await showStatus(page, '🔍 Checking hasMoreOlder flag...');
    const state = await getScrollState(page);
    expect(state.hasMoreOlder).toBe(true);

    await showStatus(page, '✅ Test passed: hasMoreOlder is true');
  });

  test('should show correct item count', async ({ page }) => {
    await showStatus(page, '📋 Test: Item count');

    await showStatus(page, '🔄 Seeding 30 messages...');
    await setupScrollTest(page, { count: 30 });

    await showStatus(page, '🔢 Counting items...');
    const state = await getScrollState(page);
    expect(state.itemCount).toBeGreaterThan(0);
    expect(state.itemCount).toBeLessThanOrEqual(30);

    await showStatus(page, `✅ Test passed: ${state.itemCount} items loaded`);
  });

  test('should display correct item content', async ({ page }) => {
    await showStatus(page, '📋 Test: Item content');

    await showStatus(page, '🔄 Seeding 10 messages...');
    await setupScrollTest(page, { count: 10, variedHeights: false });

    await showStatus(page, '🔍 Verifying item properties...');
    const items = await getItems(page);
    expect(items.length).toBeGreaterThan(0);

    for (const item of items) {
      expect(item.id).toBeTruthy();
      expect(item.text).toBeTruthy();
      expect(typeof item.timestamp).toBe('number');
    }

    await showStatus(page, '✅ Test passed: All items have correct content');
  });

  test('should handle empty room gracefully', async ({ page }) => {
    await showStatus(page, '📋 Test: Empty room handling');

    await showStatus(page, '🗑️ Clearing all messages...');
    await page.evaluate(async () => {
      const helpers = window.testHelpers!;
      await helpers.clearAllMessages();
      await helpers.createScrollManager('empty-room', 400);
    });

    await page.waitForTimeout(500);

    await showStatus(page, '🔍 Checking empty state...');
    const state = await getScrollState(page);
    expect(state.itemCount).toBe(0);
    expect(state.mode).toBe('Live');

    await showStatus(page, '✅ Test passed: Empty room handled correctly');
  });

  test('intersection should be null on initial load (only set during pagination)', async ({
    page,
  }) => {
    await showStatus(page, '📋 Test: Intersection on initial load');

    await showStatus(page, '🔄 Seeding 50 messages...');
    await setupScrollTest(page, { count: 50 });

    await showStatus(page, '🔍 Checking intersection state...');
    const state = await getScrollState(page);
    expect(state.intersection).toBeNull();
    expect(state.itemCount).toBeGreaterThan(0);

    await showStatus(page, '✅ Test passed: Intersection is null on initial load');
  });
});
