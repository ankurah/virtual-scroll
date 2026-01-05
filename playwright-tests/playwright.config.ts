import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: './tests',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 1 : undefined,
  reporter: 'html',

  use: {
    baseURL: 'http://localhost:5199',
    trace: 'on-first-retry',
    // Fixed viewport for deterministic results
    viewport: { width: 800, height: 600 },
  },

  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'], viewport: { width: 800, height: 600 } },
    },
  ],

  webServer: {
    command: 'cd react-app && npm run dev',
    url: 'http://localhost:5199',
    reuseExistingServer: !process.env.CI,
    timeout: 120000,
  },
});
