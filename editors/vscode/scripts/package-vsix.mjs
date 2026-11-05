import { mkdir } from "node:fs/promises";
import { resolve, dirname } from "node:path";
import { spawnSync } from "node:child_process";
import { readFile } from "node:fs/promises";

const vscodeRoot = resolve(import.meta.dirname, "..");
const pkgJson = JSON.parse(await readFile(resolve(vscodeRoot, "package.json"), "utf8"));
const version = process.env.RELEASE_VERSION ?? pkgJson.version;
const outDir = resolve(vscodeRoot, "dist/github");
const outFile = resolve(outDir, `everyinit-${version}.vsix`);

await mkdir(outDir, { recursive: true });

console.log(`Packaging Everyinit VS Code extension v${version} into ${outFile}...`);

const vsce = resolve(vscodeRoot, "node_modules/.bin/vsce");
const result = spawnSync(vsce, ["package", "--no-dependencies", "-o", outFile], {
  cwd: vscodeRoot,
  stdio: "inherit",
  env: {
    ...process.env,
    npm_package_version: version,
  },
});

if (result.error) {
  throw result.error;
}

if (result.status !== 0) {
  process.exit(result.status ?? 1);
}

console.log(`Successfully packaged: ${outFile}`);

