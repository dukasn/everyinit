# Everyinit website discoverability

## Scope and architectural boundary

`evin/docs@v1` is a headless documentation producer: it parses `docs/`, extracts
frontmatter (`title` and `description` are already present across the Markdown
sources), resolves locale and version counterparts, and emits
framework-neutral page and route data. Crawler metadata, canonical URLs,
sitemaps, social cards, and HTTP headers belong in `apps/everyinit` and its
Cloudflare Pages middleware, so discoverability can be improved without editing
documentation prose.

## Implementation status

The on-site discoverability work below is implemented in the Astro app and
Cloudflare Pages middleware. The sitemap, `robots.txt`, and `llms.txt` are
generated from the Evin docs manifest; latest-version canonicalization,
noindex directives, trailing-slash URLs, metadata, structured data, and preview
host exclusions are covered by `site:verify` and `site:e2e`. Successful
production deployments submit the canonical sitemap URLs to IndexNow.

Production deployment of commit `892da00` completed on 2026-11-25. Live checks
confirmed `/robots.txt` and the IndexNow key use `text/plain`, `/sitemap.xml`
uses `application/xml`, and `/llms.txt` uses `text/plain`. The first automatic
IndexNow request reached the site before the sitemap was available and got
HTTP 500; retrying after deployment returned HTTP 202. The IndexNow script now
retries transient sitemap and API failures, with regression coverage in its
Node test suite.

Search-console verification, Cloudflare Crawler Hints, the remaining GitHub
topics, and Visual Studio Marketplace domain verification remain account-level
tasks. They require access to those services and are not changed by the site
build.

## Crawl and canonicalization behavior

The site applies these rules so crawlers see one canonical URL for each current
page and do not index preview or fallback copies:

1. **Version and locale copies.** Older docs versions and fallback locale
   routes emit `noindex, follow`; their canonical tags point to the latest
   available page in the content locale. The docs navigation lists routes only
   for the active version and requested locale.
2. **Legacy and unknown paths.** Known legacy root documentation paths return
   `308` redirects to their `/docs/.../` paths. The middleware uses a generated
   route set from `dist/docs-data/manifest.json`; unknown root and docs paths
   return `404` instead of serving the docs overview under another URL.
3. **One trailing-slash form.** Astro uses `trailingSlash: "always"`; app links,
   rendered Markdown links, search results, and canonical URLs use trailing
   slashes for page routes.
4. **Preview host exclusion.** Responses served from hosts other than
   `everyinit.dev` include `X-Robots-Tag: noindex, nofollow`, including Pages
   preview deployments and retired docs hosts.

## HTML head metadata and structured data

`apps/everyinit/src/layouts/SiteLayout.astro` shares the head contract across
`HomePage.astro` and `DocsShell.astro`:

- **Robots directive:** `SiteLayout.astro` accepts `noindex?: boolean` and
  emits `<meta name="robots" content="noindex, follow" />` for non-latest doc
  versions and fallback locale routes.
- **Open Graph and social cards:** The shared head emits `og:url` (bound to
  `canonical`), `og:locale`, `og:image`, `og:image:width` (`1200`),
  `og:image:height` (`630`), and `og:image:alt`, alongside `twitter:card`
  (`summary_large_image`), `twitter:title`, `twitter:description`, and
  `twitter:image`. `apps/everyinit/public/social-card.png` is a `1200x630`
  derivative of `BANNER.webp`.
- **`hreflang` alternates:** The home page uses `availableLocales`; docs pages
  use non-fallback variants for the active version. Both include `x-default`.
- **JSON-LD structured data (`<script type="application/ld+json">`):**
  - `HomePage.astro` emits `WebSite` and `SoftwareApplication`
    (`applicationCategory: "DeveloperApplication"`, `operatingSystem: "Linux"`,
    `license: "https://spdx.org/licenses/GPL-3.0-only.html"`,
    `codeRepository: "https://github.com/dukasn/everyinit"`, and `sameAs`
    referencing GitHub, `https://www.npmjs.com/package/everyinit`, and the
    `everyinit.everyinit` Visual Studio Marketplace listing).
  - `DocsShell.astro` emits `TechArticle` (`headline`, `description`, `url`,
    `inLanguage`) and a `BreadcrumbList` derived from `route.path` segments.
- **Home snippet and docs root title:** The Fluent
  `site-canonical-description` names repository workflows, task orchestration,
  Git hooks, CI, Evin, and ESH. The `/docs/` page has a distinct title from `/`.
- **Search-result favicon:** `apps/everyinit/public/` contains `/favicon.svg`,
  a 192x192 PNG fallback, and a minimal web manifest.

## Build-time discovery endpoints

Static Astro endpoints under `apps/everyinit/src/pages/` keep discovery files
synchronized with `virtual:evin/docs`:

- **`src/pages/sitemap.xml.ts`:** Generates an XML sitemap from
  `availableLocales` and `virtual:evin/docs`, including localized home routes
  and latest-version non-fallback docs routes with trailing slashes.
- **`src/pages/robots.txt.ts`:** Allows `User-agent: *` on `/` and declares the
  canonical sitemap URL.
- **`src/pages/llms.txt.ts`:** Emits a Markdown directory of canonical guide
  and reference URLs using each route's `metadata.title` and
  `metadata.description` from `virtual:evin/docs`.

## Pending account-level indexing and authority signals

On-page metadata helps crawlers classify `everyinit.dev`, while external links
and webmaster tools drive initial discovery and crawl frequency:

- **Search consoles:** Verify `everyinit.dev` as a DNS domain property in Google
  Search Console and Bing Webmaster Tools, then submit
  `https://everyinit.dev/sitemap.xml`. These account-level settings have not
  been verified from the repository.
- **IndexNow:** `.github/workflows/deploy.yml` submits the canonical production
  URLs from `/sitemap.xml` after successful `main` deployments, retrying
  transient sitemap and API failures. The public verification key is served
  from `apps/everyinit/public/`. A live submission was accepted with HTTP 202
  on 2026-11-25 after the initial post-deployment sitemap request returned
  HTTP 500; automatic retry behavior will be exercised by the next deployment.
- **Cloudflare Crawler Hints:** This remains an optional account-level
  alternative. Cloudflare account access is not configured in this workspace.
- **GitHub repository metadata:** The repository homepage is already set to
  `https://everyinit.dev/`. Existing topics include `task-runner`, `git-hooks`,
  and `lsp`; the remaining suggested topics are `workflow-orchestration`,
  `build-tool`, `shell`, `rust`, and `vscode-extension`.
- **Published package READMEs:** `packages/everyinit/README.md` and
  `editors/vscode/README.md` link directly to canonical `/docs/book/.../` and
  `/docs/reference/.../` pages with trailing slashes.
- **Publisher verification:** Complete Visual Studio Marketplace domain
  verification for `https://everyinit.dev` when the publisher account meets the
  Marketplace's current eligibility requirements.

## Verification coverage

`apps/everyinit/tests/site.spec.mjs` asserts:

- `/robots.txt`, `/sitemap.xml`, and `/llms.txt` return `200`, use canonical
  trailing-slash URLs, and exclude older versions from the sitemap.
- Latest docs pages omit `noindex`, include matching canonical and `og:url`
  values, and render parseable JSON-LD.
- Older docs pages emit `noindex, follow` and canonicalize to latest routes.
- Preview hosts receive `X-Robots-Tag: noindex, nofollow`; known legacy paths
  redirect with `308`; unknown paths return `404`.

Run `cargo evin-dev run site:verify` and `cargo evin-dev run site:e2e` after
changing discoverability behavior.
