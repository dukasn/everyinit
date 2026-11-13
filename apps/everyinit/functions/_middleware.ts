type PagesContext = {
  request: Request;
  next(request?: Request | string, init?: RequestInit): Promise<Response>;
};

export const onRequest = async ({ request, next }: PagesContext): Promise<Response> => {
  const url = new URL(request.url);
  const pathname = url.pathname;
  const isAsset = pathname.startsWith("/_astro/") || /\.[a-z0-9]+$/i.test(pathname);

  if (isAsset) return next();

  if (url.hostname === "docs.everyinit.dev") {
    if (pathname === "/") url.pathname = "/docs/";
    else if (pathname === "/docs") url.pathname = "/docs/";
    else if (!pathname.startsWith("/docs/")) url.pathname = `/docs${pathname}`;
    return next(new Request(url, request));
  }

  if (pathname === "/" || pathname === "/docs" || pathname.startsWith("/docs/")) return next();

  // Evin's generated links are rooted at the documentation host. Re-map them
  // to the app's static /docs tree for local preview and same-host previews.
  url.pathname = `/docs${pathname}`;
  return next(new Request(url, request));
};
