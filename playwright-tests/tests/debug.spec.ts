import { test, expect } from '@playwright/test';

test.describe('Debug', () => {
  test('should check WASM loading', async ({ page }) => {
    const consoleMessages: string[] = [];
    page.on('console', (msg) => {
      consoleMessages.push(`[${msg.type()}] ${msg.text()}`);
    });

    page.on('pageerror', (error) => {
      consoleMessages.push(`[ERROR] ${error.message}`);
    });

    await page.goto('/');

    // Wait a bit for WASM to load
    await page.waitForTimeout(5000);

    // Check if WASM loaded
    const wasmLoaded = await page.evaluate(() => window.wasm !== null);
    console.log('WASM loaded:', wasmLoaded);
    console.log('Console messages:', consoleMessages);

    // Check testHelpers
    const testHelpersAvailable = await page.evaluate(
      () => window.testHelpers !== null
    );
    console.log('testHelpers available:', testHelpersAvailable);

    // Get page content
    const bodyText = await page.textContent('body');
    console.log('Page body text:', bodyText?.substring(0, 500));

    expect(wasmLoaded).toBe(true);
    expect(testHelpersAvailable).toBe(true);
  });
});
