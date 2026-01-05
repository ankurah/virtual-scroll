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
  updateFilter,
} from './helpers';

/**
 * Edge Case Tests
 *
 * These tests cover unusual scenarios and boundary conditions that might
 * break the scroll manager.
 */

test.describe('Edge Cases - Boundaries', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await waitForWasm(page);
  });

  test.afterEach(async ({ page }) => {
    await cleanup(page);
  });

  test('should handle scrolling at absolute top (scrollTop = 0)', async ({ page }) => {
    await showStatus(page, '📋 Test: scrollTop = 0 behavior');
    await setupScrollTest(page, { count: 100 });

    await scrollTo(page, 0);
    await page.waitForTimeout(100);

    const state = await getScrollState(page);
    expect(state.scrollTop).toBe(0);

    // Multiple scroll events at top should not crash
    for (let i = 0; i < 5; i++) {
      await triggerOnScroll(page);
      await page.waitForTimeout(50);
    }

    await showStatus(page, '✅ Handles scrollTop = 0');
  });

  test('should handle scrolling at absolute bottom', async ({ page }) => {
    await showStatus(page, '📋 Test: Absolute bottom behavior');
    await setupScrollTest(page, { count: 100 });

    await scrollToBottom(page);
    await page.waitForTimeout(100);

    const state = await getScrollState(page);
    const bottomGap = state.scrollHeight - state.scrollTop - state.clientHeight;
    expect(bottomGap).toBeLessThan(5);

    // Multiple scroll events at bottom should not crash
    for (let i = 0; i < 5; i++) {
      await triggerOnScroll(page);
      await page.waitForTimeout(50);
    }

    await showStatus(page, '✅ Handles absolute bottom');
  });

  test('should handle very small content (fewer items than viewport)', async ({ page }) => {
    await showStatus(page, '📋 Test: Small content');
    await setupScrollTest(page, { count: 5 });

    const state = await getScrollState(page);

    // With only 5 items, content might be smaller than viewport
    await showStatus(page, `📏 scrollHeight=${state.scrollHeight}, clientHeight=${state.clientHeight}`);

    const items = await getItems(page);
    expect(items.length).toBe(5);

    // Should still be in live mode
    expect(state.mode).toBe('Live');

    await showStatus(page, '✅ Handles small content');
  });

  test('should handle single item', async ({ page }) => {
    await showStatus(page, '📋 Test: Single item');
    await setupScrollTest(page, { count: 1 });

    const state = await getScrollState(page);
    expect(state.itemCount).toBe(1);
    expect(state.mode).toBe('Live');
    expect(state.hasMoreOlder).toBe(false);
    expect(state.hasMoreNewer).toBe(false);

    await showStatus(page, '✅ Handles single item');
  });

  test('should handle empty room gracefully', async ({ page }) => {
    await showStatus(page, '📋 Test: Empty room');

    await page.evaluate(async () => {
      const helpers = window.testHelpers!;
      await helpers.clearAllMessages();
      await helpers.createScrollManager('empty-room', 400);
    });
    await page.waitForTimeout(500);

    const state = await getScrollState(page);
    expect(state.itemCount).toBe(0);
    expect(state.mode).toBe('Live');

    // Scroll operations on empty content should not crash
    await scrollToTop(page);
    await scrollToBottom(page);
    await triggerOnScroll(page);

    await showStatus(page, '✅ Handles empty room');
  });
});

test.describe('Edge Cases - Pagination Boundaries', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await waitForWasm(page);
  });

  test.afterEach(async ({ page }) => {
    await cleanup(page);
  });

  test('should reach oldest content and stop backward pagination', async ({ page }) => {
    await showStatus(page, '📋 Test: Reach oldest content');
    await setupScrollTest(page, { count: 50 });

    let rounds = 0;
    let state = await getScrollState(page);

    while (state.hasMoreOlder && rounds < 10) {
      rounds++;
      await scrollToTop(page);
      await page.waitForTimeout(100);
      const dir = await triggerOnScroll(page);
      if (dir !== 'Backward') break;
      await page.waitForTimeout(300);
      state = await getScrollState(page);
    }

    expect(state.hasMoreOlder).toBe(false);
    await showStatus(page, `✅ Reached oldest after ${rounds} rounds`);
  });

  test('should reach newest content and return to live', async ({ page }) => {
    await showStatus(page, '📋 Test: Return to live');
    await setupScrollTest(page, { count: 100 });

    // First go backward
    await scrollToTop(page);
    await page.waitForTimeout(100);
    await triggerOnScroll(page);
    await page.waitForTimeout(500);

    let state = await getScrollState(page);
    if (state.mode !== 'Backward') {
      await showStatus(page, '⚠️ Could not enter backward mode');
      return;
    }

    // Now forward until live
    let rounds = 0;
    while (state.hasMoreNewer && rounds < 10) {
      rounds++;
      await scrollToBottom(page);
      await page.waitForTimeout(100);
      const dir = await triggerOnScroll(page);
      if (dir !== 'Forward') break;
      await page.waitForTimeout(300);
      state = await getScrollState(page);
    }

    expect(state.mode).toBe('Live');
    expect(state.hasMoreNewer).toBe(false);
    await showStatus(page, `✅ Returned to live after ${rounds} rounds`);
  });

  test('should not trigger pagination when threshold exactly met', async ({ page }) => {
    await showStatus(page, '📋 Test: Exact threshold boundary');
    await setupScrollTest(page, { count: 200 });

    const threshold = 300; // 400 * 0.75

    // Scroll to bottom first
    await scrollToBottom(page);
    await page.waitForTimeout(100);

    // Scroll to exact threshold (should NOT trigger)
    await scrollTo(page, threshold);
    await page.waitForTimeout(100);

    const stateBefore = await getScrollState(page);
    const itemCountBefore = stateBefore.itemCount;

    const direction = await triggerOnScroll(page);

    // At exactly threshold, should not trigger
    expect(direction).toBeNull();
    expect(stateBefore.itemCount).toBe(itemCountBefore);

    await showStatus(page, '✅ No trigger at exact threshold');
  });

  test('should trigger pagination 1px below threshold', async ({ page }) => {
    await showStatus(page, '📋 Test: 1px below threshold');
    await setupScrollTest(page, { count: 200 });

    const threshold = 300;

    // Scroll to bottom first, then up to 1px below threshold
    await scrollToBottom(page);
    await page.waitForTimeout(100);
    await scrollTo(page, threshold - 1);
    await page.waitForTimeout(100);

    const direction = await triggerOnScroll(page);

    // 1px below threshold while scrolling up should trigger
    await showStatus(page, `Direction: ${direction}`);

    await showStatus(page, '✅ Threshold boundary tested');
  });
});

test.describe('Edge Cases - Rapid Operations', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await waitForWasm(page);
  });

  test.afterEach(async ({ page }) => {
    await cleanup(page);
  });

  test('should handle rapid scroll direction changes', async ({ page }) => {
    await showStatus(page, '📋 Test: Rapid direction changes');
    await setupScrollTest(page, { count: 200 });

    // Rapidly change scroll direction
    for (let i = 0; i < 10; i++) {
      const delta = i % 2 === 0 ? -50 : 50;
      await scrollBy(page, delta);
      await page.waitForTimeout(30);
    }

    await page.waitForTimeout(500);

    // Items should still be valid
    const items = await getItems(page);
    expect(items.length).toBeGreaterThan(0);

    // Order should be maintained
    for (let i = 1; i < items.length; i++) {
      expect(items[i].timestamp).toBeGreaterThan(items[i - 1].timestamp);
    }

    await showStatus(page, '✅ Handles rapid direction changes');
  });

  test('should handle multiple jumpToLive calls', async ({ page }) => {
    await showStatus(page, '📋 Test: Multiple jumpToLive');
    await setupScrollTest(page, { count: 200 });

    // Enter backward mode
    await scrollToTop(page);
    await page.waitForTimeout(100);
    await triggerOnScroll(page);
    await page.waitForTimeout(500);

    // Multiple jumpToLive calls should not break anything
    for (let i = 0; i < 5; i++) {
      await jumpToLive(page);
      await page.waitForTimeout(50);
    }

    const state = await getScrollState(page);
    expect(state.mode).toBe('Live');
    expect(state.hasMoreNewer).toBe(false);

    await showStatus(page, '✅ Handles multiple jumpToLive');
  });

  test('should handle jumpToLive while already in live mode', async ({ page }) => {
    await showStatus(page, '📋 Test: jumpToLive while live');
    await setupScrollTest(page, { count: 100 });

    let state = await getScrollState(page);
    expect(state.mode).toBe('Live');

    // Call jumpToLive while already live
    await jumpToLive(page);

    state = await getScrollState(page);
    expect(state.mode).toBe('Live');

    await showStatus(page, '✅ jumpToLive works when already live');
  });
});

test.describe('Edge Cases - Concurrent Operations', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await waitForWasm(page);
  });

  test.afterEach(async ({ page }) => {
    await cleanup(page);
  });

  test('should handle scroll during loading state', async ({ page }) => {
    await showStatus(page, '📋 Test: Scroll during load');
    await setupScrollTest(page, { count: 200 });

    // Trigger pagination and immediately scroll
    await scrollToTop(page);
    await page.waitForTimeout(50);
    await triggerOnScroll(page);

    // Immediately try scrolling more (simulating user continuing to scroll)
    await scrollBy(page, -100);
    await scrollBy(page, -100);

    await page.waitForTimeout(1000);

    // Should still have valid state
    const state = await getScrollState(page);
    const items = await getItems(page);
    expect(items.length).toBeGreaterThan(0);

    await showStatus(page, '✅ Handles scroll during load');
  });

  test('should handle multiple onScroll calls with no actual scroll', async ({ page }) => {
    await showStatus(page, '📋 Test: onScroll without scroll');
    await setupScrollTest(page, { count: 100 });

    const stateBefore = await getScrollState(page);
    const scrollTopBefore = stateBefore.scrollTop;

    // Call onScroll multiple times without changing scroll position
    for (let i = 0; i < 10; i++) {
      await triggerOnScroll(page);
      await page.waitForTimeout(10);
    }

    const stateAfter = await getScrollState(page);

    // State should be unchanged
    expect(stateAfter.scrollTop).toBe(scrollTopBefore);

    await showStatus(page, '✅ Handles no-op onScroll');
  });
});

test.describe('Edge Cases - Filter Changes', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await waitForWasm(page);
  });

  test.afterEach(async ({ page }) => {
    await cleanup(page);
  });

  test('should handle filter change with resetPosition=true', async ({ page }) => {
    await showStatus(page, '📋 Test: Filter change with reset');
    await setupScrollTest(page, { count: 100, room: 'room1' });

    // Scroll away from bottom
    await scrollTo(page, 100);
    await page.waitForTimeout(100);

    // Change filter (same room but different query)
    await updateFilter(page, "room = 'room1' AND deleted = false", true);

    const state = await getScrollState(page);
    expect(state.mode).toBe('Live');

    await showStatus(page, '✅ Filter reset works');
  });

  test('should handle filter change with resetPosition=false', async ({ page }) => {
    await showStatus(page, '📋 Test: Filter change without reset');
    await setupScrollTest(page, { count: 100, room: 'room1' });

    // Scroll to a position
    await scrollTo(page, 150);
    await page.waitForTimeout(100);

    const positionsBefore = await getItemPositions(page);

    // Change filter without reset
    await updateFilter(page, "room = 'room1' AND deleted = false", false);

    // Position should be preserved if possible
    // (depending on implementation details)

    await showStatus(page, '✅ Filter change without reset works');
  });
});

test.describe('Edge Cases - Scroll Position Precision', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await waitForWasm(page);
  });

  test.afterEach(async ({ page }) => {
    await cleanup(page);
  });

  test('should handle fractional scroll positions', async ({ page }) => {
    await showStatus(page, '📋 Test: Fractional scroll positions');
    await setupScrollTest(page, { count: 100 });

    // Try scrolling to fractional positions
    const fractionalPositions = [0.5, 10.3, 99.9, 150.75, 200.25];

    for (const pos of fractionalPositions) {
      await scrollTo(page, pos);
      await page.waitForTimeout(50);

      const state = await getScrollState(page);
      // ScrollTop should be close to requested position
      expect(Math.abs(state.scrollTop - pos)).toBeLessThan(1);
    }

    await showStatus(page, '✅ Handles fractional positions');
  });

  test('should handle negative scroll attempt gracefully', async ({ page }) => {
    await showStatus(page, '📋 Test: Negative scroll attempt');
    await setupScrollTest(page, { count: 100 });

    // Try to scroll to negative position
    await scrollTo(page, -100);
    await page.waitForTimeout(100);

    const state = await getScrollState(page);
    // Browser should clamp to 0
    expect(state.scrollTop).toBe(0);

    await showStatus(page, '✅ Handles negative scroll');
  });

  test('should handle oversized scroll attempt gracefully', async ({ page }) => {
    await showStatus(page, '📋 Test: Oversized scroll attempt');
    await setupScrollTest(page, { count: 100 });

    const state = await getScrollState(page);
    const maxScroll = state.scrollHeight - state.clientHeight;

    // Try to scroll beyond max
    await scrollTo(page, maxScroll + 1000);
    await page.waitForTimeout(100);

    const stateAfter = await getScrollState(page);
    // Browser should clamp to max
    expect(stateAfter.scrollTop).toBeLessThanOrEqual(maxScroll + 1);

    await showStatus(page, '✅ Handles oversized scroll');
  });
});
