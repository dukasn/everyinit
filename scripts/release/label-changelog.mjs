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

import { readFile, writeFile } from "node:fs/promises";
import { spawnSync } from "node:child_process";

const [path, version, milestone] = process.argv.slice(2);
if (!path || !version || !milestone) {
  throw new Error("usage: node scripts/release/label-changelog.mjs <changelog> <version> <milestone>");
}

const milestones = [
  "Alpha", "Bravo", "Charlie", "Delta", "Echo", "Foxtrot", "Golf", "Hotel", "India",
  "Juliett", "Kilo", "Lima", "Mike", "November", "Oscar", "Papa", "Quebec", "Romeo",
  "Sierra", "Tango", "Uniform", "Victor", "Whiskey", "X-ray", "Yankee", "Zulu",
];
const match = /^(.+)-(\d+)$/.exec(milestone);
if (!match || !milestones.includes(match[1]) || Number(match[2]) < 1) {
  throw new Error(`invalid canary milestone label: ${milestone}`);
}
const [, currentName, currentNumberText] = match;
const currentNumber = Number(currentNumberText);
const changelog = await readFile(path, "utf8");
const lines = changelog.split(/(?<=\n)/);
const headingIndex = lines.findIndex((line) => line.trim() === `## [v${version}]`);
if (headingIndex < 0) {
  throw new Error(`${path} is missing the generated section for v${version}`);
}
if (lines.some((line) => line.includes(`— ${milestone}`))) {
  throw new Error(`milestone label ${milestone} is already present in ${path}`);
}

const taggedMilestones = await readTaggedMilestones();
const latestPreviousMilestone = [...taggedMilestones.entries()]
  .filter(([tag]) => tag !== `v${version}`)
  .map(([, value]) => value)
  .find(Boolean);
if (!latestPreviousMilestone) {
  if (currentName !== "Alpha" || currentNumber !== 1) {
    throw new Error(`the first named canary milestone must be Alpha-01, got ${milestone}`);
  }
} else {
  const previousMatch = /^(.+)-(\d+)$/.exec(latestPreviousMilestone);
  const previousIndex = previousMatch ? milestones.indexOf(previousMatch[1]) : -1;
  const currentIndex = milestones.indexOf(currentName);
  const previousNumber = Number(previousMatch?.[2]);
  const expected = previousMatch && previousIndex === currentIndex
    ? `${currentName}-${String(previousNumber + 1).padStart(2, "0")}`
    : previousMatch && currentIndex === previousIndex + 1
      ? `${currentName}-01`
      : undefined;
  if (milestone !== expected) {
    throw new Error(`milestone after ${latestPreviousMilestone} must be ${expected ?? "the next NATO milestone"}, got ${milestone}`);
  }
}

for (let index = 0; index < lines.length; index += 1) {
  const heading = /^## \[(v[^\]]+)\]\s*$/.exec(lines[index].trim());
  if (!heading) continue;
  const tag = heading[1];
  const label = tag === `v${version}` ? milestone : taggedMilestones.get(tag);
  if (label) lines[index] = lines[index].replace(heading[0], `${heading[0]} — ${label}`);
}
await writeFile(path, lines.join(""));
console.log(`Labeled v${version} changelog section ${milestone}`);

async function readTaggedMilestones() {
  const tags = spawnSync("git", ["tag", "--merged", "HEAD", "--sort=-version:refname", "--list", "v*"], {
    encoding: "utf8",
  });
  if (tags.error) throw tags.error;
  if (tags.status !== 0) throw new Error(`git tag failed: ${tags.stderr.trim()}`);
  const milestones = new Map();
  for (const tag of tags.stdout.split(/\r?\n/).filter(Boolean)) {
    const result = spawnSync("git", ["for-each-ref", "--format=%(contents)", `refs/tags/${tag}`], {
      encoding: "utf8",
    });
    if (result.error) throw result.error;
    if (result.status !== 0) throw new Error(`could not read annotation for ${tag}: ${result.stderr.trim()}`);
    const label = /\b(Alpha|Bravo|Charlie|Delta|Echo|Foxtrot|Golf|Hotel|India|Juliett|Kilo|Lima|Mike|November|Oscar|Papa|Quebec|Romeo|Sierra|Tango|Uniform|Victor|Whiskey|X-ray|Yankee|Zulu)-(\d+)\b/.exec(result.stdout)?.[0];
    if (label) milestones.set(tag, label);
  }
  return milestones;
}
