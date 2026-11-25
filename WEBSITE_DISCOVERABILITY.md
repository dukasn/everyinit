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
Node test suite. The deployment of retry commit `af245b3` then submitted all 50
canonical URLs automatically with HTTP 200 on its first attempt.

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

- **Search consoles:** Add `everyinit.dev` as a Google Search Console Domain
  property and complete its DNS verification; Domain properties require DNS
  verification. Then submit `https://everyinit.dev/sitemap.xml`. Add the same
  domain to Bing Webmaster Tools and submit the sitemap there, or import the
  verified Google property. These account-level settings have not been
  verified. See Google's [property setup](https://support.google.com/webmasters/answer/34592)
  and [ownership verification](https://support.google.com/webmasters/answer/9008080),
  and Bing's [add and verify](https://www2.bing.com/webmasters/help/add-and-verify-site-12184f8b)
  and [sitemap submission](https://www2.bing.com/webmasters/help/sitemaps-3b5cf6ed)
  guides.
- **IndexNow:** `.github/workflows/deploy.yml` submits the canonical production
  URLs from `/sitemap.xml` after successful `main` deployments, retrying
  transient sitemap and API failures. The public verification key is served
  from `apps/everyinit/public/`. On 2026-11-25, the first rollout got HTTP 500
  while the sitemap was propagating; a manual retry was accepted with HTTP 202.
  The next rollout, with retry support, submitted 50 URLs automatically with
  HTTP 200 on its first attempt.
- **Cloudflare Crawler Hints:** This remains an optional account-level
  alternative. The [Cloudflare guide](https://developers.cloudflare.com/cache/advanced-configuration/crawler-hints/)
  says to enable it under Dashboard → Configuration. `wrangler whoami` reports
  that the local auth token has expired, so the account setting could not be
  inspected or changed from this workspace.
- **GitHub repository metadata:** The repository homepage is already set to
  `https://everyinit.dev/`. Current topics are `cli`, `dag`, `developer-tools`,
  `esh`, `evin`, `git-hooks`, `lsp`, `monorepo`, `orchestration`, `task-runner`,
  `workflow-automation`, `everyinit`, `canary`, and `canary-release`. The
  proposed additions are `workflow-orchestration`, `build-tool`, `shell`,
  `rust`, and `vscode-extension`; they have not been applied pending approval.
- **Published package READMEs:** `packages/everyinit/README.md` and
  `editors/vscode/README.md` link directly to canonical `/docs/book/.../` and
  `/docs/reference/.../` pages with trailing slashes.
- **Publisher verification:** The official [VS Code publisher verification
  guide](https://code.visualstudio.com/api/working-with-extensions/publishing-extension#verify-a-publisher)
  requires at least six months with an extension in the Marketplace and a
  domain registration that is also at least six months old. Once eligible,
  verify `https://everyinit.dev` from the publisher Details page and add the
  supplied DNS TXT record. The account's current eligibility has not been
  checked.

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
