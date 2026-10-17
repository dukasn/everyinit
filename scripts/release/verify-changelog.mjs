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

const [path, version] = process.argv.slice(2);
if (!path || !version) {
  throw new Error("usage: node scripts/release/verify-changelog.mjs <generated-changelog> <version>");
}
const changelog = await readFile(path, "utf8");
if (!/^# Changelog\s*$/m.test(changelog)) {
  throw new Error(`${path} is missing the root Changelog heading`);
}
if (!new RegExp(`^## \\[v${escapeRegExp(version)}\\](?:\\s|$)`, "m").test(changelog)) {
  throw new Error(`${path} is missing the generated section for v${version}`);
}
console.log(`Verified generated changelog section for v${version}`);

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
