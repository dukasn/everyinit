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

const [requestedTag, requestedVersion, expectedCommit, baseCommit, previousTagsBase = baseCommit] =
  process.argv.slice(2);
if (!requestedTag || !requestedVersion || !expectedCommit || !baseCommit || !previousTagsBase) {
  throw new Error(
    "usage: node scripts/release/validate-candidate.mjs <tag> <version> <commit> <base-commit> [previous-tags-base]",
  );
}

function git(args) {
  const result = spawnSync("git", args, { encoding: "utf8" });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(result.stderr.trim() || `git ${args.join(" ")} failed`);
  return result.stdout.trim();
}

function gitTextAt(commit, path) {
  return git(["show", `${commit}:${path}`]);
}

function parseVersion(value) {
  const match = /^(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?$/.exec(value);
  if (!match) throw new Error(`invalid semantic version ${value}`);
  return {
    core: match.slice(1, 4).map(BigInt),
    prerelease: match[4]?.split(".") ?? [],
  };
}

function compareVersions(leftValue, rightValue) {
  const left = parseVersion(leftValue);
  const right = parseVersion(rightValue);
  for (let index = 0; index < 3; index += 1) {
    if (left.core[index] < right.core[index]) return -1;
    if (left.core[index] > right.core[index]) return 1;
  }
  if (left.prerelease.length === 0) return right.prerelease.length === 0 ? 0 : 1;
  if (right.prerelease.length === 0) return -1;
  for (
    let index = 0;
    index < Math.max(left.prerelease.length, right.prerelease.length);
    index += 1
  ) {
    if (left.prerelease[index] === undefined) return -1;
    if (right.prerelease[index] === undefined) return 1;
    const leftPart = left.prerelease[index];
    const rightPart = right.prerelease[index];
    if (leftPart === rightPart) continue;
    const leftNumeric = /^\d+$/.test(leftPart);
    const rightNumeric = /^\d+$/.test(rightPart);
    if (leftNumeric && rightNumeric) return BigInt(leftPart) < BigInt(rightPart) ? -1 : 1;
    if (leftNumeric !== rightNumeric) return leftNumeric ? -1 : 1;
    return leftPart < rightPart ? -1 : 1;
  }
  return 0;
}

const candidateIsBasedOnMain = spawnSync("git", [
  "merge-base",
  "--is-ancestor",
  baseCommit,
  expectedCommit,
]);
if (baseCommit !== expectedCommit && candidateIsBasedOnMain.status !== 0) {
  throw new Error("candidate commit must include the target branch history");
}

const workspaceFlow = gitTextAt(expectedCommit, ".evin/workspace.flow");
const cargoToml = gitTextAt(expectedCommit, "Cargo.toml");
const cargoVersion = /^version\s*=\s*"([^"]+)"/m.exec(cargoToml)?.[1];
const workspaceVersion = /^\s+version:\s*["']?([^"'\s#]+)["']?\s*$/m.exec(workspaceFlow)?.[1];
const version = requestedVersion === "auto" ? workspaceVersion : requestedVersion;
const tag = requestedTag === "auto" ? `v${version}` : requestedTag;
if (!version) throw new Error("workspace release version could not be read from candidate commit");
if (!tag) throw new Error("candidate tag could not be resolved");
if (!/^v\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(tag) || tag !== `v${version}`) {
  throw new Error("candidate tag must be the v-prefixed workspace version");
}
if (git(["cat-file", "-t", `refs/tags/${tag}`]) !== "tag") {
  throw new Error(`${tag} must be an annotated Git tag`);
}
const taggedCommit = git(["rev-parse", `${tag}^{commit}`]);
if (taggedCommit !== expectedCommit) {
  throw new Error(`${tag} points to ${taggedCommit}, expected candidate commit ${expectedCommit}`);
}
if (workspaceVersion !== version) {
  throw new Error(
    `workspace version ${workspaceVersion ?? "<missing>"} does not match tag version ${version}`,
  );
}
if (cargoVersion !== version)
  throw new Error(
    `Cargo workspace version ${cargoVersion ?? "<missing>"} does not match ${version}`,
  );
const changelog = gitTextAt(expectedCommit, "CHANGELOG.md");
if (!/^# Changelog\s*$/m.test(changelog)) {
  throw new Error("candidate changelog is missing the root Changelog heading");
}
const escapedVersion = version.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
if (!new RegExp(`^## \\[v${escapedVersion}\\]$`, "m").test(changelog)) {
  throw new Error(`candidate changelog is missing the heading for v${version}`);
}

const priorTags = git(["tag", "--list", "v*", "--merged", previousTagsBase])
  .split("\n")
  .filter(
    (candidate) =>
      candidate && candidate !== tag && /^v\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(candidate),
  )
  .toSorted((left, right) => compareVersions(left.slice(1), right.slice(1)));
const previousTag = priorTags.at(-1);
if (previousTag) {
  if (compareVersions(version, previousTag.slice(1)) <= 0) {
    throw new Error(`${tag} must advance beyond previous release ${previousTag}`);
  }
}

const result = { tag, version, commit: taggedCommit, previous_tag: previousTag ?? null };
console.log(JSON.stringify(result));
if (process.env.GITHUB_OUTPUT) {
  await appendFile(
    process.env.GITHUB_OUTPUT,
    `tag=${tag}\nversion=${version}\ncommit=${taggedCommit}\nprevious_tag=${previousTag ?? ""}\n`,
  );
}
