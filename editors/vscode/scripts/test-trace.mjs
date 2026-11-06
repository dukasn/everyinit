import assert from "node:assert/strict";
import { resolve } from "node:path";
import test from "node:test";
import * as esbuild from "esbuild";

const vscodeRoot = resolve(import.meta.dirname, "..");

const bundled = await esbuild.build({
  entryPoints: [resolve(vscodeRoot, "src/traceModel.ts")],
  bundle: true,
  write: false,
  format: "esm",
  platform: "node",
  target: "node20",
});

const moduleUrl = `data:text/javascript;base64,${Buffer.from(
  bundled.outputFiles[0].text
).toString("base64")}`;
const { parseTraceDocument, formatDurationUs, normalizeTraceStatus } = await import(moduleUrl);

const flowTasksBundle = await esbuild.build({
  entryPoints: [resolve(vscodeRoot, "src/flowTasks.ts")],
  bundle: true,
  write: false,
  format: "esm",
  platform: "node",
  target: "node20",
  plugins: [
    {
      name: "mock-vscode",
      setup(build) {
        build.onResolve({ filter: /^vscode$/ }, () => ({
          path: "vscode",
          namespace: "mock-vscode",
        }));
        build.onLoad({ filter: /.*/, namespace: "mock-vscode" }, () => ({
          contents:
            "export class EventEmitter { event = () => {}; fire() {} }; export class Range { constructor(a,b,c,d) {} }; export class CodeLens { constructor(r, c) {} };",
          loader: "js",
        }));
      },
    },
  ],
});
const flowTasksUrl = `data:text/javascript;base64,${Buffer.from(
  flowTasksBundle.outputFiles[0].text
).toString("base64")}`;
const { parseFlowDocumentTasks, findFlowTaskAtLine } = await import(flowTasksUrl);

test("parseTraceDocument computes wall duration, parallelism, and task statuses", () => {
  const sample = JSON.stringify({
    traceEvents: [
      {
        name: "lint",
        cat: "task",
        ph: "X",
        ts: 1_000_000,
        dur: 400_000,
        pid: 1,
        tid: 1,
        args: { status: "success" },
      },
      {
        name: "test",
        cat: "task",
        ph: "X",
        ts: 1_100_000,
        dur: 500_000,
        pid: 1,
        tid: 2,
        args: { status: "failed" },
      },
    ],
    displayTimeUnit: "ms",
  });

  const report = parseTraceDocument(sample, "/workspace/.evin/trace.json", 1700000000000);
  assert.ok(report);
  assert.equal(report.wallDurationUs, 600_000);
  assert.equal(report.totalTaskDurationUs, 900_000);
  assert.equal(report.parallelismFactor, 1.5);
  assert.deepEqual(report.threadIds, [1, 2]);
  assert.equal(report.overallStatus, "failed");
  assert.equal(report.counts.total, 2);
  assert.equal(report.counts.passed, 1);
  assert.equal(report.counts.failed, 1);
  assert.equal(report.spans[0].offsetUs, 0);
  assert.equal(report.spans[1].offsetUs, 100_000);
  assert.equal(report.latestByTask.get("test")?.status, "failed");
});

test("parseTraceDocument rejects malformed payloads", () => {
  assert.equal(parseTraceDocument("{bad json", "/tmp/trace.json", 0), null);
  assert.equal(parseTraceDocument("{}", "/tmp/trace.json", 0), null);
});

test("formatDurationUs and normalizeTraceStatus format expected units", () => {
  assert.equal(formatDurationUs(420), "420µs");
  assert.equal(formatDurationUs(254_829), "254.8ms");
  assert.equal(formatDurationUs(2_450_000), "2.45s");
  assert.equal(formatDurationUs(72_300_000), "1m 12.3s");

  assert.equal(normalizeTraceStatus("success"), "success");
  assert.equal(normalizeTraceStatus("FAILED"), "failed");
  assert.equal(normalizeTraceStatus("cancelled"), "cancelled");
  assert.equal(normalizeTraceStatus("unverified"), "unverified");
  assert.equal(normalizeTraceStatus(undefined), "unknown");
});

test("parseFlowDocumentTasks detects root tasks, ranges, stages, and descriptions", () => {
  const content = `#!evin/v1
workspace:
  name: main
stages: [Build, Check]

tasks:
  build:
    stage: Build
    description: "Build binary"
    steps:
      - run: cargo build

  check:
    stage: Check
    run: cargo check
`;

  const tasks = parseFlowDocumentTasks({ getText: () => content });
  assert.equal(tasks.length, 2);
  assert.equal(tasks[0].name, "build");
  assert.equal(tasks[0].shortName, "build");
  assert.equal(tasks[0].module, null);
  assert.equal(tasks[0].stage, "Build");
  assert.equal(tasks[0].description, "Build binary");
  assert.equal(tasks[0].startLine, 6);
  assert.equal(tasks[0].endLine, 11);

  assert.equal(tasks[1].name, "check");
  assert.equal(tasks[1].stage, "Check");
  assert.equal(tasks[1].startLine, 12);

  // findFlowTaskAtLine
  assert.equal(findFlowTaskAtLine(tasks, 0), null);
  assert.equal(findFlowTaskAtLine(tasks, 7)?.name, "build");
  assert.equal(findFlowTaskAtLine(tasks, 11)?.name, "build");
  assert.equal(findFlowTaskAtLine(tasks, 14)?.name, "check");
});

test("parseFlowDocumentTasks detects module qualified tasks", () => {
  const content = `#!evin/v1
module:
  name: vscode

tasks:
  test:
    stage: Test
    run: npm test
`;

  const tasks = parseFlowDocumentTasks({ getText: () => content });
  assert.equal(tasks.length, 1);
  assert.equal(tasks[0].name, "vscode:test");
  assert.equal(tasks[0].shortName, "test");
  assert.equal(tasks[0].module, "vscode");
  assert.equal(tasks[0].stage, "Test");
});


