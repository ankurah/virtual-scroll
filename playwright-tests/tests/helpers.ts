import { Page, expect } from '@playwright/test';

// Default delay for visual debugging (set to 0 for fast tests)
const TEST_DELAY = process.env.FAST_TESTS ? 0 : 500;

/**
 * Display a status message and optionally wait
 */
export async function showStatus(page: Page, message: string, delay = TEST_DELAY): Promise<void> {
  await page.evaluate((msg) => window.testHelpers?.setTestStatus(msg), message);
  if (delay > 0) {
    await page.waitForTimeout(delay);
  }
}

/**
 * Clear the status message
 */
export async function clearStatus(page: Page): Promise<void> {
  await page.evaluate(() => window.testHelpers?.clearTestStatus());
}

/**
 * Wait for WASM to be loaded and ready
 */
export async function waitForWasm(page: Page): Promise<void> {
  // Wait for WASM module to be available (takes ~5s on first load)
  await page.waitForFunction(
    () => window.wasm !== null,
    { timeout: 60000, polling: 500 }
  );

  // Wait for testHelpers to be set (component mounted)
  await page.waitForFunction(
    () => window.testHelpers != null && typeof window.testHelpers?.getItemCount === 'function',
    { timeout: 30000, polling: 250 }
  );

  // Give a bit more time for React state to stabilize
  await page.waitForTimeout(200);
}

/**
 * Get the test helpers from the page
 */
export async function getTestHelpers(page: Page) {
  return page.evaluate(() => window.testHelpers!);
}

/**
 * Seed test data and create a scroll manager
 */
export async function setupScrollTest(
  page: Page,
  options: {
    room?: string;
    count?: number;
    startTimestamp?: number;
    variedHeights?: boolean;
    viewportHeight?: number;
  } = {}
): Promise<void> {
  const {
    room = 'room1',
    count = 100,
    startTimestamp = 1000,
    variedHeights = false,
    viewportHeight = 400,
  } = options;

  await page.evaluate(
    async ({ room, count, startTimestamp, variedHeights, viewportHeight }) => {
      const helpers = window.testHelpers!;
      await helpers.clearAllMessages();
      await helpers.seedTestData(room, count, startTimestamp, variedHeights);
      await helpers.createScrollManager(room, viewportHeight);
    },
    { room, count, startTimestamp, variedHeights, viewportHeight }
  );

  // Wait for items to load
  await page.waitForFunction(
    () => window.testHelpers?.getItemCount() > 0,
    { timeout: 5000 }
  );
}

/**
 * Get current scroll state
 */
export async function getScrollState(page: Page) {
  return page.evaluate(() => {
    const helpers = window.testHelpers!;
    return {
      scrollTop: helpers.getScrollTop(),
      scrollHeight: helpers.getScrollHeight(),
      clientHeight: helpers.getClientHeight(),
      itemCount: helpers.getItemCount(),
      mode: helpers.getMode(),
      hasMorePreceding: helpers.hasMorePreceding(),
      hasMoreFollowing: helpers.hasMoreFollowing(),
      // Legacy names for backwards compatibility
      hasMoreOlder: helpers.hasMoreOlder(),
      hasMoreNewer: helpers.hasMoreNewer(),
      shouldAutoScroll: helpers.shouldAutoScroll(),
      isLoading: helpers.isLoading(),
      intersection: helpers.getIntersection(),
    };
  });
}

/**
 * Get all item positions
 */
export async function getItemPositions(page: Page) {
  return page.evaluate(() => window.testHelpers!.getItemPositions());
}

/**
 * Get items array
 */
export async function getItems(page: Page) {
  return page.evaluate(() => window.testHelpers!.getItems());
}

/**
 * Scroll to a specific position
 */
export async function scrollTo(page: Page, scrollTop: number): Promise<void> {
  await page.evaluate((scrollTop) => {
    window.testHelpers!.setScrollTop(scrollTop);
  }, scrollTop);
  // Give React time to process the scroll event
  await page.waitForTimeout(50);
}

/**
 * Scroll by a delta amount
 */
export async function scrollBy(page: Page, delta: number): Promise<void> {
  await page.evaluate((delta) => {
    window.testHelpers!.scrollBy(delta);
  }, delta);
  // Wait for scroll and React to settle
  await page.waitForTimeout(100);
}

/**
 * Scroll to top
 */
export async function scrollToTop(page: Page): Promise<void> {
  // Use testHelpers.scrollToTop which disables auto-scroll
  await page.evaluate(() => {
    window.testHelpers!.scrollToTop();
  });
  // Wait for scroll and React to settle
  await page.waitForTimeout(100);
}

/**
 * Scroll to bottom
 */
export async function scrollToBottom(page: Page): Promise<void> {
  await page.evaluate(() => {
    window.testHelpers!.scrollToBottom();
  });
  await page.waitForTimeout(50);
}

/**
 * Trigger onScroll manually and return the load direction
 */
export async function triggerOnScroll(page: Page): Promise<string | null> {
  return page.evaluate(() => window.testHelpers!.triggerOnScroll());
}

/**
 * Wait for loading to complete
 */
export async function waitForLoading(page: Page): Promise<void> {
  await page.waitForFunction(() => !window.testHelpers!.isLoading(), {
    timeout: 5000,
  });
}

/**
 * Wait for item count to change
 */
export async function waitForItemCountChange(
  page: Page,
  currentCount: number
): Promise<void> {
  await page.waitForFunction(
    (count) => window.testHelpers!.getItemCount() !== count,
    currentCount,
    { timeout: 5000 }
  );
}

/**
 * Get position of a specific item by ID
 */
export async function getItemPosition(page: Page, id: string) {
  return page.evaluate((id) => window.testHelpers!.getItemById(id), id);
}

/**
 * Jump to live mode
 */
export async function jumpToLive(page: Page): Promise<void> {
  await page.evaluate(() => window.testHelpers!.jumpToLive());
  await page.waitForTimeout(100);
}

/**
 * Update filter predicate
 */
export async function updateFilter(
  page: Page,
  predicate: string,
  resetPosition: boolean
): Promise<void> {
  await page.evaluate(
    ({ predicate, resetPosition }) =>
      window.testHelpers!.updateFilter(predicate, resetPosition),
    { predicate, resetPosition }
  );
  await page.waitForTimeout(100);
}

/**
 * Clean up after test
 */
export async function cleanup(page: Page): Promise<void> {
  await page.evaluate(async () => {
    window.testHelpers?.destroyScrollManager();
    await window.testHelpers?.clearAllMessages();
  });
}

/**
 * Get the current selection (predicate + order by) as a string
 */
export async function getCurrentSelection(page: Page): Promise<string> {
  return page.evaluate(() => window.testHelpers!.getCurrentSelection());
}

/**
 * Assert scroll stability: check that an item's visual position hasn't changed
 */
export async function assertScrollStability(
  page: Page,
  itemId: string,
  expectedTop: number,
  tolerance: number = 1
): Promise<void> {
  const position = await getItemPosition(page, itemId);
  expect(position).not.toBeNull();
  if (position) {
    expect(Math.abs(position.top - expectedTop)).toBeLessThanOrEqual(tolerance);
  }
}
