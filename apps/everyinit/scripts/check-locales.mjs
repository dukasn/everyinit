import { readdir, readFile } from "node:fs/promises";

import { FluentBundle, FluentResource } from "@fluent/bundle";
import { parse as parseFluent } from "@fluent/syntax";

const catalogDirectory = new URL("../src/i18n/catalogs/", import.meta.url);
const sourceDirectory = new URL("../src/", import.meta.url);
const entries = await readdir(catalogDirectory, { withFileTypes: true });
const catalogFiles = entries
  .filter((entry) => entry.isFile() && entry.name.endsWith(".ftl"))
  .map((entry) => entry.name)
  .sort();

if (!catalogFiles.includes("en.ftl")) {
  throw new Error("The Astro UI localization catalog must include en.ftl");
}

const catalogs = new Map();
for (const file of catalogFiles) {
  const source = await readFile(new URL(file, catalogDirectory), "utf8");
  const syntax = parseFluent(source, { withSpans: true });
  const resource = new FluentResource(source);
  const ids = new Set(
    syntax.body.filter((entry) => entry.type === "Message").map((entry) => entry.id.name),
  );
  catalogs.set(file.slice(0, -".ftl".length), { resource, ids, syntax, source });
}

const englishIds = catalogs.get("en").ids;
let failed = false;

async function sourceFiles(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = await Promise.all(
    entries.map(async (entry) => {
      const path = new URL(`${entry.name}${entry.isDirectory() ? "/" : ""}`, directory);
      if (entry.isDirectory()) return sourceFiles(path);
      return entry.name.endsWith(".astro") || entry.name.endsWith(".ts") ? [path] : [];
    }),
  );
  return files.flat();
}

for (const file of await sourceFiles(sourceDirectory)) {
  const source = await readFile(file, "utf8");
  for (const match of source.matchAll(/\bt\(["']([^"']+)["']/g)) {
    if (!englishIds.has(match[1])) {
      failed = true;
      console.error(`${file.pathname}: no English Fluent message '${match[1]}'`);
    }
  }
}

for (const [locale, { resource, ids, syntax, source }] of catalogs) {
  try {
    if (new Intl.Locale(locale).toString() !== locale) {
      failed = true;
      console.error(`${locale}.ftl must use its canonical BCP 47 locale spelling`);
    }
  } catch {
    failed = true;
    console.error(`${locale}.ftl has an invalid BCP 47 locale name`);
  }

  for (const entry of syntax.body) {
    if (entry.type !== "Junk") continue;
    failed = true;
    const beforeError = source.slice(0, entry.span?.start ?? 0);
    const line = beforeError.split("\n").length;
    const column = beforeError.length - beforeError.lastIndexOf("\n");
    for (const annotation of entry.annotations) {
      console.error(`${locale}.ftl:${line}:${column}: ${annotation.message}`);
    }
  }

  const bundle = new FluentBundle(locale, { useIsolating: false });
  const parseErrors = bundle.addResource(resource);
  if (parseErrors.length) {
    failed = true;
    for (const error of parseErrors) {
      console.error(`${locale}.ftl: ${error.message}`);
    }
  }

  if (locale === "en") continue;

  const missing = [...englishIds].filter((id) => !ids.has(id)).sort();
  const extra = [...ids].filter((id) => !englishIds.has(id)).sort();
  if (missing.length) {
    failed = true;
    console.error(`${locale}.ftl is missing English messages: ${missing.join(", ")}`);
  }
  if (extra.length) {
    failed = true;
    console.error(`${locale}.ftl has unknown messages: ${extra.join(", ")}`);
  }
}

if (failed) process.exitCode = 1;
else console.log(`Validated ${catalogFiles.length} Astro Fluent catalog(s).`);
