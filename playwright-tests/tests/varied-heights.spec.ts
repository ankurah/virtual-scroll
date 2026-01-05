import { test, expect } from '@playwright/test';
import {
  waitForWasm,
  setupScrollTest,
  getScrollState,
  getItems,
  getItemPositions,
  getItemPosition,
  cleanup,
  scrollTo,
  scrollToTop,
  triggerOnScroll,
  waitForLoading,
  waitForItemCountChange,
} from './helpers';

test.describe('Varied Item Heights', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await waitForWasm(page);
  });

  test.afterEach(async ({ page }) => {
    await cleanup(page);
  });

  test('should render items with different heights', async ({ page }) => {
    await setupScrollTest(page, { count: 50, variedHeights: true });

    const positions = await getItemPositions(page);
    expect(positions.length).toBeGreaterThan(0);

    // Get unique heights
    const heights = new Set(positions.map((p) => p.height));

    // With varied heights, we should have more than one height value
    expect(heights.size).toBeGreaterThan(1);
  });

  test('should maintain scroll stability with varied heights during backward pagination', async ({
    page,
  }) => {
    await setupScrollTest(page, {
      count: 200,
      variedHeights: true,
      viewportHeight: 400,
    });

    const initialItems = await getItems(page);
    const initialPositions = await getItemPositions(page);
    const initialState = await getScrollState(page);

    // Find an item in the middle of the viewport
    const viewportCenter = initialState.scrollTop + initialState.clientHeight / 2;
    const anchorItem = initialPositions.find(
      (p) => p.top <= viewportCenter && p.top + p.height >= viewportCenter
    );

    if (!anchorItem) {
      // Use first visible item as fallback
      const firstVisible = initialPositions[0];
      if (!firstVisible) {
        test.skip();
        return;
      }
    }

    const anchorId = anchorItem?.id || initialPositions[0].id;
    const anchorPositionBefore = await getItemPosition(page, anchorId);

    // Scroll to top and trigger backward pagination
    await scrollToTop(page);
    await triggerOnScroll(page);

    if (initialState.hasMoreOlder) {
      await waitForItemCountChange(page, initialItems.length);

      // The anchor item should still exist and its relative position should be preserved
      const anchorPositionAfter = await getItemPosition(page, anchorId);

      if (anchorPositionAfter && anchorPositionBefore) {
        // With varied heights, scroll stability is even more important
        // The visual position delta should be minimal
        const delta = Math.abs(anchorPositionAfter.top - anchorPositionBefore.top);
        expect(delta).toBeLessThan(100); // Slightly more tolerance for varied heights
      }
    }
  });

  test('should calculate scroll position correctly with varied heights', async ({
    page,
  }) => {
    await setupScrollTest(page, { count: 100, variedHeights: true });

    const state = await getScrollState(page);
    const positions = await getItemPositions(page);

    // Verify positions are consecutive (no gaps)
    let lastBottom = 0;
    for (const pos of positions.sort((a, b) => a.top - b.top)) {
      // Each item should start where the previous ended (with some tolerance for borders)
      expect(Math.abs(pos.top - lastBottom)).toBeLessThan(5);
      lastBottom = pos.top + pos.height;
    }
  });

  test('should handle very tall items correctly', async ({ page }) => {
    // The varied heights include "very long" messages
    await setupScrollTest(page, { count: 50, variedHeights: true });

    const positions = await getItemPositions(page);

    // Find the tallest item
    const maxHeight = Math.max(...positions.map((p) => p.height));

    // Very long messages should be noticeably taller
    expect(maxHeight).toBeGreaterThan(50);
  });

  test('should handle short items correctly', async ({ page }) => {
    await setupScrollTest(page, { count: 50, variedHeights: true });

    const positions = await getItemPositions(page);

    // Find the shortest item
    const minHeight = Math.min(...positions.map((p) => p.height));

    // Short messages should still have reasonable height
    expect(minHeight).toBeGreaterThan(20);
    expect(minHeight).toBeLessThan(80);
  });

  test('items should be in correct order regardless of height', async ({
    page,
  }) => {
    await setupScrollTest(page, {
      count: 50,
      variedHeights: true,
      startTimestamp: 1000,
    });

    const items = await getItems(page);
    const positions = await getItemPositions(page);

    // Create a map of id to position
    const positionMap = new Map(positions.map((p) => [p.id, p.top]));

    // Verify items are in timestamp order
    for (let i = 1; i < items.length; i++) {
      expect(items[i].timestamp).toBeGreaterThan(items[i - 1].timestamp);

      // And their visual positions match (earlier timestamp = higher position)
      const prevTop = positionMap.get(items[i - 1].id);
      const currTop = positionMap.get(items[i].id);
      if (prevTop !== undefined && currTop !== undefined) {
        expect(currTop).toBeGreaterThan(prevTop);
      }
    }
  });

  test('pagination trigger should work correctly with varied heights', async ({
    page,
  }) => {
    await setupScrollTest(page, { count: 200, variedHeights: true });

    const initialState = await getScrollState(page);

    // Even with varied heights, scrolling to top should trigger backward pagination
    await scrollToTop(page);
    const direction = await triggerOnScroll(page);

    if (initialState.hasMoreOlder) {
      expect(direction).toBe('Backward');
    }
  });

  test('intersection point should account for varied heights', async ({
    page,
  }) => {
    await setupScrollTest(page, {
      count: 100,
      variedHeights: true,
      viewportHeight: 400,
    });

    // Scroll to a position where we can see the intersection
    const state = await getScrollState(page);

    if (state.intersection) {
      const positions = await getItemPositions(page);
      const intersectionItem = positions.find(
        (p) => p.id === state.intersection!.entityId
      );

      if (intersectionItem) {
        // The intersection item should be within the viewport
        const viewportTop = state.scrollTop;
        const viewportBottom = state.scrollTop + state.clientHeight;

        expect(intersectionItem.top).toBeGreaterThanOrEqual(viewportTop - 100);
        expect(intersectionItem.top).toBeLessThanOrEqual(viewportBottom + 100);
      }
    }
  });
});
