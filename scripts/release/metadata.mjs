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

import { createPrivateKey, createPublicKey, sign, verify, createHash } from "node:crypto";
import { copyFile, mkdir, readFile, stat, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { spawnSync } from "node:child_process";

const [
  binary,
  version,
  target,
  outputRoot = "dist/mirror",
  archiveRoot = process.env.RELEASE_ARCHIVE_ROOT ?? "dist",
] = process.argv.slice(2);
const validBinary = binary === "evin" || binary === "esh";
const validVersion = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(version ?? "");
const validTarget = target === "x86_64-unknown-linux-musl";

if (!validBinary || !validVersion || !validTarget) {
  throw new Error(
    "usage: node scripts/release/metadata.mjs <evin|esh> <version> <target> [output-root] [archive-root]",
  );
}

const channel = version.includes("-") ? "canary" : "latest";
const filename = `${binary}-v${version}-${target}.tar.gz`;
const outputDirectory = resolve(outputRoot, binary, `v${version}`);
const outputArchive = resolve(outputDirectory, filename);
const archivePath = resolve(archiveRoot, filename);
const privateKeyFile = process.env.RELEASE_SIGNING_KEY_FILE;
const releaseBaseUrl = (process.env.RELEASE_BASE_URL ?? "https://dl.everyinit.dev").replace(
  /\/+$/,
  "",
);
if (!privateKeyFile) {
  throw new Error("RELEASE_SIGNING_KEY_FILE must point to the Ed25519 PKCS#8 private key");
}
const privateKey = createPrivateKey(await readFile(resolve(privateKeyFile)));
if (privateKey.asymmetricKeyType !== "ed25519") {
  throw new Error("release signing key must be Ed25519");
}
const signingPublicKey = createPublicKey(privateKey)
  .export({ type: "spki", format: "pem" })
  .toString()
  .trim();
const pinnedPublicKey = (await readFile("infra/get/release-key.pub", "utf8")).trim();
if (signingPublicKey !== pinnedPublicKey) {
  throw new Error("release signing key does not match infra/get/release-key.pub");
}
const installerSource = await readFile("infra/get/src/index.ts", "utf8");
if (!installerSource.includes(pinnedPublicKey)) {
  throw new Error("installer Worker key does not match infra/get/release-key.pub");
}

const archive = await readFile(archivePath).catch(() => {
  throw new Error(`release archive not found: ${archivePath}`);
});
const digest = createHash("sha256").update(archive).digest("hex");
const git = spawnSync("git", ["rev-parse", "HEAD"], { encoding: "utf8" });
if (git.status !== 0) {
  throw new Error(`could not resolve release commit: ${git.stderr.trim()}`);
}

const milestone = await releaseMilestone(version);
await mkdir(outputDirectory, { recursive: true });
await copyFile(archivePath, outputArchive);
const checksums = `${digest}  ${filename}\n`;
const checksumPath = resolve(outputDirectory, "SHA256SUMS");
await writeFile(checksumPath, checksums, { mode: 0o644 });

const releaseMetadata = {
  name: binary,
  version,
  channel,
  ...(milestone ? { milestone } : {}),
  released_at: new Date().toISOString(),
  git_commit: git.stdout.trim(),
  artifacts: [
    {
      filename,
      os: "linux",
      arch: "x86_64",
      target,
      size: (await stat(archivePath)).size,
      sha256: digest,
    },
  ],
};
await writeFile(resolve(outputDirectory, "release.json"), `${JSON.stringify(releaseMetadata, null, 2)}\n`, {
  mode: 0o644,
});

const signature = sign(null, Buffer.from(checksums), privateKey);
if (!verify(null, Buffer.from(checksums), createPublicKey(privateKey), signature)) {
  throw new Error("generated signature did not verify against the signing key");
}
await writeFile(resolve(outputDirectory, "SHA256SUMS.sig"), `${signature.toString("base64")}\n`, {
  mode: 0o644,
});

const versionUrl = `${releaseBaseUrl}/${binary}/v${version}`;
const channelManifest = {
  name: binary,
  channel,
  latest_version: version,
  release_url: `${versionUrl}/release.json`,
};
await mkdir(resolve(outputRoot, binary), { recursive: true });
await writeFile(
  resolve(outputRoot, binary, `${channel}.json`),
  `${JSON.stringify(channelManifest, null, 2)}\n`,
  { mode: 0o644 },
);

console.log(`Generated signed ${binary} ${version} metadata in ${outputDirectory}`);

async function releaseMilestone(releaseVersion) {
  const changelog = await readFile("CHANGELOG.md", "utf8");
  const escapedVersion = releaseVersion.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const match = new RegExp(`^## \\[v${escapedVersion}\\]\\s+—\\s+(.+?)\\s*$`, "m").exec(changelog);
  const allowedMilestone = /^(Alpha|Bravo|Charlie|Delta|Echo|Foxtrot|Golf|Hotel|India|Juliett|Kilo|Lima|Mike|November|Oscar|Papa|Quebec|Romeo|Sierra|Tango|Uniform|Victor|Whiskey|X-ray|Yankee|Zulu)-\d+$/;
  if (releaseVersion.includes("-canary.") && (!match || !allowedMilestone.test(match[1]))) {
    throw new Error(`CHANGELOG.md has no canary milestone label for v${releaseVersion}`);
  }
  return match?.[1];
}
