import { expect, test } from '@playwright/test';

const viewports = [
  { name: 'compact', width: 390, height: 844, windowClass: 'compact', outline: 'inline' },
  { name: 'medium', width: 700, height: 900, windowClass: 'medium', outline: 'inline' },
  { name: 'expanded', width: 1024, height: 900, windowClass: 'expanded', outline: 'inline' },
  { name: 'large', width: 1366, height: 900, windowClass: 'large', outline: 'rail' },
  { name: 'extra-large', width: 1728, height: 1100, windowClass: 'extra-large', outline: 'rail' },
  { name: 'compact-height', width: 844, height: 430, windowClass: 'expanded', outline: 'inline' },
];

async function isVisible(locator) {
  return locator.evaluate((element) => getComputedStyle(element).display !== 'none');
}

for (const viewport of viewports) {
  test(`${viewport.name} scaffold is predictable`, async ({ page }, testInfo) => {
    await page.setViewportSize({ width: viewport.width, height: viewport.height });
    await page.goto('/reference/actions/documentation-action/');

    await expect(page.locator('html')).toHaveAttribute('data-window-size', viewport.windowClass);
    await expect(page.locator('html')).not.toHaveAttribute('data-window-height');
    await expect(page.locator('.header-toolbar')).toBeVisible();

    const modalDrawer = viewport.windowClass === 'compact' || viewport.windowClass === 'medium';
    await expect(page.locator('#sidebarToggle')).toHaveAttribute('aria-expanded', String(!modalDrawer));
    await expect(page.locator('.inline-toc')).toHaveCount(1);
    await expect(page.locator('.toc-rail')).toHaveCount(1);
    expect(await isVisible(page.locator('.inline-toc'))).toBe(viewport.outline === 'inline');
    expect(await isVisible(page.locator('.toc-rail'))).toBe(viewport.outline === 'rail');
    if (viewport.outline === 'rail') await expect(page.locator('.toc-rail')).toBeVisible();

    const searchLabelVisible = viewport.windowClass !== 'compact';
    const searchHintVisible = viewport.windowClass === 'large' || viewport.windowClass === 'extra-large';
    expect(await isVisible(page.locator('.search-trigger-label'))).toBe(searchLabelVisible);
    expect(await isVisible(page.locator('.search-trigger .search-kbd'))).toBe(searchHintVisible);

    const dimensions = await page.evaluate(() => ({
      scrollWidth: document.documentElement.scrollWidth,
      clientWidth: document.documentElement.clientWidth,
      toggleWidth: document.querySelector('#sidebarToggle').getBoundingClientRect().width,
      toggleHeight: document.querySelector('#sidebarToggle').getBoundingClientRect().height,
    }));
    expect(dimensions.scrollWidth).toBeLessThanOrEqual(dimensions.clientWidth);
    expect(dimensions.toggleWidth).toBeGreaterThanOrEqual(48);
    expect(dimensions.toggleHeight).toBeGreaterThanOrEqual(48);

    await page.locator('main.content').evaluate((element) => { element.scrollTop = 320; });
    await expect(page.locator('.header-toolbar')).toHaveClass(/is-scrolled/);
    await expect(page.locator('.header-toolbar')).toHaveClass(/is-hidden/);
    await page.locator('main.content').evaluate((element) => { element.scrollTop = 180; });
    await expect(page.locator('.header-toolbar')).not.toHaveClass(/is-hidden/);

    for (const theme of ['light', 'dark']) {
      await page.evaluate((nextTheme) => document.documentElement.setAttribute('data-theme', nextTheme), theme);
      await page.waitForTimeout(400);
      await page.screenshot({
        path: testInfo.outputPath(`${viewport.name}-${theme}.png`),
        fullPage: false,
      });
    }
  });
}

test('modal drawer traps focus and supports every dismissal path', async ({ page }) => {
  await page.setViewportSize({ width: 700, height: 900 });
  await page.goto('/reference/actions/documentation-action/');

  const toggle = page.locator('#sidebarToggle');
  const sidebar = page.locator('#sidebar');
  await toggle.click();
  await expect(toggle).toHaveAttribute('aria-expanded', 'true');
  await expect(sidebar).toHaveAttribute('aria-hidden', 'false');
  await expect(page.locator('body')).toHaveClass(/modal-drawer-open/);
  await expect.poll(() => page.evaluate(() => (
    document.querySelector('#sidebar').contains(document.activeElement)
  ))).toBe(true);

  await page.keyboard.press('Escape');
  await expect(toggle).toHaveAttribute('aria-expanded', 'false');
  await expect(toggle).toBeFocused();

  await toggle.click();
  await page.locator('#sidebarBackdrop').click({ position: { x: 650, y: 450 } });
  await expect(toggle).toHaveAttribute('aria-expanded', 'false');

  await toggle.click();
  const visibleLink = sidebar.locator('a.nav-item:visible').first();
  await expect(visibleLink).toBeVisible();
  const previousUrl = page.url();
  await visibleLink.click();
  await expect.poll(() => page.url()).not.toBe(previousUrl);
});

test('persistent drawer preference survives modal window classes', async ({ page }) => {
  await page.setViewportSize({ width: 1024, height: 900 });
  await page.goto('/reference/actions/documentation-action/');
  const toggle = page.locator('#sidebarToggle');

  await expect(toggle).toHaveAttribute('aria-expanded', 'true');
  await toggle.click();
  await expect(toggle).toHaveAttribute('aria-expanded', 'false');

  await page.setViewportSize({ width: 700, height: 900 });
  await expect(page.locator('html')).toHaveAttribute('data-window-size', 'medium');
  await expect(toggle).toHaveAttribute('aria-expanded', 'false');

  await page.setViewportSize({ width: 1024, height: 900 });
  await expect(page.locator('html')).toHaveAttribute('data-window-size', 'expanded');
  await expect(toggle).toHaveAttribute('aria-expanded', 'false');
});

test('home page uses the same document navigation and leaves it open on desktop', async ({ page }) => {
  await page.setViewportSize({ width: 1024, height: 900 });
  await page.goto('/');

  await expect(page.locator('#sidebarToggle')).toHaveAttribute('aria-expanded', 'true');
  await expect(page.locator('#appLayout')).not.toHaveClass(/sidebar-closed/);
  await expect(page.locator('#navFilter')).toBeVisible();
  await expect(page.locator('body')).toHaveAttribute('data-page-kind', 'document');
});

test('documentation navigation opens the current document path', async ({ page }) => {
  await page.setViewportSize({ width: 1024, height: 900 });
  await page.goto('/reference/actions/documentation-action/');

  const current = page.locator('.nav-item[aria-current="page"]');
  await expect(current).toHaveCount(1);
  await expect(current.locator('xpath=ancestor::section[contains(@class, "nav-group")]/button')).toHaveAttribute('aria-expanded', 'true');
  await expect(current.locator('xpath=ancestor::li[contains(@class, "nav-subgroup")]/button')).toHaveAttribute('aria-expanded', 'true');
});

test('site header and sidebar use the documentation layout', async ({ page }) => {
  await page.setViewportSize({ width: 1544, height: 856 });
  await page.goto('/book/installation/');

  const header = page.locator('.site-header');
  await expect(header).toBeVisible();
  await expect(header.locator('.site-brand')).toContainText('Everyinit');
  await expect(header.locator('.site-version')).toHaveText('v0.2.4');
  await expect(header.locator('.site-primary-nav a')).toHaveText(['Docs', 'Reference', 'ESH']);
  await expect(page.locator('#navFilter')).toBeVisible();
  await expect(page.locator('.toc-rail')).toBeVisible();

  const layout = await page.evaluate(() => {
    const headerBox = document.querySelector('.site-header').getBoundingClientRect();
    const sidebarBox = document.querySelector('#sidebar').getBoundingClientRect();
    const articleBox = document.querySelector('.content-inner').getBoundingClientRect();
    const outlineBox = document.querySelector('.toc-rail').getBoundingClientRect();
    return { headerBox, sidebarBox, articleBox, outlineBox };
  });
  expect(layout.headerBox.height).toBe(56);
  expect(layout.sidebarBox.width).toBe(305);
  expect(layout.sidebarBox.top).toBe(layout.headerBox.bottom);
  expect(layout.articleBox.left).toBeGreaterThan(layout.sidebarBox.right);
  expect(layout.outlineBox.left).toBeGreaterThan(layout.articleBox.right);
});

test('home content uses regular Markdown without marketing components or tape playback', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/');
  await expect(page.locator('article h1')).toHaveText('Everyinit');
  await expect(page.locator('article h2')).not.toHaveCount(0);
  await expect(page.locator('.terminal-tape, .start-feature, .start-spotlight')).toHaveCount(0);
  const overflow = await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
  expect(overflow).toBeLessThanOrEqual(0);
});

test('search stays centered and safely inset from the top', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/reference/actions/documentation-action/');
  await page.locator('#searchToggle').click();
  const search = page.locator('#searchModal');
  const searchResults = page.locator('#docs-search-results');
  await expect(search).toBeVisible();
  await expect(searchResults).toBeHidden();
  let box = await search.boundingBox();
  let viewport = await page.evaluate(() => ({ width: document.documentElement.clientWidth, height: window.innerHeight }));
  expect(box.width).toBeLessThan(viewport.width);
  expect(Math.abs((box.x + (box.width / 2)) - (viewport.width / 2))).toBeLessThanOrEqual(1);
  expect(box.x).toBeGreaterThanOrEqual(24);
  expect(viewport.width - (box.x + box.width)).toBeGreaterThanOrEqual(24);
  expect(box.y).toBeGreaterThan(0);
  expect(box.y).toBeLessThan(viewport.height * 0.25);
  expect(box.height).toBeLessThanOrEqual(80);
  await page.locator('#docs-search-input').fill('documentation');
  await expect(searchResults).toBeVisible();
  await expect(searchResults.locator('.search-result-card').first()).toBeVisible();
  expect((await search.boundingBox()).height).toBeGreaterThan(80);
  await page.locator('#searchClose').click();

  await page.setViewportSize({ width: 700, height: 900 });
  await page.locator('#searchToggle').click();
  box = await search.boundingBox();
  viewport = await page.evaluate(() => ({ width: document.documentElement.clientWidth, height: window.innerHeight }));
  expect(box.width).toBeLessThanOrEqual(600);
  expect(box.height).toBeLessThan(viewport.height);
  expect(Math.abs((box.x + (box.width / 2)) - (viewport.width / 2))).toBeLessThanOrEqual(1);
  expect(box.y).toBeGreaterThanOrEqual(32);
});

test('documentation content reflows without horizontal page overflow', async ({ page }) => {
  for (const viewport of [{ width: 390, height: 844 }, { width: 1366, height: 900 }]) {
    await page.setViewportSize(viewport);
    await page.goto('/book/installation/');
    const overflow = await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
    expect(overflow).toBeLessThanOrEqual(0);
    await expect(page.locator('.prose')).toBeVisible();
  }
});

test('sidebar filter shows matching pages and restores the full navigation', async ({ page }) => {
  await page.setViewportSize({ width: 1024, height: 900 });
  await page.goto('/book/installation/');
  const filter = page.locator('#navFilter');
  await filter.fill('installation');
  await expect(page.locator('.nav-item:visible')).toHaveCount(1);
  await expect(page.locator('.nav-item:visible').first()).toContainText('Installation');
  await filter.fill('');
  expect(await page.locator('.nav-item:visible').count()).toBeGreaterThan(1);
});
