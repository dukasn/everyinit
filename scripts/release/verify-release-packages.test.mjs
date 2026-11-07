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

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash, generateKeyPairSync, sign } from "node:crypto";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

const version = "0.2.2";
const target = "x86_64-unknown-linux-musl";
const deb = Buffer.from("!<arch>\n");
const arch = Buffer.from([0x28, 0xb5, 0x2f, 0xfd]);
const artifacts = new Map([
  [`evin-v${version}-${target}.tar.gz`, Buffer.from([0x1f, 0x8b])],
  [`esh-v${version}-${target}.tar.gz`, Buffer.from([0x1f, 0x8b])],
  [`evin_${version}-2_amd64.deb`, deb],
  [`esh_${version}-3_amd64.deb`, deb],
  [`evin-${version}-4-x86_64.pkg.tar.zst`, arch],
  [`esh-${version}-5-x86_64.pkg.tar.zst`, arch],
  [`everyinit-${version}.vsix`, Buffer.from([0x50, 0x4b, 0x03, 0x04])],
]);

async function fixture(t, files = artifacts) {
  const dir = await mkdtemp(join(tmpdir(), "everyinit-release-verify-"));
  t.after(() => rm(dir, { recursive: true, force: true }));
  const manifest = [...files]
    .map(([name, content]) => `${createHash("sha256").update(content).digest("hex")}  ${name}\n`)
    .join("");
  const { privateKey, publicKey } = generateKeyPairSync("ed25519");
  await Promise.all([
    ...[...files].map(([name, content]) => writeFile(join(dir, name), content)),
    writeFile(join(dir, "SHA256SUMS"), manifest),
    writeFile(join(dir, "SHA256SUMS.sig"), sign(null, Buffer.from(manifest), privateKey)),
    writeFile(join(dir, "release-key.pub"), publicKey.export({ type: "spki", format: "pem" })),
  ]);
  return dir;
}

function verify(dir) {
  return spawnSync(
    process.execPath,
    ["scripts/release/verify-release-packages.mjs", dir, version, target],
    { encoding: "utf8" },
  );
}

test("verifies signed packages with independent non-default revisions", async (t) => {
  const result = verify(await fixture(t));
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /Verified all 10 release artifacts/);
});

test("continues to verify revision 1 packages", async (t) => {
  const files = new Map(
    [...artifacts].map(([name, content]) => [
      name.replace(/-(?:2|3)(?=_amd64\.deb$)|-(?:4|5)(?=-x86_64\.pkg\.tar\.zst$)/, "-1"),
      content,
    ]),
  );
  const result = verify(await fixture(t, files));
  assert.equal(result.status, 0, result.stderr);
});

test("rejects ambiguous package revisions", async (t) => {
  const dir = await fixture(t);
  await writeFile(join(dir, `evin_${version}-1_amd64.deb`), deb);
  const result = verify(dir);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /Expected exactly one evin deb package.*found 2/);
});

test("rejects missing packages and invalid revisions", async (t) => {
  const dir = await fixture(t);
  await rm(join(dir, `evin_${version}-2_amd64.deb`));
  await writeFile(join(dir, `evin_${version}-0_amd64.deb`), deb);
  const result = verify(dir);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /Expected exactly one evin deb package.*found 0/);
});

test("checks checksums for resolved package names", async (t) => {
  const dir = await fixture(t);
  await writeFile(join(dir, `evin_${version}-2_amd64.deb`), Buffer.concat([deb, Buffer.from("x")]));
  const result = verify(dir);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /Hash mismatch for evin_0\.2\.2-2_amd64\.deb/);
});

test("rejects a damaged signature", async (t) => {
  const dir = await fixture(t);
  await writeFile(join(dir, "SHA256SUMS.sig"), Buffer.alloc(64));
  const result = verify(dir);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /signature verification failed/);
});
