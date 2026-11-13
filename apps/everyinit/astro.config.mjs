import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "astro/config";
import evinDocs from "everyinit/vite";

const appRoot = fileURLToPath(new URL(".", import.meta.url));
const repositoryRoot = resolve(appRoot, "../..");
const docsCommand = process.env.EVIN_BIN
  ? [process.env.EVIN_BIN, "run", "docs:build"]
  : ["cargo", "run", "--locked", "-p", "evin", "--", "run", "docs:build"];

export default defineConfig({
  site: "https://everyinit.dev",
  output: "static",
  vite: {
    server: { allowedHosts: ["docs.everyinit.dev", "everyinit.dev"] },
    preview: { allowedHosts: ["docs.everyinit.dev", "everyinit.dev"] },
    plugins: [
      tailwindcss(),
      evinDocs({
        command: process.env.EVIN_DOCS_PREBUILT === "1" ? [] : docsCommand,
        cwd: repositoryRoot,
        manifest: "dist/docs-data/manifest.json",
        watch: [resolve(repositoryRoot, "docs")],
      }),
    ],
  },
});
