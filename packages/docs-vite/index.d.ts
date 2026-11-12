import type { Plugin } from "vite";
/// <reference path="./virtual.d.ts" />

export interface EvinDocsRoute {
  path: string;
  page_id: string;
  page_file: string;
  version: string;
  locale: string;
  content_locale: string;
  is_fallback: boolean;
  navigation_order: number | null;
  counterparts: Record<string, string | null>;
}

export interface EvinDocsManifest {
  schema: "evin.docs.manifest/v1";
  format_version: 1;
  latest_version: string;
  default_locale: string;
  versions: Array<{ id: string; latest: boolean }>;
  locales: string[];
  routes: EvinDocsRoute[];
  assets: string;
  diagnostics: Array<{
    severity: string;
    code: string;
    message: string;
    version: string;
    locale: string;
    route: string;
  }>;
}

export interface EvinDocsPage {
  schema: "evin.docs.page/v1";
  id: string;
  version: string;
  locale: string;
  content_locale: string;
  source_path: string;
  route: string;
  is_fallback: boolean;
  navigation_order: number | null;
  page: unknown;
}

export interface EvinDocsRouteData extends EvinDocsRoute {
  page: EvinDocsPage;
}

export interface EvinDocsViteOptions {
  /** Manifest path, relative to the Vite project root. Defaults to dist/docs-data/manifest.json. */
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

export function createRouteTable<T>(
  docs: { routes: EvinDocsRouteData[] },
  render: (route: EvinDocsRouteData) => T,
): Array<{ path: string; element: T; handle: EvinDocsRouteData }>;
