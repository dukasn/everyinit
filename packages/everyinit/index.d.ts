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
  search_indexes: EvinDocsSearchArtifact[];
  diagnostics: Array<{
    severity: string;
    code: string;
    message: string;
    version: string;
    locale: string;
    route: string;
  }>;
}

export interface EvinDocsSearchArtifact {
  version: string;
  locale: string;
  path: string;
  byte_length: number;
  sha256: string;
}

export interface EvinDocsSearchRecord {
  pageId: string;
  route: string;
  pageTitle: string;
  title: string;
  sectionTitle: string;
  anchor: string;
  referenceId: string;
  snippet: string;
  kind: number;
  score: number;
}

export interface EvinDocsSearchIndex {
  formatVersion: number;
  records: Array<{
    pageId: string;
    route: string;
    pageTitle: string;
    sectionTitle: string;
    anchor: string;
    referenceId: string;
    snippet: string;
    kind: number;
  }>;
  terms: Map<string, { documentIds: Uint32Array; fields: Uint8Array; frequencies: Uint8Array }>;
  termList: string[];
  version?: string;
  locale?: string;
}

export function decodeSearchIndex(input: ArrayBuffer | Uint8Array): EvinDocsSearchIndex;
export function searchDocs(
  index: EvinDocsSearchIndex,
  query: string,
  options?: { limit?: number; allowFuzzy?: boolean },
): EvinDocsSearchRecord[];

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
  page: {
    slug: string;
    source_path: string;
    format: "markdown" | "re_structured_text";
    metadata: Record<string, unknown>;
    reference: unknown;
    toc: Array<{ level: number; text: string; anchor: string }>;
    ast: unknown[];
  };
}

export interface EvinDocsRouteData extends EvinDocsRoute {
  page: EvinDocsPage;
}

export interface EvinDocsRuntime {
  manifest: EvinDocsManifest;
  routes: EvinDocsRouteData[];
  pages: Record<string, EvinDocsPage>;
  assets: Record<string, unknown>;
  resolveRoute(pathname: string): EvinDocsRouteData | null;
  counterpart(route: EvinDocsRoute, version: string, locale: string): EvinDocsPage | null;
  routeForPage(page: EvinDocsPage | null | undefined): EvinDocsRouteData | null;
}

export function normalizeDocsPath(pathname: string): string;
export function createDocsRuntime(
  manifest: EvinDocsManifest,
  pages: Record<string, EvinDocsPage>,
  assets?: Record<string, unknown>,
): EvinDocsRuntime;
export function createRouteTable<T>(
  docs: Pick<EvinDocsRuntime, "routes">,
  render: (route: EvinDocsRouteData) => T,
): Array<{ path: string; element: T; handle: EvinDocsRouteData }>;
