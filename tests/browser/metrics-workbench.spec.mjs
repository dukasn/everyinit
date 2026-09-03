import { expect, test } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

test.describe('Declared metrics page and API', () => {
  test('GET /__evin_metrics returns valid live JSON snapshot', async ({ request }) => {
    const response = await request.get('/__evin_metrics');
    expect(response.status()).toBe(200);
    expect(response.headers()['content-type']).toContain('application/json');

    const data = await response.json();
    expect(data.budget_status).toBeDefined();
    expect(['pass', 'fail', 'not_configured']).toContain(data.budget_status);
    expect(data.frame0).toBeDefined();
    expect(data.frame0.raw_bytes).toBeGreaterThan(0);
    expect(data.frame0.gzip_bytes).toBeGreaterThan(0);
    expect(data.frame0.files.length).toBeGreaterThan(0);
    expect(data.frame0.files).toContain('index.html');
    expect(data.totals.file_count).toBeGreaterThan(0);
    expect(data.tree_shaking.icons.total_symbols).toBeGreaterThan(0);
    expect(data.assets.length).toBeGreaterThan(0);
  });

  test('GET /metrics renders the declared layout-native metrics page with live interactions', async ({ page }) => {
    await page.goto('/metrics');

    await expect(page).toHaveTitle(/Metrics/);
    await expect(page.locator('h1')).toContainText('KB gzip');

    // Wait for the metrics to fetch and populate
    await expect(page.locator('[data-metrics-component="manifest"]')).not.toContainText('Loading');

    // Verify live hero and budget telemetry
    await expect(page.locator('[data-metrics-component="budget"]')).toBeVisible();
    await expect(page.locator('[data-metrics-component="hero"] [data-metric-field="asset-count"]')).toHaveText(/\d+/);

    // Verify table population
    const rows = page.locator('[data-metrics-component="inventory"] tbody tr');
    await expect(rows.first()).toBeVisible();
    const initialCount = await rows.count();
    expect(initialCount).toBeGreaterThan(1);

    // Test Filter by Frame-0
    await page.locator('.metrics-chip[data-metrics-filter="frame0"]').click();
    await expect(page.locator('.metrics-chip[data-metrics-filter="frame0"]')).toHaveClass(/active/);
    const frame0Rows = await page.locator('[data-metrics-component="inventory"] tbody tr').count();
    expect(frame0Rows).toBeLessThan(initialCount);
    expect(frame0Rows).toBeGreaterThan(0);

    // Test Refresh Button
    const refreshBtn = page.locator('[data-metrics-action="refresh"]');
    await refreshBtn.click();
    await expect(page.locator('[data-metrics-component="manifest"]')).not.toContainText('Loading');

    // Test Theme Toggle
    const themeBtn = page.locator('#themeToggle');
    await expect(themeBtn).toBeVisible();
    const initialTheme = await page.evaluate(() => document.documentElement.getAttribute('data-theme'));
    await themeBtn.click();
    const toggledTheme = await page.evaluate(() => document.documentElement.getAttribute('data-theme'));
    expect(toggledTheme).not.toBe(initialTheme);

    // Switch back to All filter
    await page.locator('.metrics-chip[data-metrics-filter="all"]').click();
    await expect(page.locator('.metrics-chip[data-metrics-filter="all"]')).toHaveClass(/active/);

    // Verify Page Scrollability
    const scrollInfo = await page.locator('main.content').evaluate((container) => {
      const scrollHeight = container.scrollHeight;
      const clientHeight = container.clientHeight;
      return {
        scrollHeight,
        clientHeight,
        canScroll: scrollHeight > clientHeight,
      };
    });
    expect(scrollInfo.canScroll).toBe(true);

    // Perform scroll and verify scroll position updates
    await page.locator('main.content').evaluate((container) => container.scrollTo(0, 300));
    const scrolledTop = await page.locator('main.content').evaluate((container) => container.scrollTop);
    expect(scrolledTop).toBeGreaterThan(0);

    // Test Navigation back to documentation
    await page.locator('a.header-brand').evaluate((link) => link.click());
    await expect(page).toHaveURL(/127\.0\.0\.1:3000\/(?:index\.html)?$/);
  });

  test('Static build artifact does not contain metrics markup or route shims', () => {
    execFileSync('cargo', ['evin-dev', 'run', 'docs:build'], { stdio: 'inherit' });
    const distDir = path.resolve('dist/docs');
    expect(fs.existsSync(distDir)).toBe(true);

    const indexHtml = fs.readFileSync(path.join(distDir, 'index.html'), 'utf-8');
    expect(indexHtml).not.toContain('data-metrics-page');
    expect(indexHtml).not.toContain('/__evin_metrics');
    expect(indexHtml).not.toContain('/metrics');

    // Ensure /metrics is not emitted as a static directory or file
    expect(fs.existsSync(path.join(distDir, 'metrics'))).toBe(false);
    expect(fs.existsSync(path.join(distDir, 'metrics.html'))).toBe(false);
    expect(fs.existsSync(path.join(distDir, '__evin_metrics'))).toBe(false);
  });
});
