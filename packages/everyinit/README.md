# `everyinit`

The official JavaScript package for consuming Evin's generated documentation
data. Install once, then import the core API or a supported integration by
subpath:

For a step-by-step walkthrough, see the
[guide to writing project documentation](https://everyinit.dev/docs/book/write-project-documentation)
and [building a documentation site](https://everyinit.dev/docs/book/delivery/build-a-documentation-site).
The [hosted API reference](https://everyinit.dev/docs/reference/everyinit-package)
covers the exported helpers, Vite modules, search, and highlighting APIs.

```sh
npm install everyinit
```

Everyinit is in canary-era development, so APIs may change between releases.
This package follows the workspace's regular version train; the `canary`
channel applies to CLI and ESH downloads, not npm package versioning.

The release workflow publishes `everyinit` to npmjs and the same package files
as `@dukasn/everyinit` to GitHub Packages. To install the GitHub Packages copy,
configure the `@dukasn` scope to use `https://npm.pkg.github.com` and authenticate
with a token that can read the package.

```js
import { createDocsRuntime } from "everyinit";
import evinDocs from "everyinit/vite";
```

The framework-neutral entry point provides runtime helpers and types for Evin's
manifest, page format, and binary search index. `everyinit/vite` provides the
Vite plugin, `virtual:evin/docs`, and `virtual:evin/search`. The plugin runs the
configured Evin task, watches documentation sources, and exposes generated
route, page, locale, version, asset, and search data. Search indexes load only
when requested:

```js
import { loadSearchIndex, searchDocs } from "virtual:evin/search";

const index = await loadSearchIndex("latest", "en");
const results = searchDocs(index, "workspace configuration");
```

## Flow, ESH, and JSON code highlighting

`everyinit/highlight` tokenizes Evin Flow and ESH with the same TextMate
grammars used by the VS Code extension, and also supports standard JSON.
Initialize one highlighter for the app, then render its source-preserving
segments as spans, native elements, or theme tokens:

```js
import { createCodeHighlighter } from "everyinit/highlight";

const highlighter = await createCodeHighlighter();
const tokens = highlighter.tokenizeCode('workspace:\n  name: "site"\n', "flow");
// [{ text: "workspace", scope: "comment.block.section.flow", scopes: [...] }, ...]
```

`supportedCodeLanguages` lists the canonical identifiers: `flow`, `esh`, and
`json` (also `.flow`, `.esh`, `evin-flow`, and `evin-esh` are accepted).
Unknown identifiers return the source as one unscoped token so integrations
can keep their own highlighters as fallback. Tokens include both the most
specific `scope` and the full `scopes` stack. The package uses Shiki's
TextMate engine and Oniguruma WASM; framework integrations can use the
grammar's full scope detail or map it into a smaller visual theme.
`tokenizeCode` is also exported as an async helper that shares a lazily
initialized highlighter.

Framework adapters consume Evin's output; parsing, validation, and route
semantics remain in the Evin CLI action.

The package does not provide ready-made documentation components or a site
theme. Your app renders the page AST and embeds any SVG string from
`diagrams[pageId][diagramId]` inline; the diagram ID then remains available to
your CSS. The Book's site-building guide shows the full Vite setup and a route
lookup example.

## Diagram SVGs

The docs action renders Mermaid, Svgbob, and Pikchr diagrams into generated SVG
files. `everyinit/vite` imports the SVG source and exposes it through
`virtual:evin/docs` as `diagrams[pageId][diagramId]`. Render that string inline
so site CSS can style the SVG:

```js
import docs from "virtual:evin/docs";

const svg = docs.diagrams[page.id]?.[diagram.id];
```

Diagram IDs are authored with `id=...` after the language and optional quoted
title in a Markdown fence, or with the `:id:` option in an RST diagram
directive. The docs build generates a stable ID if none is supplied. IDs must
be unique on a page and use a letter followed by letters, numbers, `_`, or `-`.
Generated SVGs include accessible titles and CSS variables for ink, accent,
surface, highlight, and muted colors. For example, a site stylesheet can
override a selected diagram with `#release-flow { --diagram-ink: #173126; }`.

```js
import { defineConfig } from "vite";
import evinDocs from "everyinit/vite";

export default defineConfig({
  plugins: [
    evinDocs({
      command: ["evin", "run", "docs:build"],
      cwd: "../..",
      watch: ["../../docs"],
    }),
  ],
});
```

```js
import docs, { routes, resolveRoute, counterpart } from "virtual:evin/docs";
```

All public subpaths share the workspace's release version. Integrations are
included in this package and do not require separate `@everyinit/*` installs.
