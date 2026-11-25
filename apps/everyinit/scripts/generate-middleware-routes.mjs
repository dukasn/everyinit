import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repositoryRoot = resolve(fileURLToPath(new URL("../../../", import.meta.url)));
const manifestPath = resolve(repositoryRoot, "dist/docs-data/manifest.json");
const outputPath = resolve(repositoryRoot, "dist/docs-data/legacy-root-paths.js");
const declarationPath = resolve(repositoryRoot, "dist/docs-data/legacy-root-paths.d.ts");
const manifest = JSON.parse(await readFile(manifestPath, "utf8"));

if (manifest?.schema !== "evin.docs.manifest/v1" || !Array.isArray(manifest.routes)) {
  throw new Error(`Invalid Evin documentation manifest at ${manifestPath}`);
}

const paths = [...new Set(manifest.routes.map((route) => route.path))].sort();
await mkdir(dirname(outputPath), { recursive: true });
await writeFile(
  outputPath,
  `export const documentationRoutePaths = new Set(${JSON.stringify(paths)});\n`,
);
await writeFile(
  declarationPath,
  "export declare const documentationRoutePaths: ReadonlySet<string>;\n",
);
