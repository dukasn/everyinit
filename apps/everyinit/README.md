# Everyinit site

The Astro app publishes the Everyinit presentation page and the headless Evin
documentation package as a site. The home page is `/`; generated documentation
pages are built under `/docs/`. The Pages middleware maps the `docs.everyinit.dev`
host to those generated routes, so documentation URLs remain rooted at `/` on
that domain.

The documentation pages import `virtual:evin/docs` from `everyinit/vite`. The
Evin action remains the source of truth for page parsing, localized and
versioned routes, fallbacks, navigation order, and page data. Astro renders the
resulting AST and uses the manifest to create a page for every route.

## Run and preview locally

From the repository root, start the live Astro development server:

```sh
cargo evin-dev run everyinit-site:dev
```

Open `http://localhost:4321/` for the project home and
`http://localhost:4321/docs/` for the generated documentation. The Vite
integration runs `docs:build` at startup and when a watched documentation file
changes. It uses the checked-out Evin CLI through Cargo when needed; set
`EVIN_BIN` to use an already-built Evin executable instead.

For a production-style preview, including the Cloudflare Pages middleware that
maps the docs host to `/`, run:

```sh
cargo evin-dev run everyinit-site:preview
```

This builds the site and starts Wrangler at `http://127.0.0.1:8788/`. The home
is at `/`, and docs are available under `/docs/`; the host-specific rewrite is
covered by the site browser tests. Stop either server with Ctrl-C.

## Validate changes

```sh
cargo evin-dev run docs:build
cargo evin-dev run everyinit-site:check
cargo evin-dev run everyinit-site:verify
cargo evin-dev run everyinit-site:e2e
```

`everyinit-site:verify` type-checks and statically builds the app. The focused
`everyinit-site:e2e` task runs the home, generated docs, responsive layout,
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
maps `docs.everyinit.dev` to the static `/docs/` route tree. Dashboard drag and
drop uploads do not compile Pages Functions, so the first upload only
bootstraps the project; the GitHub Actions Wrangler deployment publishes the
full site behavior. Attach `everyinit.dev` and `docs.everyinit.dev` to this same
Pages project.

The `Deploy` GitHub Actions workflow deploys the exact commit after its `CI`
workflow succeeds. Pushes to `main` publish the production branch; pushes to
`release` publish a Pages preview. Production deployments also update the
installer and R2 mirror Workers from `infra/`. Keep deployment credentials
separate from the tokens used to publish release artifacts to R2: set
`CLOUDFLARE_DEPLOY_API_TOKEN` in the `release` GitHub environment and
`CLOUDFLARE_PREVIEW_DEPLOY_API_TOKEN` in `unstable`. These should be
user-owned Cloudflare API tokens with `User → Memberships → Read` and
account-scoped `Cloudflare Pages → Edit`; the production token also needs
`Developer Platform → Workers → Edit` and `Zone → Workers Routes → Edit` for
`everyinit.dev`. Both environments use `CLOUDFLARE_ACCOUNT_ID`. The existing
`CLOUDFLARE_API_TOKEN` remains dedicated to release artifact publication.

The download Workers serve the installer and R2 mirror, while signed release
archives and channel pointers are published by the existing release workflow.
Deploying the site does not create release artifacts; a successful release
proposal is still required before the download links can serve binaries.
