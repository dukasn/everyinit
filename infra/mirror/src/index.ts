const CHANNELS = new Set(["canary", "latest"]);
const VERSION_PATTERN = /^v\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/;
const TARGET_PATTERN = /^[A-Za-z0-9_]+(?:-[A-Za-z0-9_]+)+$/;

function responseHeaders(extra = {}) {
  return new Headers({
    "Access-Control-Allow-Origin": "*",
    "Access-Control-Allow-Methods": "GET, HEAD, OPTIONS",
    "Access-Control-Allow-Headers": "If-None-Match, If-Modified-Since",
    "Access-Control-Expose-Headers": "ETag, Last-Modified, Content-Length",
    "X-Content-Type-Options": "nosniff",
    ...extra,
  });
}

function objectKey(pathname: string): string | null {
  let parts;
  try {
    parts = pathname.split("/").filter(Boolean).map(decodeURIComponent);
  } catch {
    return null;
  }
  if (parts.length < 2 || parts.length > 3) return null;
  const [binary, second, artifact] = parts;
  if (binary !== "evin" && binary !== "esh") return null;

  if (parts.length === 2 && second.endsWith(".json")) {
    const channel = second.slice(0, -".json".length);
    return CHANNELS.has(channel) ? `${binary}/${channel}.json` : null;
  }

  if (parts.length !== 3 || !VERSION_PATTERN.test(second)) return null;
  if (artifact === "release.json" || artifact === "SHA256SUMS" || artifact === "SHA256SUMS.sig") {
    return `${binary}/${second}/${artifact}`;
  }
  const archivePrefix = `${binary}-${second}-`;
  if (!artifact.startsWith(archivePrefix) || !artifact.endsWith(".tar.gz")) return null;
  const target = artifact.slice(archivePrefix.length, -".tar.gz".length);
  return TARGET_PATTERN.test(target) ? `${binary}/${second}/${artifact}` : null;
}

function contentType(key: string): string {
  if (key.endsWith(".json")) return "application/json; charset=utf-8";
  if (key.endsWith(".tar.gz")) return "application/gzip";
  return "text/plain; charset=utf-8";
}

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const method = request.method.toUpperCase();
    if (method === "OPTIONS") {
      return new Response(null, { status: 204, headers: responseHeaders() });
    }
    if (method !== "GET" && method !== "HEAD") {
      return new Response("Method not allowed\n", {
        status: 405,
        headers: responseHeaders({ Allow: "GET, HEAD, OPTIONS" }),
      });
    }

    const key = objectKey(new URL(request.url).pathname);
    if (key === null) {
      return new Response("Not found\n", { status: 404, headers: responseHeaders() });
    }

    const object = await env.ARTIFACTS.get(key);
    if (object === null) {
      return new Response("Not found\n", { status: 404, headers: responseHeaders() });
    }

    const immutable = /\/v\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?\//.test(`/${key}`);
    const headers = responseHeaders({
      "Cache-Control": immutable
        ? "public, max-age=31536000, immutable"
        : "public, max-age=60, stale-while-revalidate=300",
      "Content-Type": contentType(key),
      ETag: object.httpEtag,
      "Last-Modified": object.uploaded.toUTCString(),
      "Content-Length": String(object.size),
    });
    if (key.endsWith(".tar.gz")) {
      const filename = key.split("/").at(-1);
      headers.set("Content-Disposition", `attachment; filename="${filename}"`);
    }

    const ifNoneMatch = request.headers.get("If-None-Match");
    if (
      ifNoneMatch &&
      ifNoneMatch
        .split(",")
        .map((value) => value.trim())
        .includes(object.httpEtag)
    ) {
      return new Response(null, { status: 304, headers });
    }
    if (method === "HEAD") return new Response(null, { status: 200, headers });
    return new Response(object.body, { status: 200, headers });
  },
} satisfies ExportedHandler<Env>;
