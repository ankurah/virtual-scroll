import { test, expect } from '@playwright/test';
import {
  waitForWasm,
  setupScrollTest,
  getScrollState,
  getItems,
  cleanup,
  scrollToTop,
  scrollToBottom,
  scrollBy,
  triggerOnScroll,
  getCurrentSelection,
} from './helpers';

/**
 * Advanced ScrollManager tests - mirrors advanced_scroll_tests.rs
 *
 * Tests for:
 * - 1.5 Rapid scroll stress test
 * - 1.6 Intersection anchoring
 * - 1.7 Selection predicates
 * - 1.8 Live mode behavior
 * - 1.11 Concurrent operations
 */
test.describe('Advanced Scroll Tests', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
    await waitForWasm(page);
  });

  test.afterEach(async ({ page }) => {
    await cleanup(page);
  });

  // ============================================================================
  // 1.5 Rapid Scroll Stress Test
  // ============================================================================

  /**
   * Test rapid alternating scrolls without triggering pagination.
   * Verifies no panics or inconsistent state during rapid direction changes.
   * Mirrors: test_rapid_alternating_scrolls in Rust
   */
  test('test_rapid_alternating_scrolls', async ({ page }) => {
    await setupScrollTest(page, { count: 60, startTimestamp: 1000 });

    // Initial state: Live mode
    let state = await getScrollState(page);
    let items = await getItems(page);
    expect(state.mode).toBe('Live');
    const initialCount = items.length;

    // Rapid alternating scrolls - stay in buffer zone
    for (let i = 0; i < 10; i++) {
      await scrollBy(page, -50); // Scroll up a bit
      await page.waitForTimeout(20);
      await scrollBy(page, 50); // Scroll back down
      await page.waitForTimeout(20);
    }

    // Verify still in Live mode
    state = await getScrollState(page);
    items = await getItems(page);
    expect(state.mode).toBe('Live');

    // Verify items are still ordered correctly
    for (let i = 1; i < items.length; i++) {
      expect(items[i].timestamp).toBeGreaterThan(items[i - 1].timestamp);
    }
  });

  /**
   * Test multiple scroll events that trigger pagination.
   * Verifies correct state after rapid pagination triggers.
   * Mirrors: test_rapid_pagination_triggers in Rust
   */
  test('test_rapid_pagination_triggers', async ({ page }) => {
    await setupScrollTest(page, { count: 60, startTimestamp: 1000 });

    // Initial state
    let state = await getScrollState(page);
    expect(state.mode).toBe('Live');

    // Scroll up rapidly to trigger backward pagination
    await scrollToTop(page);
    await page.waitForTimeout(100);
    let direction = await triggerOnScroll(page);
    expect(direction).toBe('Backward');
    await page.waitForTimeout(500);

    state = await getScrollState(page);
    let items = await getItems(page);
    expect(state.mode).toBe('Backward');
    const afterFirstPagination = items[0].timestamp;

    // Continue scrolling up to trigger another pagination
    await scrollToTop(page);
    await page.waitForTimeout(100);
    direction = await triggerOnScroll(page);

    if (direction === 'Backward') {
      await page.waitForTimeout(500);
      state = await getScrollState(page);
      items = await getItems(page);

      // Should have loaded more older items
      expect(items[0].timestamp).toBeLessThanOrEqual(afterFirstPagination);
    }

    // Verify mode and item ordering
    expect(state.mode).toBe('Backward');
    for (let i = 1; i < items.length; i++) {
      expect(items[i].timestamp).toBeGreaterThan(items[i - 1].timestamp);
    }
  });

  // ============================================================================
  // 1.6 Intersection Anchoring Tests
  // ============================================================================

  /**
   * Test that intersection item exists in both old and new windows.
   * Backward pagination: intersection at newest visible (bottom of viewport).
   * Mirrors: test_intersection_anchoring_backward in Rust
   */
  test('test_intersection_anchoring_backward', async ({ page }) => {
    await setupScrollTest(page, { count: 60, startTimestamp: 1000 });

    let state = await getScrollState(page);
    expect(state.mode).toBe('Live');
    expect(state.intersection).toBeNull(); // No intersection initially

    // Scroll up to trigger backward pagination
    await scrollToTop(page);
    await page.waitForTimeout(100);
    const direction = await triggerOnScroll(page);
    expect(direction).toBe('Backward');
    await page.waitForTimeout(500);

    state = await getScrollState(page);
    const items = await getItems(page);

    // Should have intersection for scroll stability
    expect(state.intersection).not.toBeNull();
    expect(state.intersection?.entityId).toBeTruthy();
    expect(typeof state.intersection?.index).toBe('number');

    // Verify intersection item exists in the current window
    const intersectionIndex = state.intersection!.index;
    expect(intersectionIndex).toBeGreaterThanOrEqual(0);
    expect(intersectionIndex).toBeLessThan(items.length);
  });

  /**
   * Test forward pagination: intersection at oldest visible (top of viewport).
   * Mirrors: test_intersection_anchoring_forward in Rust
   */
  test('test_intersection_anchoring_forward', async ({ page }) => {
    await setupScrollTest(page, { count: 60, startTimestamp: 1000 });

    // First scroll backward to get away from live edge
    await scrollToTop(page);
    await page.waitForTimeout(100);
    await triggerOnScroll(page);
    await page.waitForTimeout(500);

    // Continue backward to build up some room
    let state = await getScrollState(page);
    let cycles = 0;
    while (state.hasMorePreceding && cycles < 5) {
      await scrollToTop(page);
      await page.waitForTimeout(100);
      const dir = await triggerOnScroll(page);
      if (dir !== 'Backward') break;
      await page.waitForTimeout(500);
      state = await getScrollState(page);
      cycles++;
    }

    // Now scroll forward to trigger forward pagination
    await scrollToBottom(page);
    await page.waitForTimeout(100);
    const direction = await triggerOnScroll(page);

    if (direction === 'Forward') {
      await page.waitForTimeout(500);
      state = await getScrollState(page);
      const items = await getItems(page);

      // Verify intersection for forward pagination
      expect(state.intersection).not.toBeNull();
      const intersectionIndex = state.intersection!.index;
      expect(intersectionIndex).toBeGreaterThanOrEqual(0);
      expect(intersectionIndex).toBeLessThan(items.length);
    }
  });

  // ============================================================================
  // 1.7 Selection Predicate Tests
  // ============================================================================

  /**
   * Test that selection predicates are correctly formed.
   * Mirrors: test_selection_predicates in Rust
   */
  test('test_selection_predicates', async ({ page }) => {
    await setupScrollTest(page, { count: 60, startTimestamp: 1000 });

    // Initial: Live mode selection
    let state = await getScrollState(page);
    expect(state.mode).toBe('Live');

    // Live mode: ORDER BY DESC LIMIT live_window
    let selection = await getCurrentSelection(page);
    expect(selection).toContain('ORDER BY timestamp DESC');
    expect(selection).toMatch(/LIMIT \d+/);

    // Trigger backward pagination
    await scrollToTop(page);
    await page.waitForTimeout(100);
    await triggerOnScroll(page);
    await page.waitForTimeout(500);

    state = await getScrollState(page);
    expect(state.mode).toBe('Backward');

    // Backward: timestamp <= cursor ORDER BY DESC
    selection = await getCurrentSelection(page);
    expect(selection).toMatch(/"timestamp" <= \d+/);
    expect(selection).toContain('ORDER BY timestamp DESC');
    expect(selection).toMatch(/LIMIT \d+/);
  });

  /**
   * Test forward selection predicate at oldest edge.
   * Mirrors: test_selection_predicate_forward in Rust
   */
  test('test_selection_predicate_forward', async ({ page }) => {
    await setupScrollTest(page, { count: 60, startTimestamp: 1000 });

    // Navigate to oldest edge
    let state = await getScrollState(page);
    let cycles = 0;
    while (state.hasMorePreceding && cycles < 10) {
      await scrollToTop(page);
      await page.waitForTimeout(100);
      const dir = await triggerOnScroll(page);
      if (dir !== 'Backward') break;
      await page.waitForTimeout(500);
      state = await getScrollState(page);
      cycles++;
    }

    expect(state.hasMorePreceding).toBe(false);

    // Scroll forward to trigger forward pagination
    cycles = 0;
    while (!state.hasMoreFollowing && cycles < 5) {
      await scrollToBottom(page);
      await page.waitForTimeout(100);
      await triggerOnScroll(page);
      await page.waitForTimeout(500);
      state = await getScrollState(page);
      cycles++;
      if (state.mode === 'Forward') break;
    }

    if (state.mode === 'Forward') {
      // Forward: timestamp >= cursor ORDER BY ASC
      const selection = await getCurrentSelection(page);
      expect(selection).toMatch(/"timestamp" >= \d+/);
      expect(selection).toContain('ORDER BY timestamp ASC');
    }
  });

  // ============================================================================
  // 1.8 Live Mode Behavior
  // ============================================================================

  /**
   * Test initial Live mode with should_auto_scroll.
   * Mirrors: test_live_mode_initial in Rust
   */
  test('test_live_mode_initial', async ({ page }) => {
    await setupScrollTest(page, { count: 60, startTimestamp: 1000 });

    // Initial render should be in Live mode with auto-scroll
    const state = await getScrollState(page);
    expect(state.shouldAutoScroll).toBe(true);
    expect(state.mode).toBe('Live');

    // Should be scrolled to bottom (near the bottom)
    const distanceFromBottom = state.scrollHeight - state.scrollTop - state.clientHeight;
    expect(distanceFromBottom).toBeLessThan(50);
  });

  /**
   * Test that scrolling up exits Live mode.
   * Mirrors: test_live_mode_exit_on_scroll_up in Rust
   */
  test('test_live_mode_exit_on_scroll_up', async ({ page }) => {
    await setupScrollTest(page, { count: 60, startTimestamp: 1000 });

    let state = await getScrollState(page);
    expect(state.mode).toBe('Live');

    // Scroll up to trigger backward pagination - exits Live mode
    await scrollToTop(page);
    await page.waitForTimeout(100);
    const direction = await triggerOnScroll(page);
    expect(direction).toBe('Backward');
    await page.waitForTimeout(500);

    // Should now be in Backward mode
    state = await getScrollState(page);
    expect(state.mode).toBe('Backward');
  });

  /**
   * Test returning to Live mode when scrolling back to bottom.
   * Mirrors: test_live_mode_reentry in Rust
   */
  test('test_live_mode_reentry', async ({ page }) => {
    await setupScrollTest(page, { count: 60, startTimestamp: 1000 });

    let state = await getScrollState(page);
    expect(state.mode).toBe('Live');

    // Full round trip: Live -> Backward -> oldest -> Forward -> Live
    // Scroll backward
    await scrollToTop(page);
    await page.waitForTimeout(100);
    await triggerOnScroll(page);
    await page.waitForTimeout(500);

    state = await getScrollState(page);
    expect(state.mode).toBe('Backward');

    // Continue to oldest edge
    let cycles = 0;
    while (state.hasMorePreceding && cycles < 10) {
      await scrollToTop(page);
      await page.waitForTimeout(100);
      const dir = await triggerOnScroll(page);
      if (dir !== 'Backward') break;
      await page.waitForTimeout(500);
      state = await getScrollState(page);
      cycles++;
    }

    // Scroll forward back to live
    cycles = 0;
    while (cycles < 15 && state.mode !== 'Live') {
      await scrollToBottom(page);
      await page.waitForTimeout(100);
      await triggerOnScroll(page);
      await page.waitForTimeout(500);
      state = await getScrollState(page);
      cycles++;
    }

    // Should be back in Live mode
    expect(state.mode).toBe('Live');
  });

  // ============================================================================
  // 1.11 Concurrent Operations
  // ============================================================================

  /**
   * Test that scroll events during pending pagination don't cause issues.
   * Note: Browser is single-threaded, so we test rapid sequential events.
   * Mirrors: test_concurrent_scroll_events in Rust
   */
  test('test_concurrent_scroll_events', async ({ page }) => {
    await setupScrollTest(page, { count: 60, startTimestamp: 1000 });

    const state = await getScrollState(page);
    expect(state.mode).toBe('Live');

    // Rapidly fire scroll events without waiting for renders
    // This simulates rapid user scrolling
    await scrollBy(page, -200);
    await triggerOnScroll(page);

    // Immediately scroll more without waiting
    await scrollBy(page, -200);
    await triggerOnScroll(page);

    // Wait for any pending renders
    await page.waitForTimeout(500);

    // Verify the result is valid regardless of which scroll "won"
    const items = await getItems(page);
    expect(items.length).toBeGreaterThan(0);

    // Verify items are sorted
    for (let i = 1; i < items.length; i++) {
      expect(items[i].timestamp).toBeGreaterThan(items[i - 1].timestamp);
    }
  });

  /**
   * Test multiple pagination triggers in sequence.
   * Mirrors: test_sequential_paginations in Rust
   */
  test('test_sequential_paginations', async ({ page }) => {
    await setupScrollTest(page, { count: 100, startTimestamp: 1000 });

    // Initial state
    let state = await getScrollState(page);
    let items = await getItems(page);
    const initialNewest = items[items.length - 1].timestamp;

    // Trigger multiple backward paginations in sequence
    // Each should correctly extend the window

    // First backward
    await scrollToTop(page);
    await page.waitForTimeout(100);
    await triggerOnScroll(page);
    await page.waitForTimeout(500);

    state = await getScrollState(page);
    items = await getItems(page);
    expect(state.mode).toBe('Backward');
    const afterFirst = items[0].timestamp;

    // Second backward
    await scrollToTop(page);
    await page.waitForTimeout(100);
    const dir = await triggerOnScroll(page);

    if (dir === 'Backward') {
      await page.waitForTimeout(500);
      state = await getScrollState(page);
      items = await getItems(page);

      // Should have loaded more older items
      expect(items[0].timestamp).toBeLessThanOrEqual(afterFirst);
    }

    // Verify final state
    expect(state.mode).toBe('Backward');
    // Items should still be sorted
    for (let i = 1; i < items.length; i++) {
      expect(items[i].timestamp).toBeGreaterThan(items[i - 1].timestamp);
    }
  });
});
