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
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

const [channel, version, commit, pointerFlag] = process.argv.slice(2);
const baseUrl = (process.env.RELEASE_BASE_URL ?? "https://dl.everyinit.dev").replace(/\/+$/, "");
if (
  !new Set(["unstable", "canary"]).has(channel) ||
  !/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(version ?? "")
) {
  throw new Error(
    "usage: node scripts/release/verify-mirror-candidate.mjs <unstable|canary> <version> <commit> [--check-pointer]",
  );
}
if (!/^[0-9a-f]{40,64}$/.test(commit ?? "")) {
  throw new Error("candidate commit is required");
}

const publicKey = createPublicKey(await readFile("infra/get/release-key.pub"));
const target = "x86_64-unknown-linux-musl";

async function get(url) {
  const response = await fetch(url, { redirect: "error" });
  if (!response.ok) throw new Error(`mirror returned HTTP ${response.status} for ${url}`);
  return Buffer.from(await response.arrayBuffer());
}

for (const binary of ["evin", "esh"]) {
  const rootUrl = `${baseUrl}/${binary}/${channel}/v${version}`;
  const release = JSON.parse((await get(`${rootUrl}/release.json`)).toString("utf8"));
  if (release.name !== binary || release.version !== version || release.channel !== channel) {
    throw new Error(`${binary} release metadata does not match candidate identity`);
  }
  if (release.git_commit !== commit) {
    throw new Error(`${binary} release metadata has the wrong source commit`);
  }
  const filename = `${binary}-v${version}-${target}.tar.gz`;
  const checksums = await get(`${rootUrl}/SHA256SUMS`);
  const signatureText = (await get(`${rootUrl}/SHA256SUMS.sig`)).toString("utf8").trim();
  const signature = Buffer.from(signatureText, "base64");
  if (!verify(null, checksums, publicKey, signature)) {
    throw new Error(`${binary} checksum signature is invalid`);
  }
  const archive = await get(`${rootUrl}/${filename}`);
  const digest = createHash("sha256").update(archive).digest("hex");
  if (checksums.toString("utf8") !== `${digest}  ${filename}\n`) {
    throw new Error(`${binary} archive does not match its signed checksum manifest`);
  }
  if (
    release.artifacts?.length !== 1 ||
    release.artifacts[0].filename !== filename ||
    release.artifacts[0].sha256 !== digest
  ) {
    throw new Error(`${binary} release metadata does not describe the fetched archive`);
  }
  const directory = resolve(`crates/${binary}/dist/github`);
  await mkdir(directory, { recursive: true });
  await writeFile(resolve(directory, filename), archive);
}

if (pointerFlag === "--check-pointer") {
  const pointer = JSON.parse(
    (await get(`${baseUrl}/${channel}.json?verify=${Date.now()}`)).toString("utf8"),
  );
  if (pointer.channel !== channel || pointer.version !== version || pointer.git_commit !== commit) {
    throw new Error(`${channel} pointer does not identify ${version} at ${commit}`);
  }
  for (const binary of ["evin", "esh"]) {
    if (pointer.binaries?.[binary]?.version !== version) {
      throw new Error(`${channel} pointer does not include ${binary} ${version}`);
    }
  }
}

console.log(`Verified public ${channel} artifacts for Evin and ESH at ${version}`);
