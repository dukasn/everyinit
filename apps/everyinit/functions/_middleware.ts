type PagesContext = {
  request: Request;
  next(request?: Request | string, init?: RequestInit): Promise<Response>;
};

const legacyDocsHosts = new Set(["learn.everyinit.dev", "docs.everyinit.dev"]);
const isAssetPath = (pathname: string) =>
  pathname.startsWith("/_astro/") || /\.[a-z0-9]+$/i.test(pathname);

const canonicalDocsPath = (pathname: string) => {
  const normalized = pathname.replace(/\/+$/, "") || "/";
  if (normalized === "/" || normalized === "/docs") return "/docs/";
  if (normalized === "/install" || normalized === "/docs/install") {
    return "/docs/book/installation/";
  }
  return normalized.startsWith("/docs/") ? normalized : `/docs${normalized}`;
};

export const onRequest = async ({ request, next }: PagesContext): Promise<Response> => {
  const url = new URL(request.url);
  const pathname = url.pathname;
  const normalizedPath = pathname.replace(/\/+$/, "") || "/";

  if (legacyDocsHosts.has(url.hostname)) {
    const canonicalUrl = new URL(url);
    canonicalUrl.protocol = "https:";
    canonicalUrl.hostname = "everyinit.dev";
    canonicalUrl.port = "";
    canonicalUrl.pathname = isAssetPath(pathname) ? pathname : canonicalDocsPath(pathname);
    return Response.redirect(canonicalUrl, 308);
  }

  if (isAssetPath(pathname)) return next();

  if (normalizedPath === "/install" || normalizedPath === "/docs/install") {
    url.pathname = "/docs/book/installation/";
    return Response.redirect(url, 308);
  }

  if (pathname === "/" || normalizedPath === "/docs" || pathname.startsWith("/docs/")) {
    return next();
  }

  // Preserve old root-level documentation links while serving the canonical
  // pages from the single /docs route tree.
  url.pathname = `/docs${pathname}`;
  return next(new Request(url, request));
};
