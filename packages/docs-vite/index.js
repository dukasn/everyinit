import { spawn } from "node:child_process";
import { readFile } from "node:fs/promises";
import path from "node:path";

const publicId = "virtual:evin/docs";
const resolvedId = `\0${publicId}`;

function run(command, cwd) {
  return new Promise((resolve, reject) => {
    const [bin, ...args] = command;
    const child = spawn(bin, args, { cwd, stdio: "inherit", shell: process.platform === "win32" });
    child.once("error", reject);
    child.once("exit", (code, signal) => {
      if (code === 0) resolve();
      else reject(new Error(`Evin docs command failed (${signal ?? code}): ${command.join(" ")}`));
    });
  });
}

function viteFilePath(file) {
  const absolute = path.resolve(file).replaceAll("\\", "/");
  return `/@fs/${absolute.replace(/^\/+/, "")}`;
}

export default function evinDocs(options = {}) {
  const command = options.command ?? ["evin", "run", "docs-data"];
  const watch = options.watch ?? ["docs"];
  let projectRoot;
  let workingDirectory;
  let manifestPath;
  let sourceRoots = [];
  let server;
  let building;
  let moduleCode;

  async function buildData(force = false) {
    if (moduleCode && !force) return moduleCode;
    if (building) return building;
    building = (async () => {
      await run(command, workingDirectory);
      const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
      if (manifest.schema !== "evin.docs.manifest/v1") {
        throw new Error(`Unsupported Evin docs manifest schema: ${manifest.schema}`);
      }
      const outputRoot = path.dirname(manifestPath);
      const assetsPath = path.resolve(outputRoot, manifest.assets);
      const assets = JSON.parse(await readFile(assetsPath, "utf8"));
      const pageEntries = new Map();
      for (const route of manifest.routes) pageEntries.set(route.page_id, route.page_file);
      const imports = [];
      const pairs = [];
      let index = 0;
      for (const [id, relativeFile] of [...pageEntries].sort(([a], [b]) => a.localeCompare(b))) {
        const pagePath = path.resolve(outputRoot, relativeFile);
        const page = JSON.parse(await readFile(pagePath, "utf8"));
        if (page.schema !== "evin.docs.page/v1" || page.id !== id) {
          throw new Error(`Invalid Evin page data for ${id}: ${relativeFile}`);
        }
        const importName = `page${index++}`;
        imports.push(`import ${importName} from ${JSON.stringify(viteFilePath(pagePath))};`);
        pairs.push(`${JSON.stringify(id)}: ${importName}`);
      }
      moduleCode = [
        `import manifest from ${JSON.stringify(viteFilePath(manifestPath))};`,
        ...imports,
        `const assets = ${JSON.stringify(assets)};`,
        `const pages = { ${pairs.join(", ")} };`,
        `const pageRoutes = manifest.routes.map(route => ({ ...route, page: pages[route.page_id] }));`,
        `const byPath = new Map(pageRoutes.map(route => [normalizePath(route.path), route]));`,
        `function normalizePath(path) { const value = path.length > 1 ? path.replace(/\\/+$/, "") : path; return value || "/"; }`,
        `export { manifest, pageRoutes as routes, pages, assets };`,
        `export function resolveRoute(path) { return byPath.get(normalizePath(path)) ?? null; }`,
        `export function counterpart(route, version, locale) { const id = route.counterparts[version + "/" + locale]; return id ? pages[id] ?? null : null; }`,
        `export default { manifest, routes: pageRoutes, pages, assets, resolveRoute, counterpart };`,
      ].join("\n");
      sourceRoots = watch.map((entry) => path.resolve(workingDirectory, entry));
      return moduleCode;
    })().finally(() => { building = undefined; });
    return building;
  }

  return {
    name: "@everyinit/docs-vite",
    enforce: "pre",
    configResolved(config) {
      projectRoot = config.root;
      workingDirectory = path.resolve(projectRoot, options.cwd ?? ".");
      manifestPath = path.resolve(workingDirectory, options.manifest ?? "dist/docs-data/manifest.json");
    },
    async buildStart() {
      await buildData(true);
    },
    resolveId(id) {
      if (id === publicId) return resolvedId;
    },
    async load(id) {
      if (id === resolvedId) return buildData();
    },
    configureServer(devServer) {
      server = devServer;
      sourceRoots = watch.map((entry) => path.resolve(workingDirectory, entry));
      devServer.watcher.add(sourceRoots);
    },
    async handleHotUpdate(context) {
      if (!sourceRoots.some((root) => context.file === root || context.file.startsWith(`${root}${path.sep}`))) return;
      await buildData(true);
      const module = server?.moduleGraph.getModuleById(resolvedId);
      if (module && server) {
        server.moduleGraph.invalidateModule(module);
        return [module];
      }
    },
  };
}

export function createRouteTable(docs, render) {
  return docs.routes.map((route) => ({
    path: route.path,
    element: render(route),
    handle: route,
  }));
}
