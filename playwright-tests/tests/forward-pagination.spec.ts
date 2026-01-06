import { test, expect } from '@playwright/test';
import {
  waitForWasm,
  setupScrollTest,
  getScrollState,
  getItems,
  cleanup,
  scrollTo,
  scrollToTop,
  scrollToBottom,
  triggerOnScroll,
  waitForLoading,
  waitForItemCountChange,
  jumpToLive,
} from './helpers';

test.describe('Forward Pagination', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await waitForWasm(page);
  });

  test.afterEach(async ({ page }) => {
    await cleanup(page);
  });

  test('should trigger forward pagination when scrolling near bottom in backward mode', async ({
    page,
  }) => {
    await setupScrollTest(page, { count: 200 });

    const initialState = await getScrollState(page);

    // First, trigger backward pagination to get into backward mode
    // The scrollToTop will trigger the DOM scroll event which calls handleScroll
    // and that might trigger backward pagination. Wait for that to complete.
    await scrollToTop(page);
    await page.waitForTimeout(300); // Wait for scroll event to process

    // Now check if backward pagination was triggered by the scroll event
    let state = await getScrollState(page);

    // If we're still in Live mode, manually trigger backward pagination
    if (state.mode === 'Live' && state.hasMoreOlder) {
      await triggerOnScroll(page);
      await page.waitForTimeout(300);
    }

    if (initialState.hasMoreOlder) {
      await waitForLoading(page);

      const backwardState = await getScrollState(page);
      expect(backwardState.mode).toBe('Backward');
      expect(backwardState.hasMoreNewer).toBe(true);

      // Now scroll to bottom to trigger forward pagination
      // Note: scrollToBottom triggers the DOM scroll event, which may already trigger forward pagination
      await scrollToBottom(page);
      await page.waitForTimeout(300); // Wait for scroll event to process

      const stateAtBottom = await getScrollState(page);

      // The scroll event may have already triggered forward pagination
      // Check if mode changed to Forward (or Live if we reached latest)
      if (backwardState.hasMoreNewer) {
        // Either scrollToBottom already triggered forward pagination via DOM event,
        // or we need to trigger it manually
        if (stateAtBottom.mode === 'Backward') {
          const direction = await triggerOnScroll(page);
          expect(direction).toBe('Forward');
          await page.waitForTimeout(300);
        }

        const forwardState = await getScrollState(page);
        // Mode should be either Forward or Live (if we returned to live edge)
        expect(['Forward', 'Live']).toContain(forwardState.mode);
      }
    }
  });

  test('should return to live mode when reaching latest items', async ({
    page,
  }) => {
    await setupScrollTest(page, { count: 50 }); // Fewer items for faster test

    const initialState = await getScrollState(page);

    // Get into backward mode
    await scrollToTop(page);
    await triggerOnScroll(page);

    if (initialState.hasMoreOlder) {
      await waitForLoading(page);

      // Jump to live should return to live mode
      await jumpToLive(page);
      await waitForLoading(page);

      const state = await getScrollState(page);
      expect(state.mode).toBe('Live');
      expect(state.hasMoreNewer).toBe(false);
      expect(state.shouldAutoScroll).toBe(true);
    }
  });

  test('jumpToLive should work from any scroll position', async ({ page }) => {
    await setupScrollTest(page, { count: 200 });

    const initialState = await getScrollState(page);

    // Get into backward mode by scrolling up
    await scrollToTop(page);
    await triggerOnScroll(page);

    if (initialState.hasMoreOlder) {
      await waitForLoading(page);

      const backwardState = await getScrollState(page);
      expect(backwardState.mode).toBe('Backward');

      // Jump to live
      await jumpToLive(page);
      await waitForLoading(page);

      const liveState = await getScrollState(page);
      expect(liveState.mode).toBe('Live');
      expect(liveState.shouldAutoScroll).toBe(true);
    }
  });

  test('should set hasMoreNewer to false when in live mode', async ({
    page,
  }) => {
    await setupScrollTest(page, { count: 50 });

    const state = await getScrollState(page);
    expect(state.mode).toBe('Live');
    expect(state.hasMoreNewer).toBe(false);
  });

  test('should set hasMoreNewer to true when in backward mode with newer items', async ({
    page,
  }) => {
    await setupScrollTest(page, { count: 200 });

    const initialState = await getScrollState(page);

    // Get into backward mode
    await scrollToTop(page);
    await triggerOnScroll(page);

    if (initialState.hasMoreOlder) {
      await waitForLoading(page);

      const state = await getScrollState(page);
      expect(state.mode).toBe('Backward');
      expect(state.hasMoreNewer).toBe(true);
    }
  });

  test('forward pagination should load newer items at bottom', async ({
    page,
  }) => {
    await setupScrollTest(page, { count: 200, startTimestamp: 1000 });

    const initialState = await getScrollState(page);

    // Get into backward mode
    await scrollToTop(page);
    await triggerOnScroll(page);

    if (initialState.hasMoreOlder) {
      await waitForLoading(page);

      const backwardItems = await getItems(page);
      const newestInBackward = Math.max(...backwardItems.map((i) => i.timestamp));

      // Now scroll to bottom for forward pagination
      await scrollToBottom(page);
      const direction = await triggerOnScroll(page);

      const backwardState = await getScrollState(page);
      if (backwardState.hasMoreNewer) {
        expect(direction).toBe('Forward');
        await waitForItemCountChange(page, backwardItems.length);

        const forwardItems = await getItems(page);
        const newestInForward = Math.max(...forwardItems.map((i) => i.timestamp));

        // Should have loaded newer items
        expect(newestInForward).toBeGreaterThan(newestInBackward);
      }
    }
  });

  test('should disable auto-scroll when not at live', async ({ page }) => {
    await setupScrollTest(page, { count: 200 });

    const initialState = await getScrollState(page);
    expect(initialState.shouldAutoScroll).toBe(true); // Initially at live

    // Get into backward mode
    await scrollToTop(page);
    await triggerOnScroll(page);

    if (initialState.hasMoreOlder) {
      await waitForLoading(page);

      const state = await getScrollState(page);
      expect(state.shouldAutoScroll).toBe(false); // Not at live anymore
    }
  });

  test('should re-enable auto-scroll when returning to live', async ({
    page,
  }) => {
    await setupScrollTest(page, { count: 100 });

    const initialState = await getScrollState(page);

    // Get into backward mode
    await scrollToTop(page);
    await triggerOnScroll(page);

    if (initialState.hasMoreOlder) {
      await waitForLoading(page);

      const backwardState = await getScrollState(page);
      expect(backwardState.shouldAutoScroll).toBe(false);

      // Jump to live
      await jumpToLive(page);
      await waitForLoading(page);

      const liveState = await getScrollState(page);
      expect(liveState.shouldAutoScroll).toBe(true);
    }
  });
});
