import { test, expect } from '@playwright/test';

test.describe('WASM Debug', () => {
  test('should test basic WASM function calls', async ({ page }) => {
    const consoleMessages: string[] = [];
    page.on('console', (msg) => {
      consoleMessages.push(`[${msg.type()}] ${msg.text()}`);
    });

    page.on('pageerror', (error) => {
      consoleMessages.push(`[PAGE ERROR] ${error.message}`);
    });

    await page.goto('/');

    // Wait a fixed time and see what we get
    await page.waitForTimeout(10000);

    // Get page content
    const pageText = await page.textContent('body');
    console.log('Page text after 10s:', pageText);

    // Check wasm state
    const state = await page.evaluate(() => ({
      hasWasm: window.wasm !== null && window.wasm !== undefined,
      wasmType: typeof window.wasm,
      hasHelpers: window.testHelpers !== null && window.testHelpers !== undefined,
    }));
    console.log('State after 10s:', state);

    // Print all console messages
    console.log('Browser console messages:', consoleMessages);

    // Just check if page loaded at all
    expect(pageText).toBeTruthy();
  });
});
