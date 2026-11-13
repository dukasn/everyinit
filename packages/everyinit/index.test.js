import assert from "node:assert/strict";
import test from "node:test";

import { createDocsRuntime, decodeSearchIndex, normalizeDocsPath, searchDocs } from "./index.js";

const page = (id, route) => ({
  schema: "evin.docs.page/v1",
  id,
  route,
});

const manifest = {
  schema: "evin.docs.manifest/v1",
  routes: [
    {
      path: "/",
      page_id: "home-en",
      counterparts: { "v1/en": "home-en", "v1/fr": "home-fr" },
    },
    {
      path: "/fr",
      page_id: "home-fr",
      counterparts: { "v1/en": "home-en", "v1/fr": "home-fr" },
    },
  ],
};

const pages = {
  "home-en": page("home-en", "/"),
  "home-fr": page("home-fr", "/fr"),
};

test("normalizes route paths without changing root", () => {
  assert.equal(normalizeDocsPath("guide/intro///"), "/guide/intro");
  assert.equal(normalizeDocsPath("/"), "/");
  assert.equal(normalizeDocsPath(""), "/");
});

test("resolves routes and counterpart variants from one manifest", () => {
  const docs = createDocsRuntime(manifest, pages);
  assert.equal(docs.routes.length, 2);
  assert.equal(docs.resolveRoute("/"), docs.routes[0]);
  assert.equal(docs.resolveRoute("/fr/"), docs.routes[1]);
  assert.equal(docs.resolveRoute("/missing"), null);
  assert.equal(docs.counterpart(docs.routes[0], "v1", "fr"), pages["home-fr"]);
  assert.equal(docs.routeForPage(pages["home-fr"]), docs.routes[1]);
});

test("rejects incompatible generated data", () => {
  assert.throws(() => createDocsRuntime({ schema: "unknown" }, {}), /Unsupported/);
  assert.throws(() => createDocsRuntime(manifest, {}), /Missing or invalid/);
});

function writeVarint(bytes, value) {
  while (value >= 0x80) {
    bytes.push((value & 0x7f) | 0x80);
    value >>>= 7;
  }
  bytes.push(value);
}

function writeString(bytes, value) {
  const encoded = new TextEncoder().encode(value);
  writeVarint(bytes, encoded.length);
  bytes.push(...encoded);
}

function searchFixture() {
  const bytes = [...new TextEncoder().encode("EVIS"), 1, 0, 0, 0];
  const records = [
    [
      "page-install",
      "/book/install",
      "Installation & Quickstart",
      "Installation & Quickstart",
      "",
      "",
      "Install Evin and get started.",
      0,
    ],
    [
      "page-install",
      "/book/install",
      "Installation & Quickstart",
      "Install with cargo",
      "install-with-cargo",
      "REF-1200",
      "Run cargo install evin to install the CLI.",
      1,
    ],
  ];
  const terms = [
    ["cargo", [[1, 8, 1]]],
    [
      "evin",
      [
        [0, 8, 1],
        [1, 8, 1],
      ],
    ],
    [
      "install",
      [
        [0, 2, 1],
        [1, 4, 2],
      ],
    ],
    ["installation", [[0, 2, 1]]],
    ["quickstart", [[0, 2, 1]]],
  ];
  const strings = [
    ...new Set([
      ...records.flatMap((record) => record.slice(0, 7)),
      ...terms.map(([term]) => term),
    ]),
  ].sort();
  const stringId = new Map(strings.map((value, index) => [value, index]));
  const view = new DataView(new ArrayBuffer(12));
  view.setUint32(0, records.length, true);
  view.setUint32(4, terms.length, true);
  view.setUint32(8, strings.length, true);
  bytes.push(...new Uint8Array(view.buffer));
  for (const value of strings) writeString(bytes, value);
  for (const record of records) {
    for (const field of record.slice(0, 7)) writeVarint(bytes, stringId.get(field));
    bytes.push(record[7]);
  }
  for (const [term, postings] of terms) {
    writeVarint(bytes, stringId.get(term));
    writeVarint(bytes, postings.length);
    let previous = 0;
    postings.forEach(([documentId, fields, frequency], index) => {
      writeVarint(bytes, index === 0 ? documentId : documentId - previous);
      bytes.push(fields, frequency);
      previous = documentId;
    });
  }
  return new Uint8Array(bytes);
}

test("decodes binary postings and searches page text, sections, prefixes, and typos", () => {
  const index = decodeSearchIndex(searchFixture());
  const result = searchDocs(index, "cargo install");
  assert.equal(result.length, 1);
  assert.equal(result[0].title, "Install with cargo");
  assert.equal(result[0].route, "/book/install#install-with-cargo");
  assert.equal(result[0].referenceId, "REF-1200");
  assert.match(result[0].snippet, /cargo install evin/);
  assert.equal(searchDocs(index, "instalation")[0].pageId, "page-install");
  assert.equal(searchDocs(index, "quick")[0].route, "/book/install");
  assert.deepEqual(searchDocs(index, ""), []);
});

test("rejects unsupported, truncated, and corrupt search indexes", () => {
  assert.throws(() => decodeSearchIndex(new Uint8Array(19)), /header/);
  const unsupported = searchFixture();
  unsupported[4] = 2;
  assert.throws(() => decodeSearchIndex(unsupported), /Unsupported/);
  assert.throws(() => decodeSearchIndex(searchFixture().subarray(0, 24)), /Truncated/);
});
