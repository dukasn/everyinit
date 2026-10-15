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

const cargo = await readFile("Cargo.toml", "utf8");
const flow = await readFile(".evin/workspace.flow", "utf8");
const cargoVersion = /^version\s*=\s*"([^"]+)"/m.exec(cargo)?.[1];
const flowVersion = /^\s+version:\s*"([^"]+)"/m.exec(flow)?.[1];
if (!cargoVersion || !flowVersion || cargoVersion !== flowVersion) {
  throw new Error(
    `workspace versions do not match (Cargo.toml=${cargoVersion ?? "missing"}, .evin/workspace.flow=${flowVersion ?? "missing"})`,
  );
}
if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(cargoVersion)) {
  throw new Error(`workspace version is not a supported release version: ${cargoVersion}`);
}
process.stdout.write(`${cargoVersion}\n`);
