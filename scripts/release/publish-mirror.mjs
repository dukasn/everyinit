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

import { readFile, readdir } from "node:fs/promises";
import { relative, resolve, sep } from "node:path";
import { spawnSync } from "node:child_process";

const [channel, version, mode, mirrorRoot = "dist/mirror"] = process.argv.slice(2);
if (!new Set(["unstable", "canary"]).has(channel)) {
  throw new Error(
    "usage: node scripts/release/publish-mirror.mjs <unstable|canary> <version> <all|objects|pointers> [mirror-root]",
  );
}
if (!new Set(["all", "objects", "pointers"]).has(mode)) {
  throw new Error("publish mode must be all, objects, or pointers");
}
if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(version ?? "")) {
  throw new Error("a valid release version is required");
}

const bucket = channel === "unstable" ? "everyinit-preview-artifacts" : "everyinit-artifacts";
const config = "infra/mirror/wrangler.toml";
const entries = [];
const releases = {};

async function collect(root) {
  const absoluteRoot = resolve(root);
  async function walk(directory) {
    const children = await readdir(directory, { withFileTypes: true });
    for (const child of children.toSorted((a, b) => a.name.localeCompare(b.name))) {
      const path = resolve(directory, child.name);
      if (child.isDirectory()) {
        await walk(path);
      } else if (child.isFile()) {
        const relativePath = relative(absoluteRoot, path).split(sep).join("/");
        const rootName = relativePath.split("/")[0];
        const binary = rootName === "channels" ? "channels" : rootName;
        if (!new Set(["evin", "esh", "channels"]).has(binary)) {
          throw new Error(`unexpected mirror object path ${relativePath}`);
        }
        entries.push({
          binary,
          path,
          key: relativePath,
        });
      }
    }
  }
  await walk(absoluteRoot);
}

await collect(mirrorRoot);

for (const binary of ["evin", "esh"]) {
  if (!entries.some((entry) => entry.binary === binary)) {
    throw new Error(`mirror objects for ${binary} are required`);
  }
  const prefix = `${binary}/${channel}/v${version}/`;
  const candidate = entries.filter(
    (entry) => entry.binary === binary && entry.key.startsWith(prefix),
  );
  const names = new Set(candidate.map((entry) => entry.key.slice(prefix.length)));
  const expected = [
    "release.json",
    "SHA256SUMS",
    "SHA256SUMS.sig",
    `${binary}-v${version}-x86_64-unknown-linux-musl.tar.gz`,
  ];
  for (const name of expected) {
    if (!names.has(name)) throw new Error(`${binary} mirror metadata is missing ${name}`);
  }
  const release = JSON.parse(
    await readFile(candidate.find((entry) => entry.key.endsWith("/release.json")).path, "utf8"),
  );
  if (release.name !== binary || release.version !== version || release.channel !== channel) {
    throw new Error(`${binary} release metadata does not match ${channel} ${version}`);
  }
  releases[binary] = release;
}

const selected = entries.filter((entry) =>
  entry.key.startsWith(`${entry.binary}/${channel}/v${version}/`),
);
const pointer = entries.find((entry) => entry.key === `channels/${channel}.json`);
if (!pointer) throw new Error(`${channel} channel pointer is missing`);
const manifest = JSON.parse(await readFile(pointer.path, "utf8"));
if (
  manifest.channel !== channel ||
  manifest.version !== version ||
  !manifest.binaries?.evin ||
  !manifest.binaries?.esh
) {
  throw new Error(`${channel} channel pointer does not identify both binaries at ${version}`);
}
if (
  manifest.git_commit !== releases.evin.git_commit ||
  releases.evin.git_commit !== releases.esh.git_commit
) {
  throw new Error(
    `${channel} channel pointer and both binary manifests must identify one source commit`,
  );
}
for (const binary of ["evin", "esh"]) {
  const expectedUrl = `https://dl.everyinit.dev/${binary}/${channel}/v${version}/release.json`;
  if (
    manifest.binaries[binary].version !== version ||
    manifest.binaries[binary].release_url !== expectedUrl
  ) {
    throw new Error(`${channel} channel pointer has an invalid ${binary} release URL`);
  }
}
const objects = selected;
const pointers = [pointer];

const uploadEntries =
  mode === "objects" ? objects : mode === "pointers" ? pointers : [...objects, ...pointers];
for (const entry of uploadEntries) {
  const result = spawnSync(
    "corepack",
    [
      "npm",
      "exec",
      "--",
      "wrangler",
      "r2",
      "object",
      "put",
      `${bucket}/${entry.key}`,
      "--file",
      entry.path,
      "--config",
      config,
    ],
    { stdio: "inherit" },
  );
  if (result.error) throw result.error;
  if (result.status !== 0)
    throw new Error(`Wrangler failed to publish ${entry.binary}/${entry.key}`);
}

console.log(`Published ${mode} for ${channel} ${version} mirror objects for Evin and ESH`);
