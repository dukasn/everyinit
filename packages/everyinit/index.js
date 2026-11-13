export function normalizeDocsPath(pathname) {
  if (typeof pathname !== "string" || pathname.length === 0) return "/";
  const path = pathname.split(/[?#]/, 1)[0].replace(/\\/g, "/");
  const normalized = path.startsWith("/") ? path : `/${path}`;
  return normalized.length > 1 ? normalized.replace(/\/+$/, "") : normalized;
}

export function createDocsRuntime(manifest, pages, assets = {}) {
  if (manifest?.schema !== "evin.docs.manifest/v1") {
    throw new TypeError(`Unsupported Evin docs manifest schema: ${manifest?.schema}`);
  }
  if (!pages || typeof pages !== "object" || Array.isArray(pages)) {
    throw new TypeError("Evin docs pages must be an object keyed by page ID");
  }

  const routes = manifest.routes.map((route) => {
    const page = pages[route.page_id];
    if (!page || page.id !== route.page_id) {
      throw new TypeError(`Missing or invalid Evin page data for ${route.page_id}`);
    }
    return { ...route, page };
  });
  const byPath = new Map(routes.map((route) => [normalizeDocsPath(route.path), route]));
  const byPageId = new Map(routes.map((route) => [route.page_id, route]));
  const resolveRoute = (pathname) => byPath.get(normalizeDocsPath(pathname)) ?? null;
  const routeForPage = (page) => {
    if (!page) return null;
    return byPageId.get(page.id) ?? routes.find((route) => route.page.route === page.route) ?? null;
  };
  const counterpart = (route, version, locale) => {
    const pageId = route?.counterparts?.[`${version}/${locale}`];
    return pageId ? (pages[pageId] ?? null) : null;
  };

  return {
    manifest,
    routes,
    pages,
    assets,
    resolveRoute,
    counterpart,
    routeForPage,
  };
}

export function createRouteTable(docs, render) {
  return docs.routes.map((route) => ({
    path: route.path,
    element: render(route),
    handle: route,
  }));
}

const SEARCH_MAGIC = "EVIS";
const SEARCH_VERSION = 1;

class SearchReader {
  constructor(input) {
    this.bytes = input instanceof Uint8Array ? input : new Uint8Array(input);
    this.view = new DataView(this.bytes.buffer, this.bytes.byteOffset, this.bytes.byteLength);
    this.offset = 0;
    this.decoder = new TextDecoder("utf-8", { fatal: true });
  }

  take(length) {
    if (!Number.isSafeInteger(length) || length < 0 || this.offset + length > this.bytes.length) {
      throw new TypeError("Truncated Evin search index");
    }
    const start = this.offset;
    this.offset += length;
    return start;
  }

  u8() {
    return this.view.getUint8(this.take(1));
  }

  u16() {
    return this.view.getUint16(this.take(2), true);
  }

  u32() {
    return this.view.getUint32(this.take(4), true);
  }

  varint() {
    let value = 0;
    let shift = 0;
    for (let byteCount = 0; byteCount < 5; byteCount += 1) {
      const byte = this.u8();
      value += (byte & 0x7f) * 2 ** shift;
      if ((byte & 0x80) === 0) return value;
      shift += 7;
    }
    throw new TypeError("Invalid varint in Evin search index");
  }

  string() {
    const length = this.varint();
    const start = this.take(length);
    return this.decoder.decode(this.bytes.subarray(start, start + length));
  }
}

/** Decode an Evin EVIS v1 search artifact into records and compact postings. */
export function decodeSearchIndex(input) {
  const reader = new SearchReader(input);
  if (reader.bytes.length < 20) throw new TypeError("Invalid Evin search index header");
  const magic = String.fromCharCode(...reader.bytes.subarray(0, 4));
  reader.offset = 4;
  if (magic !== SEARCH_MAGIC) throw new TypeError("Invalid Evin search index magic");
  if (reader.u16() !== SEARCH_VERSION) throw new TypeError("Unsupported Evin search index version");
  if (reader.u16() !== 0) throw new TypeError("Unsupported Evin search index flags");
  const recordCount = reader.u32();
  const termCount = reader.u32();
  const stringCount = reader.u32();
  if (
    recordCount > reader.bytes.length ||
    termCount > reader.bytes.length ||
    stringCount > reader.bytes.length
  ) {
    throw new TypeError("Invalid Evin search index counts");
  }
  const strings = new Array(stringCount);
  for (let index = 0; index < stringCount; index += 1) strings[index] = reader.string();
  const stringAt = () => {
    const index = reader.varint();
    if (index >= strings.length)
      throw new TypeError("Invalid string reference in Evin search index");
    return strings[index];
  };

  const records = new Array(recordCount);
  for (let index = 0; index < recordCount; index += 1) {
    records[index] = {
      pageId: stringAt(),
      route: stringAt(),
      pageTitle: stringAt(),
      sectionTitle: stringAt(),
      anchor: stringAt(),
      referenceId: stringAt(),
      snippet: stringAt(),
      kind: reader.u8(),
    };
  }

  const terms = new Map();
  for (let index = 0; index < termCount; index += 1) {
    const term = stringAt();
    if (!term || terms.has(term))
      throw new TypeError("Invalid duplicate term in Evin search index");
    const count = reader.varint();
    if (count > recordCount) throw new TypeError("Invalid posting count in Evin search index");
    const documentIds = new Uint32Array(count);
    const fields = new Uint8Array(count);
    const frequencies = new Uint8Array(count);
    let documentId = 0;
    for (let posting = 0; posting < count; posting += 1) {
      const delta = reader.varint();
      documentId = posting === 0 ? delta : documentId + delta;
      if (documentId >= recordCount || (posting > 0 && delta === 0)) {
        throw new TypeError("Invalid document posting in Evin search index");
      }
      documentIds[posting] = documentId;
      fields[posting] = reader.u8();
      frequencies[posting] = reader.u8();
    }
    terms.set(term, { documentIds, fields, frequencies });
  }
  if (reader.offset !== reader.bytes.length)
    throw new TypeError("Unexpected trailing Evin search index data");
  const termList = [...terms.keys()].sort();
  const trigramMap = new Map();
  for (const term of termList) {
    for (const gram of trigrams(term)) {
      const candidates = trigramMap.get(gram) ?? [];
      candidates.push(term);
      trigramMap.set(gram, candidates);
    }
  }
  return { formatVersion: SEARCH_VERSION, records, terms, termList, trigramMap };
}

function searchTokens(value) {
  return value
    .toLocaleLowerCase()
    .split(/[^\p{L}\p{N}_-]+/u)
    .filter((term) => [...term].length > 1);
}

function editDistanceAtMostOne(left, right) {
  const leftChars = [...left];
  const rightChars = [...right];
  if (Math.abs(leftChars.length - rightChars.length) > 1) return false;
  let i = 0;
  let j = 0;
  let edits = 0;
  while (i < leftChars.length && j < rightChars.length) {
    if (leftChars[i] === rightChars[j]) {
      i += 1;
      j += 1;
    } else {
      if (++edits > 1) return false;
      if (leftChars.length > rightChars.length) i += 1;
      else if (rightChars.length > leftChars.length) j += 1;
      else {
        i += 1;
        j += 1;
      }
    }
  }
  return edits + Number(i < leftChars.length || j < rightChars.length) <= 1;
}

function trigrams(value) {
  const chars = [...`^${value}$`];
  const grams = new Set();
  for (let index = 0; index + 2 < chars.length; index += 1) {
    grams.add(chars.slice(index, index + 3).join(""));
  }
  return grams;
}

function lowerBound(values, target) {
  let low = 0;
  let high = values.length;
  while (low < high) {
    const middle = (low + high) >>> 1;
    if (values[middle] < target) low = middle + 1;
    else high = middle;
  }
  return low;
}

function postingScore(fields, frequency) {
  let score = 0;
  if (fields & 1) score += 120;
  if (fields & 2) score += 90;
  if (fields & 4) score += 72;
  if (fields & 16) score += 52;
  if (fields & 8) score += Math.min(frequency * 5, 30);
  if (fields & 32) score += Math.min(frequency * 2, 12);
  return score;
}

/** Search decoded artifacts; all query terms must match a page or section. */
export function searchDocs(index, query, { limit = 10, allowFuzzy = true } = {}) {
  if (!index?.terms || !Array.isArray(index.records))
    throw new TypeError("Invalid decoded Evin search index");
  const terms = [...new Set(searchTokens(String(query)))];
  if (!terms.length) return [];
  const scores = new Map();
  const matchedTerms = new Map();

  for (const term of terms) {
    const start = lowerBound(index.termList, term);
    const candidates = [];
    for (let at = start; at < index.termList.length; at += 1) {
      const candidate = index.termList[at];
      if (candidate === term || candidate.startsWith(term))
        candidates.push([candidate, candidate === term ? 1 : 0.68]);
      else if (candidate > term && !candidate.startsWith(term)) break;
    }
    if (!candidates.length && allowFuzzy && [...term].length >= 5) {
      const nearby = new Map();
      for (const gram of trigrams(term)) {
        for (const candidate of index.trigramMap.get(gram) ?? []) {
          nearby.set(candidate, (nearby.get(candidate) ?? 0) + 1);
        }
      }
      const likely = [...nearby]
        .sort((left, right) => right[1] - left[1])
        .slice(0, 256)
        .map(([candidate]) => candidate);
      for (const candidate of likely) {
        if (editDistanceAtMostOne(term, candidate)) candidates.push([candidate, 0.42]);
      }
    }
    const perTerm = new Map();
    for (const [candidate, multiplier] of candidates) {
      const posting = index.terms.get(candidate);
      for (let item = 0; item < posting.documentIds.length; item += 1) {
        const documentId = posting.documentIds[item];
        const score = postingScore(posting.fields[item], posting.frequencies[item]) * multiplier;
        perTerm.set(documentId, Math.max(perTerm.get(documentId) ?? 0, score));
      }
    }
    for (const [documentId, score] of perTerm) {
      scores.set(documentId, (scores.get(documentId) ?? 0) + score);
      matchedTerms.set(documentId, (matchedTerms.get(documentId) ?? 0) + 1);
    }
  }

  const normalizedQuery = terms.join(" ");
  const results = [];
  for (const [documentId, score] of scores) {
    if (matchedTerms.get(documentId) !== terms.length) continue;
    const record = index.records[documentId];
    const searchable =
      `${record.pageTitle} ${record.sectionTitle} ${record.route}`.toLocaleLowerCase();
    const phraseBoost = searchable.includes(normalizedQuery) ? 100 : 0;
    const title = record.kind === 1 && record.sectionTitle ? record.sectionTitle : record.pageTitle;
    results.push({
      pageId: record.pageId,
      route: `${record.route}${record.anchor ? `#${record.anchor}` : ""}`,
      pageTitle: record.pageTitle,
      title,
      sectionTitle: record.sectionTitle,
      anchor: record.anchor,
      referenceId: record.referenceId,
      snippet: record.snippet,
      kind: record.kind,
      score: score + phraseBoost,
    });
  }
  return results
    .sort((left, right) => right.score - left.score || left.title.localeCompare(right.title))
    .slice(0, Math.max(1, limit));
}
