import installerScript from "./installer.sh";

const SCRIPT_CACHE = "public, max-age=300";
const INSTALLER_PATHS = new Set(["/", "/evin", "/esh", "/all"]);

export default {
  async fetch(request: Request): Promise<Response> {
    if (request.method !== "GET" && request.method !== "HEAD") {
      return new Response("Method not allowed\n", {
        status: 405,
        headers: { Allow: "GET, HEAD", "Cache-Control": "no-store" },
      });
    }

    const path = new URL(request.url).pathname.replace(/\/+$/, "") || "/";
    if (!INSTALLER_PATHS.has(path)) {
      return new Response("Not found\n", {
        status: 404,
        headers: { "Cache-Control": "no-store" },
      });
    }

    return new Response(request.method === "HEAD" ? null : installerScript, {
      status: 200,
      headers: {
        "Cache-Control": SCRIPT_CACHE,
        "Content-Type": "text/x-shellscript; charset=utf-8",
        "X-Content-Type-Options": "nosniff",
      },
    });
  },
} satisfies ExportedHandler<Env>;
