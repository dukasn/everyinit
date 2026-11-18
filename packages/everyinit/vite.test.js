import assert from "node:assert/strict";
import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import evinDocs from "./vite.js";

test("Vite virtual module imports page and asset artifacts from the generated package", async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), "everyinit-vite-"));
  const output = path.join(root, "dist", "docs-data");
  await mkdir(path.join(output, "pages"), { recursive: true });
  await mkdir(path.join(output, "assets", "v1-en"), { recursive: true });
  await mkdir(path.join(output, "search", "v1"), { recursive: true });
  await writeFile(path.join(output, "assets", "v1-en", "logo.png"), "image");
  await writeFile(path.join(output, "search", "v1", "en.bin"), "binary-index");
  const manifest = {
    schema: "evin.docs.manifest/v1",
    assets: "assets.json",
    search_indexes: [
      {
        version: "v1",
        locale: "en",
        path: "search/v1/en.bin",
        byte_length: 12,
        sha256: "a".repeat(64),
      },
    ],
    routes: [{ path: "/", page_id: "page-home", page_file: "pages/home.json" }],
  };
  await writeFile(path.join(output, "manifest.json"), JSON.stringify(manifest));
  await writeFile(
    path.join(output, "pages", "home.json"),
    JSON.stringify({
      schema: "evin.docs.page/v1",
      id: "page-home",
      route: "/",
    }),
  );
  await writeFile(
    path.join(output, "assets.json"),
    JSON.stringify({
      "page-home": { "/logo.png": "assets/v1-en/logo.png" },
    }),
  );

  try {
    const plugin = evinDocs({
      command: [],
      manifest: "dist/docs-data/manifest.json",
      watch: [],
    });
    plugin.configResolved({ root });
    await plugin.buildStart();
    const resolvedId = plugin.resolveId("virtual:evin/docs");
    const code = await plugin.load(resolvedId);
    const searchId = plugin.resolveId("virtual:evin/search");
    const searchCode = await plugin.load(searchId);

    assert.match(code, /createDocsRuntime\(manifest, pages, assets\)/);
    assert.match(code, /\/logo\.png": asset1/);
    assert.match(code, /assets\/v1-en\/logo\.png/);
    assert.match(searchCode, /loadSearchIndex\(version, locale\)/);
    assert.match(searchCode, /decodeSearchIndex\(bytes\)/);
    assert.match(searchCode, /search\/v1\/en\.bin\?url/);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("Vite adapter rejects assets that escape the generated package", async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), "everyinit-vite-"));
  const output = path.join(root, "dist", "docs-data");
  await mkdir(path.join(output, "pages"), { recursive: true });
  await writeFile(
    path.join(output, "manifest.json"),
    JSON.stringify({
      schema: "evin.docs.manifest/v1",
      assets: "assets.json",
      search_indexes: [],
      routes: [{ path: "/", page_id: "page-home", page_file: "pages/home.json" }],
    }),
  );
  await writeFile(
    path.join(output, "pages", "home.json"),
    JSON.stringify({
      schema: "evin.docs.page/v1",
      id: "page-home",
    }),
  );
  await writeFile(
    path.join(output, "assets.json"),
    JSON.stringify({
      "page-home": { "/unsafe.png": "../../outside.png" },
    }),
  );

  try {
    const plugin = evinDocs({ command: [], manifest: "dist/docs-data/manifest.json", watch: [] });
    plugin.configResolved({ root });
    await assert.rejects(plugin.buildStart(), /Invalid Evin asset path/);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("Vite virtual module imports generated diagram SVG as source", async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), "everyinit-vite-"));
  const output = path.join(root, "dist", "docs-data");
  await mkdir(path.join(output, "pages"), { recursive: true });
  await mkdir(path.join(output, "diagrams", "page-home"), { recursive: true });
  await writeFile(
    path.join(output, "diagrams", "page-home", "overview.svg"),
    '<svg id="overview"></svg>',
  );
  await writeFile(
    path.join(output, "manifest.json"),
    JSON.stringify({
      schema: "evin.docs.manifest/v1",
      assets: "assets.json",
      search_indexes: [],
      routes: [{ path: "/", page_id: "page-home", page_file: "pages/home.json" }],
    }),
  );
  await writeFile(
    path.join(output, "pages", "home.json"),
    JSON.stringify({
      schema: "evin.docs.page/v1",
      id: "page-home",
      page: {
        ast: [{ type: "Diagram", id: "overview", svg_path: "diagrams/page-home/overview.svg" }],
      },
    }),
  );
  await writeFile(path.join(output, "assets.json"), JSON.stringify({}));

  try {
    const plugin = evinDocs({ command: [], manifest: "dist/docs-data/manifest.json", watch: [] });
    plugin.configResolved({ root });
    await plugin.buildStart();
    const code = await plugin.load(plugin.resolveId("virtual:evin/docs"));
    assert.match(code, /overview\.svg\?raw/);
    assert.match(code, /const diagrams = \{ "page-home": \{ "overview": diagram0 \} \}/);
    assert.match(code, /createDocsRuntime\(manifest, pages, assets\), diagrams/);
    assert.match(code, /export \{ manifest, pages, assets, diagrams \}/);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("Vite adapter rejects search artifacts that escape the generated package", async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), "everyinit-vite-"));
  const output = path.join(root, "dist", "docs-data");
  await mkdir(path.join(output, "pages"), { recursive: true });
  await writeFile(
    path.join(output, "manifest.json"),
    JSON.stringify({
      schema: "evin.docs.manifest/v1",
      assets: "assets.json",
      search_indexes: [
        {
          version: "v1",
          locale: "en",
          path: "../../outside.bin",
          byte_length: 1,
          sha256: "a".repeat(64),
        },
      ],
      routes: [{ path: "/", page_id: "page-home", page_file: "pages/home.json" }],
    }),
  );
  await writeFile(
    path.join(output, "pages", "home.json"),
    JSON.stringify({ schema: "evin.docs.page/v1", id: "page-home" }),
  );
  await writeFile(path.join(output, "assets.json"), JSON.stringify({}));

  try {
    const plugin = evinDocs({ command: [], manifest: "dist/docs-data/manifest.json", watch: [] });
    plugin.configResolved({ root });
    await assert.rejects(plugin.buildStart(), /Invalid Evin search artifact path/);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
