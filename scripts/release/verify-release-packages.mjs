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

import { createHash, createPublicKey, verify } from "node:crypto";
import { readFile, stat } from "node:fs/promises";
import { resolve } from "node:path";

const [
  targetDir = "target/release-archives",
  version = process.env.RELEASE_VERSION,
  target = process.env.RELEASE_TARGET ?? "x86_64-unknown-linux-musl",
] = process.argv.slice(2);

if (!version) {
  throw new Error("usage: verify-release-packages.mjs [target-dir] <version> [target]");
}

const dir = resolve(targetDir);

const expectedFiles = [
  `evin-v${version}-${target}.tar.gz`,
  `esh-v${version}-${target}.tar.gz`,
  `evin_${version}-1_amd64.deb`,
  `esh_${version}-1_amd64.deb`,
  `evin-${version}-1-x86_64.pkg.tar.zst`,
  `esh-${version}-1-x86_64.pkg.tar.zst`,
  `everyinit-${version}.vsix`,
  "SHA256SUMS",
  "SHA256SUMS.sig",
  "release-key.pub",
];

for (const name of expectedFiles) {
  const filePath = resolve(dir, name);
  const info = await stat(filePath).catch(() => undefined);
  if (!info || !info.isFile() || info.size === 0) {
    throw new Error(`Required release artifact is missing or empty: ${filePath}`);
  }
}

// 1. Verify file magic formats
const checkMagic = async (name, bytes, desc) => {
  const content = await readFile(resolve(dir, name));
  for (let i = 0; i < bytes.length; i++) {
    if (content[i] !== bytes[i]) {
      throw new Error(`${name} does not match expected magic bytes for ${desc}`);
    }
  }
};

await checkMagic(
  `evin_${version}-1_amd64.deb`,
  [0x21, 0x3c, 0x61, 0x72, 0x63, 0x68, 0x3e, 0x0a],
  "Debian package (!<arch>\\n)",
);
await checkMagic(
  `esh_${version}-1_amd64.deb`,
  [0x21, 0x3c, 0x61, 0x72, 0x63, 0x68, 0x3e, 0x0a],
  "Debian package (!<arch>\\n)",
);
await checkMagic(
  `evin-${version}-1-x86_64.pkg.tar.zst`,
  [0x28, 0xb5, 0x2f, 0xfd],
  "Arch Linux zstd package",
);
await checkMagic(
  `esh-${version}-1-x86_64.pkg.tar.zst`,
  [0x28, 0xb5, 0x2f, 0xfd],
  "Arch Linux zstd package",
);
await checkMagic(
  `everyinit-${version}.vsix`,
  [0x50, 0x4b, 0x03, 0x04],
  "VS Code extension ZIP/VSIX archive",
);
await checkMagic(`evin-v${version}-${target}.tar.gz`, [0x1f, 0x8b], "Gzip compressed tarball");
await checkMagic(`esh-v${version}-${target}.tar.gz`, [0x1f, 0x8b], "Gzip compressed tarball");

// 2. Verify SHA256SUMS
const manifest = await readFile(resolve(dir, "SHA256SUMS"), "utf8");
const manifestLines = manifest.trim().split("\n");
const manifestEntries = new Map();

for (const line of manifestLines) {
  const parts = line.trim().split(/\s+/);
  if (parts.length !== 2) {
    throw new Error(`Malformed line in SHA256SUMS: "${line}"`);
  }
  manifestEntries.set(parts[1], parts[0]);
}

for (const name of expectedFiles) {
  if (name === "SHA256SUMS" || name.startsWith("SHA256SUMS.") || name === "release-key.pub") {
    continue;
  }
  const expectedHash = manifestEntries.get(name);
  if (!expectedHash) {
    throw new Error(`Artifact ${name} is missing from SHA256SUMS`);
  }
  const content = await readFile(resolve(dir, name));
  const actualHash = createHash("sha256").update(content).digest("hex");
  if (actualHash !== expectedHash) {
    throw new Error(`Hash mismatch for ${name}: expected ${expectedHash}, computed ${actualHash}`);
  }
}

// 3. Verify cryptographic signature
const signature = await readFile(resolve(dir, "SHA256SUMS.sig"));
const publicKeyText = (await readFile(resolve(dir, "release-key.pub"), "utf8")).trim();
const publicKey = createPublicKey(publicKeyText);

const signatureValid = verify(null, Buffer.from(manifest), publicKey, signature);
if (!signatureValid) {
  throw new Error("SHA256SUMS cryptographic signature verification failed!");
}

const pinnedKey = (await readFile("infra/get/release-key.pub", "utf8")).trim();
const isPinned = publicKeyText === pinnedKey;

console.log(
  `Verified all ${expectedFiles.length} release artifacts, magic formats, checksums, and Ed25519 signature (${isPinned ? "official pinned release key" : "rehearsal key"}) for v${version}.`,
);
