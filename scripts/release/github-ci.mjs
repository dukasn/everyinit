// Copyright (C) 2026 Eduardo Marques Santos
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

import { createHash } from "node:crypto";
import { appendFile, mkdir, readFile, readdir, stat, writeFile } from "node:fs/promises";
import { dirname, isAbsolute, resolve, sep } from "node:path";

const root = process.cwd();
const statePath = ".evin/actions/ci/remote-state.v1.json";
const repository = process.env.GITHUB_REPOSITORY;
const token = process.env.GITHUB_TOKEN;
const apiBase = process.env.GITHUB_API_URL ?? "https://api.github.com";
const headers = {
  Accept: "application/vnd.github+json",
  Authorization: `Bearer ${token}`,
  "X-GitHub-Api-Version": "2026-03-10",
};
const formatSize = (bytes) => `${(bytes / (1024 * 1024)).toFixed(2)} MiB`;

async function request(path, options = {}, allowMissing = false) {
  const response = await fetch(path.startsWith("https://") ? path : `${apiBase}${path}`, {
    ...options,
    headers: { ...headers, ...options.headers },
  });
  if (allowMissing && response.status === 404) return undefined;
  if (!response.ok) {
    const detail = await response.text();
    throw new Error(
      `GitHub API ${options.method ?? "GET"} ${path} failed (${response.status}): ${detail}`,
    );
  }
  if (response.status === 204) return undefined;
  return response.json();
}

function repoPath(path) {
  return `/repos/${repository}/${path}`;
}

function normalizeAsset(asset) {
  const digest =
    typeof asset.digest === "string" ? asset.digest.replace(/^sha256:/, "") : undefined;
  return {
    name: asset.name,
    size_bytes: asset.size,
    ...(digest ? { sha256: digest } : {}),
  };
}

async function listReleases() {
  const all = [];
  for (let page = 1; ; page += 1) {
    const pageItems = await request(repoPath(`releases?per_page=100&page=${page}`));
    all.push(...pageItems);
    if (pageItems.length < 100) return all;
  }
}

async function snapshot() {
  const releases = await listReleases();
  const state = {
    $schema: "https://everyinit.dev/schemas/v1/ci-remote-state.json",
    v: 1,
    platform: "github",
    repository,
    releases: releases.map((release) => ({
      tag: release.tag_name,
      title: release.name ?? release.tag_name,
      is_draft: release.draft,
      is_prerelease: release.prerelease,
      body_sha256: createHash("sha256")
        .update(release.body ?? "")
        .digest("hex"),
      ...(release.published_at ? { published_at: release.published_at } : {}),
      assets: release.assets.map(normalizeAsset),
    })),
  };
  await mkdir(dirname(statePath), { recursive: true });
  await writeFile(statePath, `${JSON.stringify(state, null, 2)}\n`);
  process.stdout.write(`Captured ${state.releases.length} GitHub releases for ${repository}\n`);
}

async function findRelease(tag) {
  return request(repoPath(`releases/tags/${encodeURIComponent(tag)}`), {}, true);
}

async function createRelease(action) {
  const notes = action.notes_src ? await readFile(localPath(action.notes_src), "utf8") : "";
  const flags = action.flags ?? {};
  return request(repoPath("releases"), {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      tag_name: action.ref,
      target_commitish: action.commit,
      name: action.title ?? action.ref,
      body: notes,
      draft: flags.draft ?? false,
      prerelease: flags.prerelease ?? false,
    }),
  });
}

async function updateRelease(release, fields) {
  return request(repoPath(`releases/${release.id}`), {
    method: "PATCH",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(fields),
  });
}

function localPath(value) {
  const candidate = isAbsolute(value) ? resolve(value) : resolve(root, value);
  if (candidate !== root && !candidate.startsWith(`${root}${sep}`)) {
    throw new Error(`Plan path escapes the repository: ${value}`);
  }
  return candidate;
}

async function replaceAsset(release, action) {
  const name = action.dest;
  const current = release.assets.find((asset) => asset.name === name);
  if (current) {
    await request(repoPath(`releases/assets/${current.id}`), { method: "DELETE" });
    release.assets = release.assets.filter((asset) => asset.id !== current.id);
  }
  const source = localPath(action.src);
  const content = await readFile(source);
  const uploadUrl = release.upload_url.replace(/\{.*$/, "");
  const separator = uploadUrl.includes("?") ? "&" : "?";
  const uploaded = await request(`${uploadUrl}${separator}name=${encodeURIComponent(name)}`, {
    method: "POST",
    headers: { "Content-Type": "application/octet-stream" },
    body: content,
  });
  release.assets.push(uploaded);
  return uploaded;
}

async function applyPlan(planFile) {
  const plan = JSON.parse(await readFile(resolve(root, planFile), "utf8"));
  if (plan.v !== 1 || !Array.isArray(plan.plan))
    throw new Error(`Unsupported Evin CI plan: ${planFile}`);
  const releases = new Map();
  const getRelease = async (tag) => {
    if (!releases.has(tag)) releases.set(tag, await findRelease(tag));
    return releases.get(tag);
  };
  for (const action of plan.plan) {
    let release;
    switch (action.code) {
      case "SYNC_OK":
        break;
      case "REL_INIT":
        release = await getRelease(action.ref);
        if (!release) {
          release = await createRelease(action);
          releases.set(action.ref, release);
        }
        break;
      case "REL_MUT": {
        release = await getRelease(action.ref);
        if (!release) throw new Error(`Cannot update missing GitHub release ${action.ref}`);
        const fields = {};
        if (action.title != null) fields.name = action.title;
        if (action.flags?.draft != null) fields.draft = action.flags.draft;
        if (action.flags?.prerelease != null) fields.prerelease = action.flags.prerelease;
        releases.set(action.ref, await updateRelease(release, fields));
        break;
      }
      case "REL_NOTES_SYNC": {
        release = await getRelease(action.ref);
        if (!release) throw new Error(`Cannot update notes for missing release ${action.ref}`);
        const body = await readFile(localPath(action.notes_src), "utf8");
        releases.set(action.ref, await updateRelease(release, { body }));
        break;
      }
      case "REL_RETARGET": {
        release = await getRelease(action.ref);
        if (!release) throw new Error(`Cannot retarget missing release ${action.ref}`);
        releases.set(action.ref, await updateRelease(release, { target_commitish: action.commit }));
        break;
      }
      case "REL_PRUNE":
        release = await getRelease(action.ref);
        if (release) await request(repoPath(`releases/${release.id}`), { method: "DELETE" });
        releases.set(action.ref, undefined);
        break;
      case "ART_PUT":
        release = await getRelease(action.ref);
        if (!release) throw new Error(`Cannot upload asset to missing release ${action.ref}`);
        await replaceAsset(release, action);
        break;
      case "ART_PRUNE":
        release = await getRelease(action.ref);
        if (release) {
          const asset = release.assets.find((candidate) => candidate.name === action.dest);
          if (asset) {
            await request(repoPath(`releases/assets/${asset.id}`), { method: "DELETE" });
            release.assets = release.assets.filter((candidate) => candidate.id !== asset.id);
          }
        }
        break;
      default:
        throw new Error(`Unsupported Evin CI plan action: ${action.code}`);
    }
  }
  process.stdout.write(`Applied ${plan.delta_count} Evin CI changes from ${planFile}\n`);
}

async function filesIn(directory) {
  const entries = await readdir(directory, { withFileTypes: true }).catch(() => []);
  return Promise.all(
    entries
      .filter((entry) => entry.isFile())
      .map(async (entry) => {
        const path = resolve(directory, entry.name);
        return { name: entry.name, size: (await stat(path)).size };
      }),
  );
}

async function summary(tag) {
  const release = await findRelease(tag);
  if (!release) throw new Error(`GitHub release ${tag} is missing after reconciliation`);
  const version = tag.replace(/^v/, "");
  const rows = [];
  for (const binary of ["evin", "esh"]) {
    const prefix = `crates/${binary}/dist`;
    const archives = (await filesIn(`${prefix}/github`)).filter(
      (file) => file.name.includes(`-v${version}-`) && file.name.endsWith(".tar.gz"),
    );
    if (archives.length !== 1)
      throw new Error(`Expected one ${binary} tar.gz for ${tag}, found ${archives.length}`);
    const archive = archives[0];
    const packages = await filesIn(`${prefix}/packages`);
    const published = release.assets.filter((asset) => archive && asset.name === archive.name);
    for (const extension of [".deb", ".apk", ".pkg.tar.zst"]) {
      const count = packages.filter(
        (file) => file.name.includes(version) && file.name.endsWith(extension),
      ).length;
      if (count !== 1)
        throw new Error(`Expected one ${binary} ${extension} for ${tag}, found ${count}`);
    }
    rows.push({ binary, archive, packages, published });
  }
  const expectedAssets = rows.map((row) => row.archive.name).toSorted();
  const actualAssets = release.assets.map((asset) => asset.name).toSorted();
  if (JSON.stringify(actualAssets) !== JSON.stringify(expectedAssets)) {
    throw new Error(
      `Expected only GitHub tar.gz assets ${expectedAssets.join(", ")}; found ${actualAssets.join(", ")}`,
    );
  }
  const lines = [
    `## Linux canary release ${tag}`,
    "",
    "| Workspace | Built and verified | GitHub Release assets |",
    "| --- | --- | --- |",
  ];
  for (const row of rows) {
    const built = [row.archive && `${row.archive.name} (${formatSize(row.archive.size)})`];
    for (const ext of [".deb", ".apk", ".pkg.tar.zst"]) {
      const file = row.packages.find((item) => item.name.endsWith(ext));
      if (file) built.push(`${file.name} (${formatSize(file.size)})`);
    }
    const uploaded = row.published.map((asset) => `${asset.name} (${formatSize(asset.size)})`);
    lines.push(
      `| ${row.binary} | ${built.filter(Boolean).join("<br>")} | ${uploaded.join("<br>") || "None"} |`,
    );
  }
  lines.push(
    "",
    "Native package formats are built and verified for CI; only the `.tar.gz` archives are published in this rollout.",
    "",
  );
  const summaryPath = process.env.GITHUB_STEP_SUMMARY;
  if (summaryPath) await appendFile(summaryPath, `${lines.join("\n")}\n`);
  else process.stdout.write(`${lines.join("\n")}\n`);
}

if (!repository || !token) throw new Error("GITHUB_REPOSITORY and GITHUB_TOKEN are required");
const [command, ...args] = process.argv.slice(2);
if (command === "snapshot") {
  await snapshot();
} else if (command === "apply" && args[0]) {
  await applyPlan(args[0]);
} else if (command === "summary" && args[0]) {
  await summary(args[0]);
} else {
  throw new Error("usage: github-ci.mjs snapshot|apply <plan-file>|summary <tag>");
}
