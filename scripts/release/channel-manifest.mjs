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

import { mkdir, readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

const [channel, version, target, mirrorRoot = "dist/mirror"] = process.argv.slice(2);
if (
  !new Set(["unstable", "canary"]).has(channel) ||
  !/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(version ?? "")
) {
  throw new Error(
    "usage: node scripts/release/channel-manifest.mjs <unstable|canary> <version> <target> [mirror-root]",
  );
}
if (target !== "x86_64-unknown-linux-musl") throw new Error(`unsupported release target ${target}`);

const releaseBaseUrl = (process.env.RELEASE_BASE_URL ?? "https://dl.everyinit.dev").replace(
  /\/+$/,
  "",
);
const binaries = {};
let sourceCommit;
for (const binary of ["evin", "esh"]) {
  const releasePath = resolve(mirrorRoot, binary, channel, `v${version}`, "release.json");
  const release = JSON.parse(await readFile(releasePath, "utf8"));
  if (release.name !== binary || release.channel !== channel || release.version !== version) {
    throw new Error(`${binary} release metadata does not match ${channel} ${version}`);
  }
  if (
    release.artifacts?.length !== 1 ||
    release.artifacts[0].target !== target ||
    release.artifacts[0].filename !== `${binary}-v${version}-${target}.tar.gz`
  ) {
    throw new Error(`${binary} release metadata does not describe the expected tar.gz target`);
  }
  if (release.git_commit !== process.env.RELEASE_COMMIT && process.env.RELEASE_COMMIT) {
    throw new Error(`${binary} release metadata has an unexpected source commit`);
  }
  if (sourceCommit !== undefined && release.git_commit !== sourceCommit) {
    throw new Error("Evin and ESH metadata must identify the same source commit");
  }
  sourceCommit = release.git_commit;
  binaries[binary] = {
    version,
    release_url: `${releaseBaseUrl}/${binary}/${channel}/v${version}/release.json`,
  };
}

const manifest = { channel, version, git_commit: sourceCommit, binaries };
const directory = resolve(mirrorRoot, "channels");
await mkdir(directory, { recursive: true });
await writeFile(resolve(directory, `${channel}.json`), `${JSON.stringify(manifest, null, 2)}\n`, {
  mode: 0o644,
});
console.log(`Generated the shared ${channel} channel pointer for Evin and ESH ${version}`);
