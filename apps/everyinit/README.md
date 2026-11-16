# Everyinit site

The Astro app publishes the Everyinit presentation page and the headless Evin
documentation package on `https://everyinit.dev`. The home page is `/`, and all
documentation routes are under `/docs/`. The former `learn.everyinit.dev` and
`docs.everyinit.dev` hosts permanently redirect to the matching canonical docs
URL on the main domain.

The documentation pages import `virtual:evin/docs` from `everyinit/vite`. The
Evin action remains the source of truth for page parsing, localized and
versioned routes, fallbacks, navigation order, and page data. Astro renders the
resulting AST and uses the manifest to create a page for every route.

## Translate the site interface

Astro's interface uses Fluent catalogs in `src/i18n/catalogs/`. The English
catalog, `en.ftl`, is the source of truth. Add a catalog named for its BCP 47
locale (for example, `pt-BR.ftl`) and translate every English message ID. The
site validates Fluent syntax and matching message IDs with:

```sh
npm run check:locales --workspace @everyinit/site
```

The verifier is part of `site:verify`. A complete non-English catalog
automatically adds a localized homepage at `/<locale>/`; documentation chrome
uses the current documentation route's locale and falls back to English when
that locale has no site catalog. Keep message IDs stable when editing English
copy so translation catalogs remain aligned.

## Run and preview locally

From the repository root, start the live Astro development server:

```sh
cargo evin-dev run site:dev
```

Open `http://localhost:4321/` for the project home and
`http://localhost:4321/docs/` for the generated documentation. The Vite
integration runs `docs:build` at startup and when a watched documentation file
changes. It uses the checked-out Evin CLI through Cargo when needed; set
`EVIN_BIN` to use an already-built Evin executable instead.

For a production-style preview, including the Cloudflare Pages redirects, run:

```sh
cargo evin-dev run site:preview
```

This builds the site and starts Wrangler at `http://127.0.0.1:8788/`. The home
is at `/`, and docs are available under `/docs/`. The canonical routes and
redirects from legacy docs URLs are covered by the site browser tests. Stop
either server with Ctrl-C.

## Validate changes

```sh
cargo evin-dev run docs:build
cargo evin-dev run site:check
cargo evin-dev run site:verify
cargo evin-dev run site:e2e
```

`site:verify` type-checks and statically builds the app. The focused
`site:e2e` task runs the home, generated docs, responsive layout,
search, and Cloudflare host-routing browser checks. `cargo evin-dev run e2e`
runs those alongside the Evin and ESH end-to-end suites. The root `npm run
build` command follows the same workspace release flow for Cloudflare Pages.

## Cloudflare Pages

The `Deploy` GitHub Actions workflow builds the app and uses Wrangler Direct
Upload, so Cloudflare does not need a separate Git build. Create the Pages
project once if it does not already exist:

```sh
corepack npm exec -- wrangler pages project create everyinit-site --production-branch main
```

The Wrangler deployment runs from `apps/everyinit`, where it can include
`functions/_middleware.ts` alongside the built `dist/` assets. That function
redirects the retired `learn.everyinit.dev` and `docs.everyinit.dev` hostnames
to canonical paths on `everyinit.dev`. Dashboard drag and drop uploads do not
compile Pages Functions, so the first upload only
bootstraps the project; the GitHub Actions Wrangler deployment publishes the
full site behavior. Attach `everyinit.dev` to the Pages project. Keep the two
former docs hostnames attached while their redirects are needed.

The `Deploy` GitHub Actions workflow deploys the exact commit after its `CI`
workflow succeeds. Pushes to `main` publish the production branch; pushes to
`release` publish a shared Pages preview. To review the latest shared preview,
push the changes to `release`, wait for CI and then the `Deploy` workflow to
finish, and open the URL reported by the Wrangler deploy step in that run's
logs. Pull requests from the repository's `automation/release` branch into
`main` also get a PR-specific preview and an updated preview comment; closing
that PR retires its deployment history. Other pull requests do not get an
automatic Pages preview. Production deployments also update the
installer and R2 mirror Workers from `infra/`. Keep deployment credentials
separate from the tokens used to publish release artifacts to R2: set
`CLOUDFLARE_DEPLOY_API_TOKEN` in the `release` GitHub environment and
`CLOUDFLARE_PREVIEW_DEPLOY_API_TOKEN` in the GitHub `unstable` environment. That
environment name is only used for the shared Pages preview deployment; it is
not a binary download channel. These should be
user-owned Cloudflare API tokens with `User → Memberships → Read` and
account-scoped `Cloudflare Pages → Edit`; the production token also needs
`Developer Platform → Workers → Edit` and `Zone → Workers Routes → Edit` for
`everyinit.dev`. Both environments use `CLOUDFLARE_ACCOUNT_ID`. The existing
`CLOUDFLARE_API_TOKEN` remains dedicated to release artifact publication.

The download Workers serve the installer and R2 mirror, while signed release
archives and channel pointers are published by the existing release workflow.
Deploying the site does not create release artifacts; a successful release
proposal is still required before the download links can serve binaries.
