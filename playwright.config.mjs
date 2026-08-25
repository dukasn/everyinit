import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './tests/browser',
  outputDir: './test-results/playwright',
  fullyParallel: false,
  retries: process.env.CI ? 2 : 0,
  reporter: process.env.CI ? [['line'], ['html', { outputFolder: 'test-results/playwright-report', open: 'never' }]] : 'line',
  use: {
    baseURL: 'http://127.0.0.1:3000',
    browserName: 'chromium',
    channel: 'chromium',
    colorScheme: 'light',
    trace: 'retain-on-failure',
  },
  webServer: {
    command: 'cargo evin-dev run docs:serve',
    url: 'http://127.0.0.1:3000',
    reuseExistingServer: !process.env.CI,
    timeout: 600_000,
  },
});
