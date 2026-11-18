/// <reference path="./virtual.d.ts" />
import type { Plugin } from "vite";
import type { EvinDocsRouteData } from "./index.js";

export interface EvinDocsViteOptions {
  /** Manifest path, relative to `cwd`. Defaults to dist/docs-data/manifest.json. */
  manifest?: string;
  /** Command and arguments used to regenerate the Evin data package. */
  command?: string[];
  /** Working directory for the Evin command. Defaults to the Vite project root. */
  cwd?: string;
  /** Source directories to watch. Defaults to ["docs"]. */
  watch?: string[];
}

/**
 * Load Evin's manifest and page files as `virtual:evin/docs` and rebuild them
 * when watched source files change.
 */
export default function evinDocs(options?: EvinDocsViteOptions): Plugin;
