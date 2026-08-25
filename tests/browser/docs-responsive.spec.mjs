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

test('start page keeps the persistent drawer closed by default', async ({ page }) => {
  await page.setViewportSize({ width: 1024, height: 900 });
  await page.goto('/');

  await expect(page.locator('#sidebarToggle')).toHaveAttribute('aria-expanded', 'false');
  await expect(page.locator('#appLayout')).toHaveClass(/sidebar-closed/);
  await expect.poll(() => page.locator('#sidebar').evaluate((element) => ({
    overflow: getComputedStyle(element).overflow,
    childMinWidth: getComputedStyle(element.firstElementChild).minInlineSize,
  }))).toEqual({ overflow: 'hidden', childMinWidth: '280px' });

  const drawerToggle = page.locator('#sidebarToggle');
  await drawerToggle.click();
  await expect(drawerToggle).toHaveAttribute('aria-expanded', 'true');
  await expect(page.locator('#appLayout')).toHaveClass(/sidebar-open/);
  await expect(page.locator('#sidebar')).toBeVisible();

  const groups = page.locator('.nav-group-toggle');
  await expect(groups).not.toHaveCount(0);
  expect(await groups.evaluateAll((toggles) => toggles.map((toggle) => toggle.getAttribute('aria-expanded'))))
    .toEqual(Array(await groups.count()).fill('false'));
  await groups.first().click();
  await expect(groups.first()).toHaveAttribute('aria-expanded', 'true');
});

test('documentation navigation opens the current document path', async ({ page }) => {
  await page.setViewportSize({ width: 1024, height: 900 });
  await page.goto('/reference/actions/documentation-action/');

  const current = page.locator('.nav-item[aria-current="page"]');
  await expect(current).toHaveCount(1);
  await expect(current.locator('xpath=ancestor::section[contains(@class, "nav-group")]/button')).toHaveAttribute('aria-expanded', 'true');
  await expect(current.locator('xpath=ancestor::li[contains(@class, "nav-subgroup")]/button')).toHaveAttribute('aria-expanded', 'true');
});

test('topbar is a 44px shell announcement and remembers dismissal', async ({ page }) => {
  await page.setViewportSize({ width: 1024, height: 900 });
  await page.goto('/');

  const topbar = page.locator('.evin-topbar');
  await expect(topbar).toHaveAttribute('data-topbar-variant', 'holo');
  await expect(topbar).toHaveAttribute('data-topbar-id', 'first-public-canary');
  await expect(topbar.locator('.topbar-action')).toHaveAttribute('href', 'https://github.com/dukasn/evin/releases');
  const layout = await page.evaluate(() => {
    const bar = document.querySelector('.evin-topbar').getBoundingClientRect();
    const app = document.querySelector('#appLayout').getBoundingClientRect();
    const sidebar = document.querySelector('#sidebar').getBoundingClientRect();
    return { top: bar.top, bottom: bar.bottom, width: bar.width, height: bar.height, appTop: app.top, appBottom: app.bottom, sidebarTop: sidebar.top, viewport: window.innerHeight };
  });
  expect(layout.top).toBe(0);
  expect(layout.width).toBeGreaterThanOrEqual(1022);
  expect(layout.height).toBe(44);
  expect(layout.appTop).toBe(layout.bottom);
  expect(layout.sidebarTop).toBe(layout.bottom);
  expect(layout.appBottom).toBeLessThanOrEqual(layout.viewport + 1);

  await topbar.locator('.topbar-close').click();
  await expect(topbar).toHaveClass(/is-hidden/);
  await page.reload();
  await expect(topbar).toHaveClass(/is-hidden/);
});

test('terminal tape player displays one frame and pauses without movement', async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/');

  const tape = page.locator('.terminal-tape').first();
  const svg = tape.locator('.evin-terminal-tape');
  await expect(svg).toBeVisible();

  const state = await svg.evaluate((element) => {
    return {
      duration: Number(element.dataset.tapeDurationMs),
      frames: element.querySelectorAll('.tape-frame').length,
      visibleFrames: [...element.querySelectorAll('.tape-frame')]
        .filter((frame) => getComputedStyle(frame).display !== 'none').length,
      starts: [...element.querySelectorAll('.tape-frame')]
        .map((frame) => Number(frame.dataset.tapeFrameStartMs)),
    };
  });

  expect(state.duration).toBeGreaterThan(0);
  expect(state.frames).toBeGreaterThan(1);
  expect(state.visibleFrames).toBe(1);
  expect(state.starts).toEqual([...state.starts].sort((a, b) => a - b));

  const playback = tape.locator('.tape-playback');
  const visibleFrame = () => svg.locator('.tape-frame').evaluateAll((frames) => (
    frames.findIndex((frame) => getComputedStyle(frame).display !== 'none')
  ));
  await playback.click();
  await expect(playback).toHaveAttribute('aria-pressed', 'true');
  const pausedFrame = await visibleFrame();
  await page.waitForTimeout(250);
  expect(await visibleFrame()).toBe(pausedFrame);

  await playback.click();
  await expect(playback).toHaveAttribute('aria-pressed', 'false');
  await expect.poll(visibleFrame).not.toBe(pausedFrame);
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

test('landing and footer grids reflow without page overflow', async ({ page }) => {
  for (const viewport of [{ width: 390, height: 844 }, { width: 1366, height: 900 }]) {
    await page.setViewportSize(viewport);
    await page.goto('/');
    const gridState = await page.evaluate(() => ({
      cardColumns: getComputedStyle(document.querySelector('.start-card-grid')).gridTemplateColumns.split(' ').length,
      footerColumns: getComputedStyle(document.querySelector('.start-footer-links')).gridTemplateColumns.split(' ').length,
      overflow: document.documentElement.scrollWidth - document.documentElement.clientWidth,
    }));
    expect(gridState.overflow).toBeLessThanOrEqual(0);
    expect(gridState.cardColumns).toBeGreaterThanOrEqual(1);
    expect(gridState.footerColumns).toBeGreaterThanOrEqual(1);
  }
});

test('start-page spotlight is centered, themed, and precedes the footer', async ({ page }) => {
  await page.setViewportSize({ width: 1200, height: 900 });
  await page.goto('/');
  const spotlight = page.locator('.start-spotlight');
  await expect(spotlight).toHaveAttribute('data-presentation-variant', 'holo');
  await expect(spotlight.locator('.start-spotlight-action')).toHaveAttribute('href', '/book');
  const state = await page.evaluate(() => {
    const spotlight = document.querySelector('.start-spotlight').getBoundingClientRect();
    const content = document.querySelector('.start-spotlight-content').getBoundingClientRect();
    const footer = document.querySelector('.start-footer').getBoundingClientRect();
    const action = document.querySelector('.start-spotlight-action');
    const title = document.querySelector('.start-spotlight h2');
    return {
      spotlight,
      content,
      footer,
      actionDecoration: getComputedStyle(action).textDecorationLine,
      actionLetterSpacing: getComputedStyle(action).letterSpacing,
      actionRadius: getComputedStyle(action).borderTopLeftRadius,
      actionBorder: getComputedStyle(action).borderTopWidth,
      topbarActionRadius: getComputedStyle(document.querySelector('.topbar-action')).borderTopLeftRadius,
      titleFontSize: Number.parseFloat(getComputedStyle(title).fontSize),
    };
  });
  expect(Math.abs((state.content.left + state.content.width / 2) - (state.spotlight.left + state.spotlight.width / 2))).toBeLessThanOrEqual(1);
  expect(state.spotlight.bottom).toBeLessThanOrEqual(state.footer.top);
  expect(state.actionDecoration).toBe('none');
  expect(state.actionLetterSpacing).not.toBe('normal');
  expect(state.actionRadius).toBe(state.topbarActionRadius);
  expect(state.actionBorder).toBe('1px');
  expect(state.titleFontSize).toBeGreaterThan(48);

  await page.locator('#themeToggle').click();
  await expect.poll(() => spotlight.evaluate(el => getComputedStyle(el).color)).not.toBe('rgba(0, 0, 0, 0)');
});

test('start page hero is full-width, cards show no underlines, and footer layout is correctly sized', async ({ page }) => {
  await page.setViewportSize({ width: 1200, height: 900 });
  await page.goto('/');

  const heroBox = await page.locator('.start-hero').boundingBox();
  const viewport = await page.evaluate(() => ({
    width: document.documentElement.clientWidth,
    height: window.innerHeight,
  }));
  expect(heroBox.width).toBeGreaterThanOrEqual(viewport.width - 2);

  const firstCard = page.locator('.start-card').first();
  const cardStyles = await firstCard.evaluate(el => ({
    textDecorationLine: getComputedStyle(el).textDecorationLine,
    iconWidth: getComputedStyle(el.querySelector('.start-card-icon')).width,
    iconHeight: getComputedStyle(el.querySelector('.start-card-icon')).height,
  }));
  expect(cardStyles.textDecorationLine).toBe('none');
  expect(cardStyles.iconWidth).toBe('40px');
  expect(cardStyles.iconHeight).toBe('40px');

  await firstCard.hover();
  const cardHoverDecoration = await firstCard.evaluate(el => getComputedStyle(el).textDecorationLine);
  expect(cardHoverDecoration).toBe('none');

  const footerLogo = await page.locator('.start-footer-logo').evaluate(el => ({
    width: getComputedStyle(el).width,
    height: getComputedStyle(el).height,
  }));
  expect(footerLogo.width).toBe('48px');
  expect(footerLogo.height).toBe('48px');
});
