import { readdir } from "node:fs/promises";
import { resolve, relative, sep } from "node:path";
import { spawnSync } from "node:child_process";

const root = resolve(process.argv[2] ?? "dist/mirror");
const config = "infra/mirror/wrangler.toml";
const bucket = "everyinit-artifacts";

async function filesUnder(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = [];
  for (const entry of entries.sort((a, b) => a.name.localeCompare(b.name))) {
    const path = resolve(directory, entry.name);
    if (entry.isDirectory()) files.push(...(await filesUnder(path)));
    else if (entry.isFile()) files.push(path);
  }
  return files;
}

const files = await filesUnder(root).catch((error) => {
  if (error.code === "ENOENT") {
    throw new Error(`${root} is missing; generate release metadata before seeding local R2`);
  }
  throw error;
});
if (files.length === 0) throw new Error("dist/mirror contains no files to seed");

for (const file of files) {
  const key = relative(root, file).split(sep).join("/");
  const result = spawnSync(
    "npx",
    ["wrangler", "r2", "object", "put", `${bucket}/${key}`, "--file", file, "--local", "--config", config],
    { stdio: "inherit" },
  );
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`Wrangler failed to seed ${key}`);
}

console.log(`Seeded ${files.length} objects into local R2 bucket ${bucket}`);
