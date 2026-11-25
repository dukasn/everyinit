import { expect, test } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { indexNowKey } from "../scripts/indexnow-ping.mjs";

const widths = [320, 390, 640, 700, 768, 820, 1023, 1024, 1366];

async function expectNoHorizontalOverflow(page) {
  const dimensions = await page.evaluate(() => ({
    document: document.documentElement.scrollWidth,
    viewport: document.documentElement.clientWidth,
    overflowing: [...document.querySelectorAll("body *")]
      .map((element) => ({
        element: `${element.tagName.toLowerCase()}${element.className ? `.${String(element.className).replaceAll(" ", ".")}` : ""}`,
        right: Math.round(element.getBoundingClientRect().right),
      }))
      .filter((element) => element.right > document.documentElement.clientWidth + 1)
      .slice(0, 8),
  }));
  expect(dimensions.document, JSON.stringify(dimensions.overflowing)).toBeLessThanOrEqual(
    dimensions.viewport,
  );
}

test("the project home presents Evin and links to current installation instructions", async ({
  page,
}) => {
  await page.goto("/");
  await expect(page).toHaveTitle(/Everyinit/);
  await expect(page.getByRole("heading", { level: 1 })).toContainText("Development");
  const heroCode = page.locator(".home-hero-code code");
  await expect(heroCode.locator('[data-scope="markup.bold.flow"]').first()).toHaveText("lint");
  await expect(heroCode.locator('[data-scope="keyword.control.property.flow"]').first()).toHaveText(
    "name",
  );
  await expect(
    heroCode
      .locator('[data-scope="entity.name.tag.enum.flow"]')
      .filter({ hasText: "Check" })
      .first(),
  ).toHaveText("Check");
  expect(
    await heroCode
      .locator('[data-scope="keyword.control.property.flow"]')
      .first()
      .evaluate((element) => getComputedStyle(element).color),
  ).toBe("rgb(232, 121, 114)");
  for (const project of ["Evin", "ESH", "IDE integrations", "Everyinit for the web"]) {
    await expect(page.getByRole("heading", { name: project, exact: true })).toBeVisible();
  }
  await expect(page.getByText(/VS Code extension packages syntax support/)).toBeVisible();
  await expect(page.getByText("NPM PACKAGE · PUBLISHED")).toBeVisible();
  const footer = page.getByRole("contentinfo");
  await expect(footer.getByRole("link", { name: "Project license" })).toHaveAttribute(
    "href",
    "https://github.com/dukasn/everyinit/blob/main/LICENSE",
  );
  await expect(footer.getByRole("link", { name: "Third-party notices" })).toBeVisible();
  await expect(footer.getByRole("link", { name: "Acknowledgments" })).toBeVisible();
  await expect(footer.getByRole("link", { name: "Trademark policy" })).toBeVisible();
  await expect(page.getByRole("link", { name: /documentation/i }).first()).toHaveAttribute(
    "href",
    "/docs/",
  );
  await expect(page.getByRole("link", { name: "Install Everyinit" }).first()).toHaveAttribute(
    "href",
    "/docs/book/installation/",
  );
  await expect(page.getByText(/Linux x86_64 CLI downloads available/)).toBeVisible();

  for (const width of widths) {
    await page.setViewportSize({ width, height: 900 });
    await expectNoHorizontalOverflow(page);
    const backdrop = await page.locator(".home-hero-backdrop").boundingBox();
    expect(backdrop?.x).toBeGreaterThanOrEqual(0);
    expect(backdrop && backdrop.x + backdrop.width).toBeLessThanOrEqual(width);
    if (width < 1024) {
      await expect(page.locator(".home-mobile-nav > summary")).toBeVisible();
      await expect(page.getByRole("navigation", { name: "Main navigation" })).toBeHidden();
    }
  }
});

test("the home navigation menus work with pointer, keyboard, and touch layouts", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1366, height: 900 });
  await page.goto("/");

  const desktopNavigation = page.getByRole("navigation", { name: "Main navigation" });
  const toolsMenu = desktopNavigation.locator(".home-nav-menu").nth(0);
  await toolsMenu.locator("summary").focus();
  await page.keyboard.press("Enter");
  await expect(toolsMenu.getByRole("link", { name: /IDE integrations/ })).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(toolsMenu).not.toHaveAttribute("open", "");
  await expect(toolsMenu.locator("summary")).toBeFocused();

  const resourcesMenu = desktopNavigation.locator(".home-nav-menu").nth(1);
  await resourcesMenu.locator("summary").click();
  await expect(resourcesMenu.getByRole("link", { name: "Documentation" })).toBeVisible();
  await expect(toolsMenu).not.toHaveAttribute("open", "");

  await page.setViewportSize({ width: 1024, height: 900 });
  const helpMenu = desktopNavigation.locator(".home-nav-menu").nth(2);
  await helpMenu.locator("summary").click();
  const helpMenuBounds = await helpMenu.locator(":scope > div").boundingBox();
  expect(helpMenuBounds?.x).toBeGreaterThanOrEqual(0);
  expect(helpMenuBounds && helpMenuBounds.x + helpMenuBounds.width).toBeLessThanOrEqual(1024);

  await page.setViewportSize({ width: 320, height: 700 });
  const mobileMenu = page.locator(".home-mobile-nav");
  await mobileMenu.locator(":scope > summary").click();
  const mobileNavigation = page.getByRole("navigation", { name: "Mobile navigation" });
  await expect(mobileNavigation).toBeVisible();
  const helpGroup = mobileMenu.locator(".home-mobile-nav-group").nth(2);
  const helpChevron = helpGroup.locator(".home-nav-chevron");
  expect(await helpChevron.evaluate((icon) => getComputedStyle(icon).transform)).toBe("none");
  await helpGroup.locator("summary").click();
  expect(await helpChevron.evaluate((icon) => getComputedStyle(icon).transform)).not.toBe("none");
  await expect(
    mobileNavigation.getByRole("link", { name: /Ask or report an issue/ }),
  ).toBeVisible();
  await expectNoHorizontalOverflow(page);
  await page.keyboard.press("Escape");
  await expect(mobileNavigation).toBeHidden();

  await page.setViewportSize({ width: 900, height: 500 });
  await mobileMenu.locator(":scope > summary").click();
  const compactMenuSize = await mobileNavigation.evaluate((menu) => ({
    contentHeight: menu.scrollHeight,
    visibleHeight: menu.clientHeight,
  }));
  expect(compactMenuSize.contentHeight).toBeLessThanOrEqual(compactMenuSize.visibleHeight);
  for (const group of ["Tools", "Resources", "Help"]) {
    await expect(
      mobileNavigation.locator(".home-mobile-nav-group > summary", { hasText: group }),
    ).toBeVisible();
  }
});

test("discovery endpoints expose only canonical latest documentation routes", async ({ page }) => {
  const robots = await page.request.get("/robots.txt");
  expect(robots.status()).toBe(200);
  expect(await robots.text()).toContain("Sitemap: https://everyinit.dev/sitemap.xml");

  const sitemap = await page.request.get("/sitemap.xml");
  expect(sitemap.status()).toBe(200);
  const sitemapBody = await sitemap.text();
  expect(sitemapBody).toContain("https://everyinit.dev/");
  expect(sitemapBody).toContain("https://everyinit.dev/docs/book/installation/");
  expect(sitemapBody).not.toContain("/docs/v0.2.4/");
  expect(sitemapBody).toMatch(/<loc>https:\/\/everyinit\.dev\/docs\/[\w/-]+<\/loc>/);

  const llms = await page.request.get("/llms.txt");
  expect(llms.status()).toBe(200);
  const llmsBody = await llms.text();
  expect(llmsBody).toContain("# Everyinit documentation");
  expect(llmsBody).toContain("https://everyinit.dev/docs/book/installation/");
  expect(llmsBody).not.toContain("/docs/v0.2.4/");

  const indexNowKeyFile = await page.request.get(`/${indexNowKey}.txt`);
  expect(indexNowKeyFile.status()).toBe(200);
  expect((await indexNowKeyFile.text()).trim()).toBe(indexNowKey);
});

test("home and documentation pages publish canonical crawler metadata", async ({ page }) => {
  await page.goto("/");
  await expect(page.locator('link[rel="canonical"]')).toHaveAttribute(
    "href",
    "https://everyinit.dev/",
  );
  await expect(page.locator('meta[property="og:url"]')).toHaveAttribute(
    "content",
    "https://everyinit.dev/",
  );
  await expect(page.locator('meta[property="og:image"]')).toHaveAttribute(
    "content",
    "https://everyinit.dev/social-card.png",
  );
  await expect(page.locator('meta[name="description"]')).toHaveAttribute(
    "content",
    /repository workflows/,
  );
  await expect(page.locator('meta[name="twitter:card"]')).toHaveAttribute(
    "content",
    "summary_large_image",
  );
  await expect(page.locator('link[rel="alternate"][hreflang="x-default"]')).toHaveAttribute(
    "href",
    "https://everyinit.dev/",
  );
  const homeSchemas = JSON.parse(
    await page.locator('script[type="application/ld+json"]').textContent(),
  );
  expect(homeSchemas.map((schema) => schema["@type"])).toEqual(["WebSite", "SoftwareApplication"]);
  expect(homeSchemas[1].license).toBe("https://spdx.org/licenses/GPL-3.0-only.html");

  const socialCard = await page.request.get("/social-card.png");
  expect(socialCard.status()).toBe(200);
  const socialBytes = Buffer.from(await socialCard.body());
  expect(socialBytes.readUInt32BE(16)).toBe(1200);
  expect(socialBytes.readUInt32BE(20)).toBe(630);
  const favicon = await page.request.get("/favicon-192.png");
  expect(favicon.status()).toBe(200);
  const faviconBytes = Buffer.from(await favicon.body());
  expect(faviconBytes.readUInt32BE(16)).toBe(192);
  expect(faviconBytes.readUInt32BE(20)).toBe(192);
  const manifest = await page.request.get("/site.webmanifest");
  expect(manifest.status()).toBe(200);
  expect((await manifest.json()).icons[0].src).toBe("/favicon-192.png");

  await page.goto("/docs/book/installation/");
  await expect(page.locator('meta[name="robots"]')).toHaveCount(0);
  await expect(page.locator('link[rel="canonical"]')).toHaveAttribute(
    "href",
    "https://everyinit.dev/docs/book/installation/",
  );
  await expect(page.locator('meta[property="og:url"]')).toHaveAttribute(
    "content",
    "https://everyinit.dev/docs/book/installation/",
  );
  await expect(page.locator('link[rel="alternate"][hreflang="en"]')).toHaveAttribute(
    "href",
    "https://everyinit.dev/docs/book/installation/",
  );
  await expect(page.locator(".docs-sidebar a[href*='/docs/v0.2.4/']")).toHaveCount(0);
  const docsSchemas = JSON.parse(
    await page.locator('script[type="application/ld+json"]').textContent(),
  );
  expect(docsSchemas.map((schema) => schema["@type"])).toEqual(["TechArticle", "BreadcrumbList"]);

  await page.goto("/docs/v0.2.4/book/installation/");
  await expect(page.locator('meta[name="robots"]')).toHaveAttribute("content", "noindex, follow");
  await expect(page.locator('link[rel="canonical"]')).toHaveAttribute(
    "href",
    "https://everyinit.dev/docs/book/installation/",
  );
});

test("the docs site renders generated routes and manifest selectors", async ({ page }) => {
  await page.setViewportSize({ width: 1366, height: 900 });
  await page.goto("/docs/");
  await expect(page).toHaveTitle(/Evin Docs/);
  await expect(page.locator("main h1")).toHaveText("Everyinit");
  const flowExample = page.locator(".doc-content code[data-language='flow']").first();
  await expect(flowExample.locator(".syntax-section").first()).toHaveText("workspace");
  const sectionToken = flowExample.locator('[data-scope="comment.block.section.flow"]').first();
  const propertyToken = flowExample.locator('[data-scope="keyword.control.property.flow"]').first();
  const enumToken = flowExample.locator('[data-scope="entity.name.tag.enum.flow"]').first();
  const taskToken = flowExample.locator('[data-scope="markup.bold.flow"]').first();
  await expect(propertyToken).toBeVisible();
  await expect(enumToken).toBeVisible();
  await expect(taskToken).toBeVisible();
  expect(await sectionToken.evaluate((element) => getComputedStyle(element).color)).toBe(
    "rgb(158, 172, 153)",
  );
  expect(await propertyToken.evaluate((element) => getComputedStyle(element).color)).toBe(
    "rgb(232, 121, 114)",
  );
  expect(await enumToken.evaluate((element) => getComputedStyle(element).color)).toBe(
    "rgb(167, 207, 140)",
  );
  await page.goto("/docs/book/compose-workflows/");
  const diagram = page.locator(".doc-content .diagram-svg").first();
  await expect(diagram).toBeVisible();
  await expect(diagram).toHaveAttribute("role", "img");
  await expect(diagram.locator(":scope > title")).toHaveCount(1);
  const diagramLabels = diagram.locator("foreignObject .nodeLabel p");
  await expect(diagramLabels).toHaveCount(3);
  await expect(diagramLabels).toContainText(["lint", "package", "test"]);
  expect(
    await diagram.evaluate((element) =>
      getComputedStyle(element).getPropertyValue("--diagram-ink").trim(),
    ),
  ).toBe("#15241b");
  await expect(page.locator(".doc-content figure.diagram img")).toHaveCount(0);
  const latestVersion = (await page.locator("header details summary").first().innerText()).match(
    /v\d+\.\d+\.\d+/,
  )?.[0];
  expect(latestVersion).toBeDefined();
  await expect(page.locator("header details summary").nth(1)).toContainText("en");
  await expect(page.locator(".docs-sidebar-heading", { hasText: "Guides" })).toBeVisible();

  await page.goto("/docs/book/installation/");
  await expect(page).toHaveTitle(/Installation/);
  await expect(page.locator("main h1")).toHaveText("Installation & Quickstart");
  await expect(
    page.getByRole("link", { name: "Everyinit from the Visual Studio Marketplace" }),
  ).toHaveAttribute(
    "href",
    "https://marketplace.visualstudio.com/items?itemName=everyinit.everyinit",
  );
  await expect(page.locator(".doc-content")).toContainText("No official prebuilt binaries");
  await expect(page.locator(".doc-content")).toContainText(
    "The Marketplace extension and everyinit npm package follow the regular workspace version train",
  );
  await expect(page.locator(".doc-content pre code").first()).toContainText(
    "get.everyinit.dev/all",
  );
  await expect(page.getByRole("link", { name: "Installation & Quickstart" }).first()).toBeVisible();
  await expect(page.locator(".docs-page-nav")).toBeVisible();
  await expect(page.locator(".docs-page-previous")).toContainText("The Evin Book");
  await expect(page.locator(".docs-page-next")).toContainText("Run your first workflow");
  const overviewLink = page.locator(".docs-quick-overview");
  await expect(overviewLink).toHaveAttribute("href", "/docs/");
  await expect(page.locator(".docs-nav-overview")).toHaveCount(0);
  await overviewLink.click();
  await expect(page).toHaveURL(/\/docs\/$/);
  await expect(page.locator("main h1")).toHaveText("Everyinit");
  await page.goto("/docs/book/installation/");
  await page.locator(".docs-variant summary").first().click();
  await expect(page.locator(".docs-variant-menu").first()).toBeVisible();
  await expect(page.locator(".docs-variant-option.is-selected").first()).toContainText(
    latestVersion,
  );
  await page.goto("/install/");
  await expect(page).toHaveURL(/\/docs\/book\/installation\/$/);
  await expect(page.locator("main h1")).toHaveText("Installation & Quickstart");
  await page.goto("/docs/install");
  await expect(page).toHaveURL(/\/docs\/book\/installation\/$/);
  await expect(page.locator("main h1")).toHaveText("Installation & Quickstart");
  await page.goto("/docs/book/esh/first-script/");
  const eshExample = page.locator(".doc-content code[data-language='esh']").first();
  await expect(eshExample.locator(".syntax-builtin").first()).toContainText("echo");
  await expect(
    eshExample.locator(".syntax-variable").filter({ hasText: "$TARGET" }).first(),
  ).toBeVisible();
});

test("documentation navigation and search work at desktop and mobile widths", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("/docs/book/installation/");
  await page.locator("#docs-search-open").click();
  const modalBox = await page.locator("#docs-search-dialog").boundingBox();
  expect(Math.abs(modalBox.x + modalBox.width / 2 - 195)).toBeLessThan(2);
  expect(Math.abs(modalBox.y + modalBox.height / 2 - 422)).toBeLessThan(2);
  const searchInput = page.locator("#docs-search-input");
  const focusCue = await page.locator(".docs-search-input-row").evaluate((row) => ({
    rowShadow: getComputedStyle(row).boxShadow,
    inputOutline: getComputedStyle(document.querySelector("#docs-search-input")).outlineStyle,
  }));
  expect(focusCue.rowShadow).not.toBe("none");
  expect(focusCue.inputOutline).toBe("none");
  const inputBox = await searchInput.boundingBox();
  await searchInput.fill("installation");
  const populatedModalBox = await page.locator("#docs-search-dialog").boundingBox();
  const populatedInputBox = await searchInput.boundingBox();
  expect(populatedModalBox.height).toBe(modalBox.height);
  expect(populatedInputBox.y).toBe(inputBox.y);
  expect(populatedInputBox.height).toBe(inputBox.height);
  const mobileResult = page.locator("[data-search-option]:visible").first();
  await expect(mobileResult).toContainText("Installation & Quickstart");
  await searchInput.press("ArrowDown");
  await expect(mobileResult).toHaveAttribute("aria-selected", "true");
  await searchInput.press("Enter");
  await expect(page).toHaveURL(/\/docs\/book\/installation\/$/);
  await page.locator("#docs-search-open").click();
  await page.locator("#docs-search-input").fill("Rustup");
  const bodyResult = page.locator("[data-search-option]:visible").first();
  await expect(bodyResult).toContainText("Rustup");
  await expect(bodyResult).toHaveAttribute("href", "/docs/book/installation/#build-from-source");
  await page.keyboard.press("Escape");
  await page.locator(".docs-mobile-nav > summary").click();
  await expect(page.locator(".docs-mobile-nav")).toContainText("Workflows");
  await page.locator("#docs-mobile-search").click();
  await page.keyboard.press("Escape");
  await expect(page.locator("#docs-mobile-search")).toBeFocused();
  await expectNoHorizontalOverflow(page);

  await page.evaluate(() => window.scrollTo(0, document.body.scrollHeight));
  await expect(page.locator("#docs-site-header")).toHaveClass(/is-hidden/);
  await page.evaluate(() => window.scrollTo(0, 0));
  await expect(page.locator("#docs-site-header")).not.toHaveClass(/is-hidden/);

  await page.setViewportSize({ width: 1366, height: 900 });
  await page.goto("/docs/book/installation/");
  await page.keyboard.press("Control+k");
  await expect(page.locator("#docs-search-dialog")).toBeVisible();
  await page.locator("#docs-search-input").fill("workspce configuration");
  await expect(page.locator("[data-search-option]:visible").first()).toContainText(
    "Workspace configuration",
  );
  await page.keyboard.press("Escape");
  await expect(page.locator("#docs-search-dialog")).toBeHidden();
  await expectNoHorizontalOverflow(page);
});

test("search keeps results across dismissal and restores the invoking control", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1366, height: 900 });
  await page.goto("/docs/book/installation/");
  const sidebarSearch = page.locator("#docs-sidebar-search");
  await sidebarSearch.click();
  const input = page.locator("#docs-search-input");
  await expect(input).toBeFocused();
  await input.fill("workspace configuration");
  const firstResult = page.locator("[data-search-option]:visible").first();
  await expect(firstResult).toContainText("Workspace configuration");
  const highlightedRanges = await page.evaluate(
    () => CSS.highlights.get("docs-search-match")?.size ?? 0,
  );
  expect(highlightedRanges).toBeGreaterThan(0);
  await input.press("Tab");
  await expect(firstResult).toBeFocused();
  await expect(firstResult).toHaveAttribute("aria-selected", "true");
  await expect(firstResult).toHaveClass(/is-active/);
  await expect
    .poll(() => firstResult.evaluate((result) => getComputedStyle(result).backgroundColor))
    .toBe("rgb(231, 237, 226)");
  expect(await firstResult.evaluate((result) => getComputedStyle(result).outlineStyle)).toBe(
    "none",
  );
  await page.keyboard.press("Shift+Tab");
  await expect(input).toBeFocused();
  const savedResults = await page.locator("[data-search-option]:visible").allTextContents();

  await page.keyboard.press("Escape");
  await expect(page.locator("#docs-search-dialog")).toBeHidden();
  await expect(sidebarSearch).toBeFocused();
  await expect(input).toHaveValue("workspace configuration");

  await page.keyboard.press("Control+k");
  await expect(input).toBeFocused();
  await expect(input).toHaveValue("workspace configuration");
  await expect(page.locator("[data-search-option]:visible")).toHaveCount(savedResults.length);
  await expect(page.locator("#docs-search-input")).toHaveAttribute("aria-expanded", "true");

  await input.press("Home");
  await expect(page.locator("[data-search-option][aria-selected='true']")).toHaveCount(1);
  await expect(input).toHaveAttribute("aria-activedescendant", "docs-search-option-0");
  await input.press("End");
  const lastOption = page.locator("[data-search-option]:visible").last();
  await expect(lastOption).toHaveAttribute("aria-selected", "true");
  await input.press("PageUp");
  await expect(page.locator("[data-search-option][aria-selected='true']")).toHaveCount(1);
  await page.keyboard.press("Escape");
  await expect(sidebarSearch).toBeFocused();
  await expect(input).toHaveAttribute("aria-expanded", "false");
  await expect(input).not.toHaveAttribute("aria-activedescendant", /.+/);
});

test("search gives useful empty, no-match, and recovery states", async ({ page }) => {
  await page.setViewportSize({ width: 1366, height: 900 });
  await page.goto("/docs/book/installation/");
  await page.locator("#docs-search-open").click();
  await expect(page.getByRole("heading", { name: "What are you working on?" })).toBeVisible();
  const queryButton = page.locator("[data-search-query='workspace configuration']").first();
  await expect(queryButton).toBeVisible();
  await queryButton.click();
  await expect(page.locator("#docs-search-input")).toHaveValue("workspace configuration");
  await expect(page.locator("[data-search-option]:visible").first()).toContainText(
    "Workspace configuration",
  );

  await page.locator("#docs-search-input").fill("qzxv-no-such-page");
  await expect(page.getByRole("heading", { name: "No matches just yet" })).toBeVisible();
  await expect(page.locator("#docs-search-empty-message")).toContainText("Try a shorter phrase");
  await page.locator("#docs-search-empty [data-search-query='documentation action']").click();
  await expect(page.locator("#docs-search-input")).toHaveValue("documentation action");
  await expect(page.locator("[data-search-option]:visible").first()).toBeVisible();
});

test("the open search dialog locks page scrolling and contains result scrolling", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1366, height: 900 });
  await page.goto("/docs/book/installation/");
  await page.evaluate(() => window.scrollTo(0, 500));
  await page.keyboard.press("Control+k");
  const input = page.locator("#docs-search-input");
  await input.fill("workflow");
  const results = page.locator("#docs-search-results");
  await expect(results.locator("[data-search-option]").first()).toBeVisible();
  const scrollable = await results.evaluate(
    (element) => element.scrollHeight > element.clientHeight,
  );
  expect(scrollable).toBe(true);

  const pageScroll = await page.evaluate(() => window.scrollY);
  await page.mouse.move(8, 300);
  await page.mouse.wheel(0, 600);
  await expect.poll(() => page.evaluate(() => window.scrollY)).toBe(pageScroll);

  await results.hover();
  await page.mouse.wheel(0, 500);
  await expect.poll(() => results.evaluate((element) => element.scrollTop)).toBeGreaterThan(0);
});

test("a search finishing after dismissal is ready when the dialog reopens", async ({ page }) => {
  let releaseSearchAsset;
  const searchAssetPaused = new Promise((resolve) => {
    releaseSearchAsset = resolve;
  });
  await page.route("**/*.bin", async (route) => {
    await searchAssetPaused;
    await route.continue();
  });
  await page.setViewportSize({ width: 1366, height: 900 });
  await page.goto("/docs/book/installation/");
  const trigger = page.locator("#docs-search-open");
  await trigger.click();
  const input = page.locator("#docs-search-input");
  await input.fill("Rustup");
  await expect(page.locator("#docs-search-status")).toContainText("Searching documentation");
  await page.keyboard.press("Escape");
  await expect(trigger).toBeFocused();
  releaseSearchAsset();

  await expect(page.locator("[data-search-option]").first()).toContainText("Rustup");
  await trigger.click();
  await expect(input).toHaveValue("Rustup");
  await expect(page.locator("[data-search-option]:visible").first()).toContainText("Rustup");
});

test("Pages serves docs on everyinit.dev and redirects retired docs hosts", () => {
  const fetchForHost = (host, pathname) =>
    execFileSync(
      "curl",
      [
        "--fail",
        "--silent",
        "--show-error",
        "--location",
        "--resolve",
        `${host}:8788:127.0.0.1`,
        `http://${host}:8788${pathname}`,
      ],
      { encoding: "utf8" },
    );

  expect(fetchForHost("everyinit.dev", "/docs/")).toContain("Evin Docs overview · Everyinit");
  expect(fetchForHost("everyinit.dev", "/docs/book/installation")).toContain(
    "Installation &amp; Quickstart · Evin Docs",
  );
  expect(fetchForHost("everyinit.dev", "/install/")).toContain(
    "Installation &amp; Quickstart · Evin Docs",
  );
  expect(fetchForHost("everyinit.dev", "/docs/install")).toContain(
    "Installation &amp; Quickstart · Evin Docs",
  );
  const redirectForHost = (host, pathname) =>
    execFileSync(
      "curl",
      [
        "--silent",
        "--show-error",
        "--output",
        "/dev/null",
        "--write-out",
        "%{http_code} %{redirect_url}",
        "--resolve",
        `${host}:8788:127.0.0.1`,
        `http://${host}:8788${pathname}`,
      ],
      { encoding: "utf8" },
    );
  expect(redirectForHost("learn.everyinit.dev", "/book/installation")).toBe(
    "308 https://everyinit.dev/docs/book/installation/",
  );
  expect(redirectForHost("docs.everyinit.dev", "/")).toBe("308 https://everyinit.dev/docs/");
  expect(redirectForHost("everyinit.dev", "/book/installation")).toBe(
    "308 http://everyinit.dev:8788/docs/book/installation/",
  );
  expect(redirectForHost("everyinit.dev", "/docs/not-a-documentation-route")).toMatch(/^404\b/);
  expect(fetchForHost("everyinit.dev", "/")).toContain("Everyinit — development, orchestrated");

  const unknownRootStatus = execFileSync(
    "curl",
    [
      "--silent",
      "--show-error",
      "--output",
      "/dev/null",
      "--write-out",
      "%{http_code}",
      "--resolve",
      "everyinit.dev:8788:127.0.0.1",
      "http://everyinit.dev:8788/not-a-documentation-route/",
    ],
    { encoding: "utf8" },
  );
  expect(unknownRootStatus).toBe("404");

  const previewHeaders = execFileSync(
    "curl",
    [
      "--silent",
      "--show-error",
      "--head",
      "--resolve",
      "pr-123.everyinit-site.pages.dev:8788:127.0.0.1",
      "http://pr-123.everyinit-site.pages.dev:8788/",
    ],
    { encoding: "utf8" },
  );
  expect(previewHeaders).toMatch(/x-robots-tag: noindex, nofollow/i);
});

test("docs navbar links separate project home from docs home", async ({ page }) => {
  await page.setViewportSize({ width: 1366, height: 900 });
  await page.goto("/docs/book/installation/");

  const homeLink = page.locator(".docs-brand-home");
  const docsLink = page.locator(".docs-brand-docs");
  await expect(homeLink).toHaveAttribute("href", "/");
  await expect(docsLink).toHaveAttribute("href", "/docs/");

  await docsLink.click();
  await expect(page).toHaveURL(/\/docs\/$/);
  await expect(page.locator("main h1")).toHaveText("Everyinit");

  await page.locator(".docs-brand-home").click();
  await expect(page).toHaveURL(/\/$/);
  await expect(page.getByRole("heading", { level: 1 })).toContainText("Development");
});

test("daily accent color auto-updates across midnight", async ({ page }) => {
  await page.clock.install({ time: new Date(2026, 10, 18, 23, 59, 58) });
  await page.setViewportSize({ width: 1366, height: 900 });
  await page.goto("/docs/");

  await expect(page.locator("html")).toHaveAttribute("data-accent", "amber");
  await expect(page.locator("html")).toHaveClass(/is-accent-ready/);

  await page.clock.fastForward(4000);
  await expect(page.locator("html")).toHaveAttribute("data-accent", "apricot");
});
