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
import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";

const snapshotPath = resolve("target/release-rehearsal/source-status.before");
const result = spawnSync("git", ["status", "--porcelain"], {
  encoding: "utf8",
});
if (result.error) throw result.error;
if (result.status !== 0) {
  throw new Error(`git status failed: ${result.stderr.trim()}`);
}
if (process.argv[2] === "--snapshot") {
  await mkdir(dirname(snapshotPath), { recursive: true });
  await writeFile(snapshotPath, result.stdout);
  console.log("Captured source status before distribution rehearsal");
} else {
  const before = await readFile(snapshotPath, "utf8");
  if (before !== result.stdout) {
    throw new Error(
      `distribution rehearsal changed repository files:\nbefore:\n${before || "(clean)\n"}after:\n${result.stdout || "(clean)\n"}`,
    );
  }
  console.log("Distribution rehearsal left repository files unchanged");
}
