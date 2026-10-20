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

import { appendFile } from "node:fs/promises";
import { spawnSync } from "node:child_process";

const [before, after] = process.argv.slice(2);
if (!before || !after)
  throw new Error("usage: node scripts/release/find-merged-candidate.mjs <before-sha> <after-sha>");

function git(args) {
  const result = spawnSync("git", args, { encoding: "utf8" });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(result.stderr.trim() || `git ${args.join(" ")} failed`);
  }
  return result.stdout.trim();
}

const commits = git(["rev-list", after, ...(before.match(/^0+$/) ? [] : [`^${before}`])])
  .split("\n")
  .filter(Boolean);
const newCommits = new Set(commits);
const introduced = new Set();
for (const tag of git(["tag", "--list", "v*"]).split("\n").filter(Boolean)) {
  if (!/^v\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(tag)) continue;
  const target = git(["rev-parse", `${tag}^{commit}`]);
  if (newCommits.has(target)) introduced.add(tag);
}
if (introduced.size > 1) {
  throw new Error(
    `main update contains multiple release tags: ${[...introduced].toSorted().join(", ")}`,
  );
}

const tag = [...introduced][0];
const commit = tag ? git(["rev-parse", `${tag}^{commit}`]) : "";
const result = { found: Boolean(tag), tag: tag ?? "", version: tag?.slice(1) ?? "", commit };
console.log(JSON.stringify(result));
if (process.env.GITHUB_OUTPUT) {
  await appendFile(
    process.env.GITHUB_OUTPUT,
    `found=${result.found}\ntag=${result.tag}\nversion=${result.version}\ncommit=${commit}\n`,
  );
}
