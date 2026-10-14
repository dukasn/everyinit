import { readdir } from "node:fs/promises";
import { resolve, relative, sep } from "node:path";
import { spawnSync } from "node:child_process";

const roots = (process.argv.length > 2 ? process.argv.slice(2) : ["dist/mirror"]).map((path) =>
  resolve(path),
);
const config = "infra/mirror/wrangler.toml";
const bucket = "everyinit-artifacts";

async function filesUnder(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = [];
  for (const entry of entries.toSorted((a, b) => a.name.localeCompare(b.name))) {
    const path = resolve(directory, entry.name);
    if (entry.isDirectory()) files.push(...(await filesUnder(path)));
    else if (entry.isFile()) files.push(path);
  }
  return files;
}

let count = 0;
for (const root of roots) {
  const files = await filesUnder(root).catch((error) => {
    if (error.code === "ENOENT") {
      throw new Error(`${root} is missing; generate release metadata before seeding local R2`);
    }
    throw error;
  });
  for (const file of files) {
    const key = relative(root, file).split(sep).join("/");
    const result = spawnSync(
      "npx",
      [
        "wrangler",
        "r2",
        "object",
        "put",
        `${bucket}/${key}`,
        "--file",
        file,
        "--local",
        "--config",
        config,
      ],
      { stdio: "inherit" },
    );
    if (result.error) throw result.error;
    if (result.status !== 0) throw new Error(`Wrangler failed to seed ${key}`);
    count += 1;
  }
}

if (count === 0)
  throw new Error(
    "No mirror objects were found; generate release metadata before seeding local R2",
  );
console.log(`Seeded ${count} objects into local R2 bucket ${bucket}`);
