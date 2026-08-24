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
    command: 'cargo evin-dev run docs:build && python3 -m http.server 3000 --bind 127.0.0.1 --directory dist/docs',
    url: 'http://127.0.0.1:3000',
    reuseExistingServer: !process.env.CI,
    timeout: 120_000,
  },
});
