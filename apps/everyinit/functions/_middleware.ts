import { documentationRoutePaths } from "../../../dist/docs-data/legacy-root-paths.js";

type PagesContext = {
  request: Request;
  next(request?: Request | string, init?: RequestInit): Promise<Response>;
};

const legacyDocsHosts = new Set(["learn.everyinit.dev", "docs.everyinit.dev"]);
const isAssetPath = (pathname: string) =>
  pathname.startsWith("/_astro/") || /\.[a-z0-9]+$/i.test(pathname);
const withRobotsHeader = (response: Response, hostname: string) => {
  if (hostname === "everyinit.dev") return response;
  const headers = new Headers(response.headers);
  headers.set("X-Robots-Tag", "noindex, nofollow");
  return new Response(response.body, {
    status: response.status,
    statusText: response.statusText,
    headers,
  });
};
const withTrailingSlash = (pathname: string) => {
  const normalized = pathname.replace(/\/+$/, "") || "/";
  return normalized === "/" ? "/" : `${normalized}/`;
};

const canonicalDocsPath = (pathname: string) => {
  const normalized = pathname.replace(/\/+$/, "") || "/";
  if (normalized === "/" || normalized === "/docs") return "/docs/";
  if (normalized === "/install" || normalized === "/docs/install") {
    return "/docs/book/installation/";
  }
  return withTrailingSlash(normalized.startsWith("/docs/") ? normalized : `/docs${normalized}`);
};
const docsRoutePath = (pathname: string) => {
  const normalized = pathname.replace(/\/+$/, "") || "/";
  if (normalized === "/docs") return "/";
  return normalized.startsWith("/docs/") ? normalized.slice("/docs".length) : normalized;
};
const notFound = (hostname: string) =>
  withRobotsHeader(new Response("Not Found\n", { status: 404 }), hostname);

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
    return withRobotsHeader(Response.redirect(canonicalUrl, 308), url.hostname);
  }

  if (isAssetPath(pathname)) return withRobotsHeader(await next(), url.hostname);

  if (normalizedPath === "/install" || normalizedPath === "/docs/install") {
    url.pathname = "/docs/book/installation/";
    return withRobotsHeader(Response.redirect(url, 308), url.hostname);
  }

  if (normalizedPath === "/docs" && pathname !== "/docs/") {
    url.pathname = "/docs/";
    return withRobotsHeader(Response.redirect(url, 308), url.hostname);
  }

  if (pathname.startsWith("/docs/")) {
    if (!documentationRoutePaths.has(docsRoutePath(pathname))) return notFound(url.hostname);
    const canonicalPath = withTrailingSlash(pathname);
    if (canonicalPath !== pathname) {
      url.pathname = canonicalPath;
      return withRobotsHeader(Response.redirect(url, 308), url.hostname);
    }
    return withRobotsHeader(await next(), url.hostname);
  }

  if (pathname === "/") return withRobotsHeader(await next(), url.hostname);

  const canonicalPath = canonicalDocsPath(pathname);
  if (!documentationRoutePaths.has(docsRoutePath(canonicalPath))) return notFound(url.hostname);
  url.pathname = canonicalPath;
  const redirectUrl = new URL(request.url);
  redirectUrl.pathname = canonicalPath;
  return withRobotsHeader(Response.redirect(redirectUrl, 308), url.hostname);
};
