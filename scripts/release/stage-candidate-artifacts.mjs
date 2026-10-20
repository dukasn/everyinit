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

import { copyFile, mkdir, readdir } from "node:fs/promises";
import { resolve } from "node:path";

const [artifactRoot, version] = process.argv.slice(2);
if (!artifactRoot || !/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(version ?? "")) {
  throw new Error(
    "usage: node scripts/release/stage-candidate-artifacts.mjs <artifact-root> <version>",
  );
}

async function findArchives(directory, expectedName, matches = []) {
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const path = resolve(directory, entry.name);
    if (entry.isDirectory()) await findArchives(path, expectedName, matches);
    else if (entry.isFile() && entry.name === expectedName) matches.push(path);
    else if (!entry.isFile()) throw new Error(`unexpected non-file artifact entry: ${path}`);
  }
  return matches;
}

for (const binary of ["evin", "esh"]) {
  const filename = `${binary}-v${version}-x86_64-unknown-linux-musl.tar.gz`;
  const matches = await findArchives(resolve(artifactRoot), filename);
  if (matches.length !== 1)
    throw new Error(
      `expected exactly one ${filename} in candidate artifacts, found ${matches.length}`,
    );
  const output = resolve(`crates/${binary}/dist/github/${filename}`);
  await mkdir(resolve(output, ".."), { recursive: true });
  await copyFile(matches[0], output);
}
