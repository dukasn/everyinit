import { pathToFileURL } from "node:url";

const host = "everyinit.dev";
const sitemapUrl = `https://${host}/sitemap.xml`;
export const indexNowKey = "ad47ebd7f547c76463e71185f52f737f";

const xmlEntities = {
  "&amp;": "&",
  "&lt;": "<",
  "&gt;": ">",
  "&quot;": '"',
  "&apos;": "'",
};

export function parseSitemapUrls(xml) {
  return [...xml.matchAll(/<loc>(.*?)<\/loc>/gs)].map(([, value]) =>
    value.replace(/&(?:amp|lt|gt|quot|apos);/g, (entity) => xmlEntities[entity]),
  );
}

export async function pingIndexNow(fetcher = fetch) {
  const sitemapResponse = await fetcher(sitemapUrl);
  if (!sitemapResponse.ok) {
    throw new Error(`Could not fetch the production sitemap: HTTP ${sitemapResponse.status}`);
  }

  const urls = parseSitemapUrls(await sitemapResponse.text());
  if (urls.length === 0) throw new Error("The production sitemap contains no URLs");
  if (urls.length > 10_000) throw new Error("The production sitemap exceeds IndexNow's URL limit");
  if (urls.some((url) => new URL(url).hostname !== host || !url.startsWith("https://"))) {
    throw new Error("The production sitemap contains a URL outside the canonical HTTPS host");
  }

  const response = await fetcher("https://api.indexnow.org/indexnow", {
    method: "POST",
    headers: { "Content-Type": "application/json; charset=utf-8" },
    body: JSON.stringify({ host, key: indexNowKey, urlList: urls }),
  });
  if (response.status !== 200 && response.status !== 202) {
    throw new Error(`IndexNow rejected the sitemap URLs: HTTP ${response.status}`);
  }

  console.log(`Submitted ${urls.length} canonical URLs to IndexNow (HTTP ${response.status})`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  pingIndexNow().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  });
}
