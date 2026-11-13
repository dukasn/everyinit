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

Set the Pages project root to `apps/everyinit`, the build output directory to
`dist`, the install command to `cd ../.. && npm ci`, and the build command to
`../../.evin/cloudflare-build`. The `wrangler.toml` file records the project
output directory, and `functions/_middleware.ts` maps the documentation host
to the static `/docs/` route tree. Attach `everyinit.dev` and
`docs.everyinit.dev` to the same Pages project.
