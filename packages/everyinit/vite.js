import { spawn } from "node:child_process";
import { readFile } from "node:fs/promises";
import path from "node:path";

const publicId = "virtual:evin/docs";
const resolvedId = `\0${publicId}`;
const searchPublicId = "virtual:evin/search";
const searchResolvedId = `\0${searchPublicId}`;

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
  const command = options.command ?? ["evin", "run", "docs:build"];
  const watch = options.watch ?? ["docs"];
  let projectRoot;
  let workingDirectory;
  let manifestPath;
  let sourceRoots = [];
  let server;
  let building;
  let moduleCode;
  let searchModuleCode;

  async function buildData(force = false) {
    if (moduleCode && !force) return moduleCode;
    if (building) return building;
    building = (async () => {
      if (command.length > 0) await run(command, workingDirectory);
      const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
      if (manifest.schema !== "evin.docs.manifest/v1") {
        throw new Error(`Unsupported Evin docs manifest schema: ${manifest.schema}`);
      }
      const outputRoot = path.dirname(manifestPath);
      const assetsPath = path.resolve(outputRoot, manifest.assets);
      const assets = JSON.parse(await readFile(assetsPath, "utf8"));
      const outputRootPath = `${path.resolve(outputRoot)}${path.sep}`;
      const pageEntries = new Map();
      for (const route of manifest.routes) pageEntries.set(route.page_id, route.page_file);
      const imports = [];
      const pairs = [];
      const assetPairs = [];
      const diagramPairs = [];
      const searchImports = [];
      const searchPairs = [];
      let index = 0;
      for (const [id, relativeFile] of [...pageEntries].sort(([a], [b]) => a.localeCompare(b))) {
        const pagePath = path.resolve(outputRoot, relativeFile);
        const page = JSON.parse(await readFile(pagePath, "utf8"));
        if (page.schema !== "evin.docs.page/v1" || page.id !== id) {
          throw new Error(`Invalid Evin page data for ${id}: ${relativeFile}`);
        }
        const diagrams = [];
        const visit = (value) => {
          if (!value || typeof value !== "object") return;
          if (Array.isArray(value)) {
            for (const child of value) visit(child);
            return;
          }
          if (value.type === "Diagram" && value.svg_path) {
            diagrams.push(value);
          }
          for (const child of Object.values(value)) visit(child);
        };
        visit(page.page?.ast);
        const mappedDiagrams = [];
        for (const diagram of diagrams.sort((a, b) => String(a.id).localeCompare(String(b.id)))) {
          if (typeof diagram.id !== "string" || !diagram.id) {
            throw new Error(`Generated Evin diagram on ${id} has no ID`);
          }
          const diagramPath = path.resolve(outputRoot, diagram.svg_path);
          if (!diagramPath.startsWith(outputRootPath)) {
            throw new Error(`Invalid Evin diagram path for ${id}: ${diagram.svg_path}`);
          }
          const importName = `diagram${index++}`;
          imports.push(
            `import ${importName} from ${JSON.stringify(`${viteFilePath(diagramPath)}?raw`)};`,
          );
          mappedDiagrams.push(`${JSON.stringify(diagram.id)}: ${importName}`);
        }
        diagramPairs.push(`${JSON.stringify(id)}: { ${mappedDiagrams.join(", ")} }`);
        const importName = `page${index++}`;
        imports.push(`import ${importName} from ${JSON.stringify(viteFilePath(pagePath))};`);
        pairs.push(`${JSON.stringify(id)}: ${importName}`);
      }
      for (const [pageId, entries] of Object.entries(assets).sort(([a], [b]) =>
        a.localeCompare(b),
      )) {
        const mapped = [];
        for (const [source, relativeFile] of Object.entries(entries).sort(([a], [b]) =>
          a.localeCompare(b),
        )) {
          const assetPath = path.resolve(outputRoot, relativeFile);
          if (!assetPath.startsWith(outputRootPath)) {
            throw new Error(`Invalid Evin asset path for ${pageId}: ${relativeFile}`);
          }
          const importName = `asset${index++}`;
          imports.push(`import ${importName} from ${JSON.stringify(viteFilePath(assetPath))};`);
          mapped.push(`${JSON.stringify(source)}: ${importName}`);
        }
        assetPairs.push(`${JSON.stringify(pageId)}: { ${mapped.join(", ")} }`);
      }
      if (!Array.isArray(manifest.search_indexes)) {
        throw new Error("Evin docs manifest is missing search_indexes");
      }
      for (const artifact of manifest.search_indexes) {
        if (
          typeof artifact.version !== "string" ||
          typeof artifact.locale !== "string" ||
          typeof artifact.path !== "string" ||
          !Number.isSafeInteger(artifact.byte_length) ||
          typeof artifact.sha256 !== "string" ||
          !/^[a-f0-9]{64}$/.test(artifact.sha256)
        ) {
          throw new Error("Invalid Evin search artifact descriptor");
        }
        const searchPath = path.resolve(outputRoot, artifact.path);
        if (!searchPath.startsWith(outputRootPath)) {
          throw new Error(`Invalid Evin search artifact path: ${artifact.path}`);
        }
        const artifactKey = `${artifact.version}/${artifact.locale}`;
        if (searchPairs.some((pair) => pair.startsWith(`${JSON.stringify(artifactKey)}:`))) {
          throw new Error(`Duplicate Evin search artifact: ${artifactKey}`);
        }
        const importName = `search${searchPairs.length}`;
        searchImports.push(
          `import ${importName} from ${JSON.stringify(`${viteFilePath(searchPath)}?url`)};`,
        );
        searchPairs.push(
          `${JSON.stringify(artifactKey)}: { url: ${importName}, byteLength: ${artifact.byte_length}, sha256: ${JSON.stringify(artifact.sha256)} }`,
        );
      }
      moduleCode = [
        `import manifest from ${JSON.stringify(viteFilePath(manifestPath))};`,
        `import { createDocsRuntime } from "everyinit";`,
        ...imports,
        `const assets = { ${assetPairs.join(", ")} };`,
        `const diagrams = { ${diagramPairs.join(", ")} };`,
        `const pages = { ${pairs.join(", ")} };`,
        `const docs = { ...createDocsRuntime(manifest, pages, assets), diagrams };`,
        `export { manifest, pages, assets, diagrams };`,
        `export const { routes, resolveRoute, counterpart, routeForPage } = docs;`,
        `export default docs;`,
      ].join("\n");
      searchModuleCode = [
        `import { decodeSearchIndex, searchDocs } from "everyinit";`,
        ...searchImports,
        `const artifacts = { ${searchPairs.join(", ")} };`,
        `const cache = new Map();`,
        `export async function loadSearchIndex(version, locale) {`,
        `  const key = [version, locale].join("/");`,
        `  if (cache.has(key)) return cache.get(key);`,
        `  const artifact = artifacts[key];`,
        `  if (!artifact) throw new Error(` +
          JSON.stringify("No Evin search index for ") +
          ` + key);`,
        `  const promise = fetch(artifact.url).then(async (response) => {`,
        `    if (!response.ok) throw new Error(` +
          JSON.stringify("Failed to load Evin search index: ") +
          ` + response.status);`,
        `    const bytes = new Uint8Array(await response.arrayBuffer());`,
        `    if (bytes.byteLength !== artifact.byteLength) throw new Error("Evin search index size does not match the manifest");`,
        `    if (globalThis.crypto?.subtle) {`,
        `      const digest = new Uint8Array(await globalThis.crypto.subtle.digest("SHA-256", bytes));`,
        `      const hash = [...digest].map((byte) => byte.toString(16).padStart(2, "0")).join("");`,
        `      if (hash !== artifact.sha256) throw new Error("Evin search index hash does not match the manifest");`,
        `    }`,
        `    const index = decodeSearchIndex(bytes);`,
        `    index.version = version;`,
        `    index.locale = locale;`,
        `    return index;`,
        `  }).catch((error) => { cache.delete(key); throw error; });`,
        `  cache.set(key, promise);`,
        `  return promise;`,
        `}`,
        `export { searchDocs };`,
        `export default { loadSearchIndex, searchDocs };`,
      ].join("\n");
      sourceRoots = watch.map((entry) => path.resolve(workingDirectory, entry));
      return moduleCode;
    })().finally(() => {
      building = undefined;
    });
    return building;
  }

  return {
    name: "everyinit/vite",
    enforce: "pre",
    configResolved(config) {
      projectRoot = config.root;
      workingDirectory = path.resolve(projectRoot, options.cwd ?? ".");
      manifestPath = path.resolve(
        workingDirectory,
        options.manifest ?? "dist/docs-data/manifest.json",
      );
    },
    async buildStart() {
      await buildData(true);
    },
    resolveId(id) {
      if (id === publicId) return resolvedId;
      if (id === searchPublicId) return searchResolvedId;
    },
    async load(id) {
      if (id === resolvedId || id === searchResolvedId) {
        await buildData();
        return id === resolvedId ? moduleCode : searchModuleCode;
      }
    },
    configureServer(devServer) {
      server = devServer;
      sourceRoots = watch.map((entry) => path.resolve(workingDirectory, entry));
      devServer.watcher.add(sourceRoots);
    },
    async handleHotUpdate(context) {
      if (
        !sourceRoots.some(
          (root) => context.file === root || context.file.startsWith(`${root}${path.sep}`),
        )
      )
        return;
      await buildData(true);
      const module = server?.moduleGraph.getModuleById(resolvedId);
      if (module && server) {
        server.moduleGraph.invalidateModule(module);
        return [module];
      }
    },
  };
}
