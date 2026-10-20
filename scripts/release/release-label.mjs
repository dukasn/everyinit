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

export async function readReleaseLabel(path = ".evin/workspace.flow") {
  return parseReleaseLabel(await readFile(path, "utf8"));
}

export function parseReleaseLabel(flow, { required = true } = {}) {
  const valuesIndex = flow.search(/^values:\s*$/m);
  if (valuesIndex < 0) {
    if (!required) return null;
    throw new Error("release configuration is missing its values section");
  }

  const afterValues = flow.slice(valuesIndex).split("\n").slice(1);
  const valuesLines = [];
  for (const line of afterValues) {
    if (line && !/^\s/.test(line)) break;
    valuesLines.push(line);
  }

  const line = valuesLines.find((value) => /^\s+release_label\s*:\s*/.test(value));
  if (!line) {
    if (!required) return null;
    throw new Error(
      "release_label must be configured in .evin/workspace.flow under top-level values",
    );
  }

  const raw = line
    .replace(/^\s+release_label\s*:\s*/, "")
    .replace(/\s+#.*$/, "")
    .trim();
  const quoted = /^(?:"([^"]*)"|'([^']*)')$/.exec(raw);
  const label = quoted ? (quoted[1] ?? quoted[2]) : raw;
  const containsControlCharacter = [...label].some((character) => {
    const codePoint = character.codePointAt(0);
    return codePoint < 0x20 || (codePoint >= 0x7f && codePoint <= 0x9f);
  });
  if (!label || !label.trim() || containsControlCharacter) {
    throw new Error("release_label must be a nonempty, single-line value");
  }
  return label.trim();
}
