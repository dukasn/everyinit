declare module "virtual:evin/docs" {
  import type { EvinDocsManifest, EvinDocsPage, EvinDocsRouteData } from "everyinit";

  export const manifest: EvinDocsManifest;
  export const routes: EvinDocsRouteData[];
  export const pages: Record<string, EvinDocsPage>;
  export const assets: Record<string, Record<string, string>>;
  export function resolveRoute(path: string): EvinDocsRouteData | null;
  export function counterpart(
    route: EvinDocsRouteData,
    version: string,
    locale: string,
  ): EvinDocsPage | null;
  export function routeForPage(page: EvinDocsPage | null | undefined): EvinDocsRouteData | null;

  const docs: {
    manifest: EvinDocsManifest;
    routes: EvinDocsRouteData[];
    pages: Record<string, EvinDocsPage>;
    assets: Record<string, Record<string, string>>;
    resolveRoute: typeof resolveRoute;
    counterpart: typeof counterpart;
    routeForPage: typeof routeForPage;
  };
  export default docs;
}

declare module "virtual:evin/search" {
  import type { EvinDocsSearchIndex, EvinDocsSearchRecord } from "everyinit";

  export function loadSearchIndex(version: string, locale: string): Promise<EvinDocsSearchIndex>;
  export function searchDocs(
    index: EvinDocsSearchIndex,
    query: string,
    options?: { limit?: number; allowFuzzy?: boolean },
  ): EvinDocsSearchRecord[];

  const search: { loadSearchIndex: typeof loadSearchIndex; searchDocs: typeof searchDocs };
  export default search;
}
