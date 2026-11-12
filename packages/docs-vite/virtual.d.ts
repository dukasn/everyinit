declare module "virtual:evin/docs" {
  import type {
    EvinDocsManifest,
    EvinDocsPage,
    EvinDocsRouteData,
  } from "@everyinit/docs-vite";

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

  const docs: {
    manifest: EvinDocsManifest;
    routes: EvinDocsRouteData[];
    pages: Record<string, EvinDocsPage>;
    assets: Record<string, Record<string, string>>;
    resolveRoute: typeof resolveRoute;
    counterpart: typeof counterpart;
  };
  export default docs;
}
