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

import { readFile } from "node:fs/promises";
import { readReleaseLabel } from "./release-label.mjs";

const [path, version, configuredLabel] = process.argv.slice(2);
const label = configuredLabel ?? (await readReleaseLabel());
if (!path || !version) {
  throw new Error(
    "usage: node scripts/release/verify-changelog.mjs <generated-changelog> <version> [label]",
  );
}
if (!label.trim()) {
  throw new Error("release label must not be empty when provided");
}
const changelog = await readFile(path, "utf8");
if (!/^# Changelog\s*$/m.test(changelog)) {
  throw new Error(`${path} is missing the root Changelog heading`);
}
const heading = new RegExp(`^## \\[v${escapeRegExp(version)}\\] — ${escapeRegExp(label)}$`, "m");
if (!heading.test(changelog)) {
  const expected = `v${version} — ${label}`;
  throw new Error(`${path} is missing the generated section for ${expected}`);
}
console.log(`Verified generated changelog section for v${version} — ${label}`);

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
