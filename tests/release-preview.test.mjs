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

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { runInNewContext } from "node:vm";

const workflow = await readFile(
  new URL("../.github/workflows/release-preview.yml", import.meta.url),
  "utf8",
);
const context = {
  repo: { owner: "test", repo: "repository" },
  payload: { workflow_run: { head_sha: "candidate-head" } },
};

function scriptFor(name) {
  const step = workflow.split(`      - name: ${name}\n`)[1];
  assert.ok(step, `missing workflow step ${name}`);
  const block = step.split("          script: |\n")[1];
  assert.ok(block, `missing script in ${name}`);
  const lines = [];
  for (const line of block.split("\n")) {
    if (line.trim() && !line.startsWith("            ")) break;
    lines.push(line.slice(12));
  }
  return runInNewContext(`(async (github, context, core, process) => {\n${lines.join("\n")}\n})`, {
    Buffer,
  });
}

const discover = scriptFor("Find the pull request and create its required preview check");
const complete = scriptFor("Complete the required unstable preview check");

async function discoverCheck({ labeled = false, changed = false } = {}) {
  const outputs = {};
  const checks = [];
  const pr = {
    number: 8,
    labels: labeled ? [{ name: "release-candidate" }] : [],
    head: { sha: "candidate-head" },
    base: { ref: "main", sha: "base" },
  };
  await discover(
    {
      rest: {
        repos: {
          listPullRequestsAssociatedWithCommit: async () => ({ data: [pr] }),
          getContent: async ({ ref }) => ({
            data: {
              content: Buffer.from(
                `workspace:\n  version: "${changed && ref === pr.head.sha ? "1.0.0-canary.101" : "1.0.0-canary.100"}"\n`,
              ).toString("base64"),
            },
          }),
        },
        checks: {
          create: async (check) => {
            checks.push(check);
            return { data: { id: 123 } };
          },
        },
      },
    },
    context,
    {
      setOutput: (key, value) => {
        outputs[key] = value;
      },
    },
    { env: {} },
  );
  return { outputs, checks };
}

async function finish(candidate, source, job = "success") {
  const updates = [];
  await complete(
    {
      rest: {
        checks: {
          update: async (update) => {
            updates.push(update);
          },
        },
      },
    },
    context,
    {},
    {
      env: { CANDIDATE: candidate, SOURCE_CONCLUSION: source, JOB_STATUS: job, CHECK_ID: "123" },
    },
  );
  return updates;
}

test("ordinary PR keeps its successful check when the candidate build is skipped", async () => {
  const { outputs, checks } = await discoverCheck();
  assert.equal(outputs.candidate, "false");
  assert.equal(checks[0].status, "completed");
  assert.equal(checks[0].conclusion, "success");
  assert.equal(checks[0].output.title, "Not a release candidate");
  for (const source of ["skipped", "success", "failure"]) {
    assert.equal((await finish(outputs.candidate, source)).length, 0);
  }
});

for (const candidate of [{ labeled: true }, { changed: true }]) {
  test(`candidate discovery requires publication: ${JSON.stringify(candidate)}`, async () => {
    const { outputs, checks } = await discoverCheck(candidate);
    assert.equal(outputs.candidate, "true");
    assert.equal(checks[0].status, "in_progress");
    assert.equal(checks[0].conclusion, undefined);
  });
}

for (const [source, job, expected] of [
  ["success", "success", "success"],
  ["skipped", "success", "failure"],
  ["failure", "success", "failure"],
  ["success", "failure", "failure"],
  ["success", "cancelled", "cancelled"],
]) {
  test(`candidate build ${source}, preview ${job}: ${expected}`, async () => {
    const updates = await finish("true", source, job);
    assert.equal(updates.length, 1);
    assert.equal(updates[0].check_run_id, 123);
    assert.equal(updates[0].conclusion, expected);
  });
}
