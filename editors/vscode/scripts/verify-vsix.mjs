import { stat, readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { createHash } from "node:crypto";

const vscodeRoot = resolve(import.meta.dirname, "..");
const pkgJson = JSON.parse(await readFile(resolve(vscodeRoot, "package.json"), "utf8"));
const version = process.env.RELEASE_VERSION ?? pkgJson.version;
const vsixPath = resolve(vscodeRoot, "dist/github", `everyinit-${version}.vsix`);

try {
  const stats = await stat(vsixPath);
  if (!stats.isFile()) {
    throw new Error(`Expected VSIX file at ${vsixPath}, but found non-file`);
  }
  if (stats.size === 0) {
    throw new Error(`VSIX package at ${vsixPath} is empty`);
  }

  const content = await readFile(vsixPath);
  // Basic zip magic number verification: PK\x03\x04
  if (content[0] !== 0x50 || content[1] !== 0x4b || content[2] !== 0x03 || content[3] !== 0x04) {
    throw new Error(`VSIX at ${vsixPath} is not a valid zip archive (magic number mismatch)`);
  }

  const sha256 = createHash("sha256").update(content).digest("hex");
  const sizeKb = (stats.size / 1024).toFixed(2);
  console.log(`Verified VSIX package: everyinit-${version}.vsix (${sizeKb} KB, sha256: ${sha256})`);
} catch (error) {
  console.error(`Failed to verify VSIX package: ${error.message}`);
  process.exit(1);
}

