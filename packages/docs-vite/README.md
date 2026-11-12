# `@everyinit/docs-vite`

Vite plugin for Evin's headless documentation data package. The Rust action
owns parsing, validation, localization, versioning, and route calculation;
this package loads that output into Vite and watches documentation sources.

```js
import { defineConfig } from "vite";
import evinDocs from "@everyinit/docs-vite";

export default defineConfig({
  plugins: [evinDocs({ command: ["evin", "run", "docs-data"] })],
});
```

The configured Evin task must write the manifest path selected by `manifest`
(default `dist/docs-data/manifest.json`). Import the route and page data from
the virtual module:

```js
import docs, { routes, assets, resolveRoute, counterpart } from "virtual:evin/docs";
```

`routes` contains manifest route records with their page payload attached.
`resolveRoute(path)` resolves a URL path, and `counterpart(route, version,
locale)` finds the equivalent page variant. Use `createRouteTable` to map
route records into a router's route shape; the application supplies the page
component and all presentation. The `assets` export maps page IDs and source
URLs to generated media paths.

When a watched source file changes, the plugin runs the configured Evin
command and invalidates the virtual module. Options are `manifest`, `command`,
`cwd`, and `watch`. The default command is `evin run docs-data`; the default
watch directory is `docs`.
