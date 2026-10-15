// Copyright (C) 2026 Eduardo Marques Santos
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

import { createHash } from "node:crypto";
import { gunzipSync } from "node:zlib";
import {
  mkdtemp,
  readFile,
  readdir,
  rm,
  stat,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";

const [name, version, target, artifactRoot] = process.argv.slice(2);
if (!(["evin", "esh"].includes(name) && version && target && artifactRoot)) {
  throw new Error(
    "usage: node scripts/release/verify-distribution.mjs <evin|esh> <version> <target> <artifact-root>",
  );
}
if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(version)) {
  throw new Error(`invalid release version: ${version}`);
}
if (target !== "x86_64-unknown-linux-musl") {
  throw new Error(`unsupported release target: ${target}`);
}

const root = resolve(artifactRoot);
const archiveDirectory = join(root, "github");
const packageDirectory = join(root, "packages");
const tarballName = `${name}-v${version}-${target}.tar.gz`;
const tarballPath = join(archiveDirectory, tarballName);
const packageFiles = await readdir(packageDirectory);
const debs = packageFiles.filter((file) =>
  file.startsWith(`${name}_${version}-`) && file.endsWith("_amd64.deb"),
);
const apks = packageFiles.filter((file) =>
  file.startsWith(`${name}-${version}-r`) && file.endsWith(".apk"),
);
const archPackages = packageFiles.filter((file) =>
  file.startsWith(`${name}-${version}-`) && file.endsWith("-x86_64.pkg.tar.zst"),
);
for (const [label, matches] of [
  ["DEB", debs],
  ["APK", apks],
  ["Arch", archPackages],
]) {
  if (matches.length !== 1) {
    throw new Error(`expected exactly one ${name} ${label} for ${version}; found ${matches.length}`);
  }
}

const artifacts = [
  tarballPath,
  join(packageDirectory, debs[0]),
  join(packageDirectory, apks[0]),
  join(packageDirectory, archPackages[0]),
];
for (const file of artifacts) {
  const fileStat = await stat(file).catch(() => undefined);
  if (!fileStat?.isFile() || fileStat.size === 0) {
    throw new Error(`required distribution artifact is missing or empty: ${file}`);
  }
}

const sourceLicenseFiles = await licenseFiles(resolve("LICENSES"));
const sourceFiles = new Map([
  ["LICENSE", await readFile("LICENSE")],
  ["THIRD_PARTY_NOTICES.md", await readFile("THIRD_PARTY_NOTICES.md")],
  ...[...sourceLicenseFiles].map(([path, contents]) => [`LICENSES/${path}`, contents]),
]);
const releaseDirectory = `${name}-v${version}-${target}`;
const tarEntries = parseTar(gunzipSync(await readFile(tarballPath)));
const tarRoot = `${releaseDirectory}/`;
const tarBinary = entryAt(tarEntries, `${tarRoot}bin/${name}`, tarballPath);
assertExactPaths(
  tarEntries,
  new Set([`${tarRoot}bin/${name}`, ...[...sourceFiles.keys()].map((path) => `${tarRoot}${path}`)]),
  tarballPath,
);
if ((tarBinary.mode & 0o111) === 0) {
  throw new Error(`${tarballPath} does not mark bin/${name} executable`);
}
await compareDistributionFiles(tarEntries, tarRoot, sourceFiles, tarballPath);

const debPath = artifacts[1];
const apkPath = artifacts[2];
const archPath = artifacts[3];
const tempRoot = await mkdtemp(join(tmpdir(), `everyinit-${name}-distribution-`));
try {
  const debRoot = join(tempRoot, "deb");
  await run("dpkg-deb", ["-x", debPath, debRoot]);
  const debControl = await run("dpkg-deb", ["-f", debPath]);
  assertDebControl(debControl, name, version, debPath);
  const debFiles = await filesystemFiles(debRoot);
  assertExactPaths(debFiles, nativePayloadPaths(name, sourceLicenseFiles), debPath);
  await verifyNativeBinary(debFiles, `usr/bin/${name}`, target, name, debPath);
  await compareDistributionFiles(
    debFiles,
    `usr/share/licenses/${name}/`,
    new Map([["LICENSE", sourceFiles.get("LICENSE")]]),
    debPath,
  );
  await compareDistributionFiles(
    debFiles,
    `usr/share/licenses/${name}/LICENSES/`,
    sourceLicenseFiles,
    debPath,
  );
  await compareDistributionFiles(
    debFiles,
    `usr/share/doc/${name}/`,
    new Map([["THIRD_PARTY_NOTICES.md", sourceFiles.get("THIRD_PARTY_NOTICES.md")]]),
    debPath,
  );

  const apkEntries = parseTar(gunzipSync(await readFile(apkPath)));
  assertPackageInfo(apkEntries, name, apkPath);
  assertExactPaths(apkEntries, new Set([...nativePayloadPaths(name, sourceLicenseFiles), ".PKGINFO"]), apkPath);
  await verifyNativeBinary(apkEntries, `usr/bin/${name}`, target, name, apkPath);
  await verifyNativeLicensePayload(apkEntries, name, sourceFiles, sourceLicenseFiles, apkPath);

  const archTar = await run("zstd", ["--decompress", "--stdout", archPath], { encoding: "buffer" });
  const archEntries = parseTar(archTar);
  assertPackageInfo(archEntries, name, archPath);
  assertExactPaths(archEntries, new Set([...nativePayloadPaths(name, sourceLicenseFiles), ".PKGINFO", ".MTREE"]), archPath);
  await verifyNativeBinary(archEntries, `usr/bin/${name}`, target, name, archPath);
  await verifyNativeLicensePayload(archEntries, name, sourceFiles, sourceLicenseFiles, archPath);

  await verifyChecksums(artifacts, tempRoot);
} finally {
  await rm(tempRoot, { recursive: true, force: true });
}

console.log(`Verified ${name} distribution artifacts, embedded licenses, and checksums for ${version}`);

async function licenseFiles(directory, prefix = "") {
  const files = new Map();
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    const relativePath = prefix ? `${prefix}/${entry.name}` : entry.name;
    if (entry.isDirectory()) {
      for (const [nestedPath, contents] of await licenseFiles(path, relativePath)) {
        files.set(nestedPath, contents);
      }
    } else if (entry.isFile()) {
      files.set(relativePath, await readFile(path));
    }
  }
  return files;
}

function parseTar(buffer) {
  const entries = new Map();
  let offset = 0;
  while (offset + 512 <= buffer.length) {
    const header = buffer.subarray(offset, offset + 512);
    if (header.every((byte) => byte === 0)) {
      offset += 512;
      continue;
    }
    const readField = (start, length) =>
      header.subarray(start, start + length).toString("utf8").replace(/\0.*$/s, "");
    const name = readField(0, 100);
    const prefix = readField(345, 155);
    const path = prefix ? `${prefix}/${name}` : name;
    const rawSize = readField(124, 12).trim();
    const size = rawSize ? Number.parseInt(rawSize, 8) : 0;
    if (!Number.isSafeInteger(size) || size < 0) {
      throw new Error(`invalid tar member size for ${path}`);
    }
    const type = readField(156, 1) || "0";
    const modeText = readField(100, 8).trim();
    const mode = modeText ? Number.parseInt(modeText, 8) : 0;
    const dataStart = offset + 512;
    const dataEnd = dataStart + size;
    if (dataEnd > buffer.length) {
      throw new Error(`truncated tar member: ${path}`);
    }
    if (type === "0" || type === "\0" || type === "7") {
      const memberPath = path.replace(/\/$/, "");
      if (entries.has(memberPath)) {
        throw new Error(`duplicate tar member: ${memberPath}`);
      }
      entries.set(memberPath, {
        data: buffer.subarray(dataStart, dataEnd),
        mode,
      });
    }
    offset = dataStart + Math.ceil(size / 512) * 512;
  }
  return entries;
}

function entryAt(entries, path, artifact) {
  const entry = entries.get(path);
  if (!entry) throw new Error(`${artifact} is missing ${path}`);
  return entry;
}

function assertExactPaths(entries, expected, artifact) {
  const actual = new Set(entries.keys());
  const missing = [...expected].filter((path) => !actual.has(path));
  const unexpected = [...actual].filter((path) => !expected.has(path));
  if (missing.length || unexpected.length) {
    throw new Error(
      `${artifact} has an unexpected file list${missing.length ? `; missing: ${missing.join(", ")}` : ""}${unexpected.length ? `; unexpected: ${unexpected.join(", ")}` : ""}`,
    );
  }
}

function nativePayloadPaths(packageName, licenses) {
  return new Set([
    `usr/bin/${packageName}`,
    `usr/share/licenses/${packageName}/LICENSE`,
    ...[...licenses.keys()].map((path) => `usr/share/licenses/${packageName}/LICENSES/${path}`),
    `usr/share/doc/${packageName}/THIRD_PARTY_NOTICES.md`,
  ]);
}

async function compareDistributionFiles(entries, prefix, files, artifact) {
  for (const [path, expected] of files) {
    const entry = entryAt(entries, `${prefix}${path}`, artifact);
    const expectedData = Buffer.isBuffer(expected) ? expected : expected.data;
    if (!entry.data.equals(expectedData)) {
      throw new Error(`${artifact} contains unexpected or stale contents for ${path}`);
    }
  }
}

async function verifyNativeBinary(entries, path, targetTriple, packageName, artifact) {
  const entry = entryAt(entries, path, artifact);
  const builtBinary = await readFile(`target/${targetTriple}/release/${packageName}`);
  if (!entry.data.equals(builtBinary)) {
    throw new Error(`${artifact} does not contain the built ${packageName} binary`);
  }
  if ((entry.mode & 0o111) === 0) {
    throw new Error(`${artifact} does not mark ${path} executable`);
  }
}

async function verifyNativeLicensePayload(entries, packageName, sourceFiles, licenses, artifact) {
  await compareDistributionFiles(
    entries,
    `usr/share/licenses/${packageName}/`,
    new Map([["LICENSE", sourceFiles.get("LICENSE")]]),
    artifact,
  );
  await compareDistributionFiles(
    entries,
    `usr/share/licenses/${packageName}/LICENSES/`,
    licenses,
    artifact,
  );
  await compareDistributionFiles(
    entries,
    `usr/share/doc/${packageName}/`,
    new Map([["THIRD_PARTY_NOTICES.md", sourceFiles.get("THIRD_PARTY_NOTICES.md")]]),
    artifact,
  );
}

function assertPackageInfo(entries, packageName, artifact) {
  const info = entryAt(entries, ".PKGINFO", artifact).data.toString("utf8");
  if (!info.includes(`pkgname = ${packageName}\n`) || !info.includes("license = GPL-3.0-only\n")) {
    throw new Error(`${artifact} has incorrect package identity or license metadata`);
  }
}

function assertDebControl(control, packageName, expectedVersion, artifact) {
  const fields = new Map(
    control.split(/\r?\n/).flatMap((line) => {
      const match = /^([^:]+):\s*(.*)$/.exec(line);
      return match ? [[match[1], match[2]]] : [];
    }),
  );
  if (fields.get("Package") !== packageName || !fields.get("Version")?.startsWith(`${expectedVersion}-`)) {
    throw new Error(`${artifact} has incorrect DEB package identity or version`);
  }
}

async function filesystemFiles(directory, prefix = "") {
  const files = new Map();
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    const relativePath = prefix ? `${prefix}/${entry.name}` : entry.name;
    if (entry.isDirectory()) {
      for (const [nestedPath, contents] of await filesystemFiles(path, relativePath)) {
        files.set(nestedPath, contents);
      }
    } else if (entry.isFile()) {
      files.set(relativePath, {
        data: await readFile(path),
        mode: (await stat(path)).mode,
      });
    }
  }
  return files;
}

async function verifyChecksums(files, temporaryDirectory) {
  const lines = [];
  for (const file of files) {
    const digest = createHash("sha256").update(await readFile(file)).digest("hex");
    lines.push(`${digest}  ${resolve(file)}`);
  }
  const manifest = join(temporaryDirectory, "SHA256SUMS");
  await writeFile(manifest, `${lines.join("\n")}\n`);
  await run("sha256sum", ["--check", manifest]);
}

async function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    encoding: options.encoding ?? "utf8",
    maxBuffer: 64 * 1024 * 1024,
  });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    const detail = Buffer.isBuffer(result.stderr) ? result.stderr.toString("utf8") : result.stderr;
    throw new Error(`${command} ${args.join(" ")} failed: ${detail?.trim() ?? "unknown error"}`);
  }
  return result.stdout;
}
