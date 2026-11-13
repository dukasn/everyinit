import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./apps/everyinit/tests",
  outputDir: "./test-results/playwright",
  fullyParallel: false,
  retries: process.env.CI ? 2 : 0,
  reporter: process.env.CI
    ? [["line"], ["html", { outputFolder: "test-results/playwright-report", open: "never" }]]
    : "line",
  use: {
    baseURL: "http://127.0.0.1:8788",
    browserName: "chromium",
    colorScheme: "light",
    trace: "retain-on-failure",
  },
  webServer: {
    // The outer EVIN graph has already built the checked-out CLI. Starting
    // that binary directly avoids recursively invoking Cargo from Playwright.
    command:
      "npm exec --workspace @everyinit/site -- wrangler pages dev dist --local --ip 127.0.0.1 --port 8788",
    url: "http://127.0.0.1:8788",
    reuseExistingServer: !process.env.CI,
    timeout: 600_000,
  },
});
