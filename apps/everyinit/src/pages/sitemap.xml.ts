import type { APIRoute } from "astro";
import docs from "virtual:evin/docs";
import { availableLocales } from "../i18n";

const escapeXml = (value: string) =>
  value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&apos;");
const docsUrl = (path: string) =>
  `https://everyinit.dev${path === "/" ? "/docs/" : `/docs${path.replace(/\/+$/, "")}/`}`;

export const GET: APIRoute = () => {
  const homeUrls = availableLocales.map((locale) =>
    locale === "en" ? "https://everyinit.dev/" : `https://everyinit.dev/${locale}/`,
  );
  const docsUrls = docs.routes
    .filter((route) => route.version === docs.manifest.latest_version && !route.is_fallback)
    .map((route) => docsUrl(route.path));
  const locations = [...new Set([...homeUrls, ...docsUrls])];
  const body = [
    '<?xml version="1.0" encoding="UTF-8"?>',
    '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">',
    ...locations.map((url) => `  <url><loc>${escapeXml(url)}</loc></url>`),
    "</urlset>",
  ].join("\n");

  return new Response(body, {
    headers: { "Content-Type": "application/xml; charset=utf-8" },
  });
};
