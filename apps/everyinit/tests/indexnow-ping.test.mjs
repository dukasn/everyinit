import assert from "node:assert/strict";
import test from "node:test";

import { readFile } from "node:fs/promises";
import { indexNowKey, parseSitemapUrls, pingIndexNow } from "../scripts/indexnow-ping.mjs";

test("IndexNow sitemap parsing decodes canonical XML URL entries", () => {
  const xml = [
    '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">',
    "<url><loc>https://everyinit.dev/</loc></url>",
    "<url><loc>https://everyinit.dev/docs/search/?q=a&amp;b</loc></url>",
    "</urlset>",
  ].join("");

  assert.deepEqual(parseSitemapUrls(xml), [
    "https://everyinit.dev/",
    "https://everyinit.dev/docs/search/?q=a&b",
  ]);
});

test("IndexNow sitemap parsing returns an empty list for an empty sitemap", () => {
  assert.deepEqual(parseSitemapUrls('<urlset xmlns="sitemap" />'), []);
});

test("the public IndexNow verification file contains the configured key", async () => {
  const keyFile = new URL(`../public/${indexNowKey}.txt`, import.meta.url);
  assert.equal((await readFile(keyFile, "utf8")).trim(), indexNowKey);
});

test("IndexNow submits the canonical URLs from the production sitemap", async () => {
  const requests = [];
  const fetcher = async (url, options) => {
    requests.push({ url, options });
    if (url === "https://everyinit.dev/sitemap.xml") {
      return new Response(
        "<urlset><url><loc>https://everyinit.dev/</loc></url><url><loc>https://everyinit.dev/docs/</loc></url></urlset>",
        { status: 200 },
      );
    }
    return new Response(null, { status: 202 });
  };

  await pingIndexNow(fetcher);

  assert.equal(requests.length, 2);
  assert.equal(requests[1].url, "https://api.indexnow.org/indexnow");
  assert.equal(requests[1].options.method, "POST");
  assert.deepEqual(JSON.parse(requests[1].options.body), {
    host: "everyinit.dev",
    key: indexNowKey,
    urlList: ["https://everyinit.dev/", "https://everyinit.dev/docs/"],
  });
});
