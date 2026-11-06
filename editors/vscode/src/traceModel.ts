import * as fs from "node:fs/promises";
import * as path from "node:path";
import {
  ParsedTraceCounts,
  ParsedTraceReport,
  ParsedTraceSpan,
  ParsedTraceStep,
  RawTraceDocument,
  RawTraceEvent,
  TraceTaskStatus,
} from "./types.js";

export const TRACE_RELATIVE_PATH = path.join(".evin", "trace.json");

export async function loadWorkspaceTraceReport(
  workspaceRoot: string | undefined
): Promise<ParsedTraceReport | null> {
  if (!workspaceRoot) {
    return null;
  }
  const tracePath = path.join(workspaceRoot, TRACE_RELATIVE_PATH);
  try {
    const [stats, rawContent] = await Promise.all([
      fs.stat(tracePath),
      fs.readFile(tracePath, "utf8"),
    ]);
    if (!stats.isFile()) {
      return null;
    }
    return parseTraceDocument(rawContent, tracePath, stats.mtimeMs);
  } catch {
    return null;
  }
}

export function parseTraceDocument(
  rawJson: string,
  filePath: string,
  modifiedTimeMs: number
): ParsedTraceReport | null {
  let parsed: RawTraceDocument;
  try {
    parsed = JSON.parse(rawJson) as RawTraceDocument;
  } catch {
    return null;
  }

  if (!parsed || typeof parsed !== "object" || !Array.isArray(parsed.traceEvents)) {
    return null;
  }

  let runId: string | undefined;
  let scope: { kind: string; module?: string; root?: string } | undefined;
  let workspaceTrace: string | undefined;
  let command: string | undefined;
  let targets: string[] | undefined;
  let context: string | undefined;
  let outcome: string | undefined;
  let modules: string[] | undefined;

  if (parsed.otherData && typeof parsed.otherData === "object") {
    const od = parsed.otherData as Record<string, unknown>;
    if (typeof od.runId === "string") runId = od.runId;
    if (typeof od.command === "string") command = od.command;
    if (Array.isArray(od.targets)) targets = od.targets.map(String);
    if (typeof od.context === "string") context = od.context;
    if (typeof od.outcome === "string") outcome = od.outcome;
    if (typeof od.workspaceTrace === "string") workspaceTrace = od.workspaceTrace;
    if (Array.isArray(od.modules)) modules = od.modules.map(String);
    if (od.scope && typeof od.scope === "object") {
      const s = od.scope as Record<string, unknown>;
      scope = {
        kind: String(s.kind ?? "workspace"),
        module: typeof s.module === "string" ? s.module : undefined,
        root: typeof s.root === "string" ? s.root : undefined,
      };
    }
  }

  const stepsByTask = new Map<string, ParsedTraceStep[]>();

  const rawSpans: Array<{
    name: string;
    category: string;
    phase: string;
    startUs: number;
    durationUs: number;
    pid: number;
    tid: number;
    status: TraceTaskStatus;
    args: Record<string, string>;
    source?: string;
  }> = [];

  for (const item of parsed.traceEvents as RawTraceEvent[]) {
    if (!item || typeof item !== "object" || typeof item.name !== "string") {
      continue;
    }
    const phase = typeof item.ph === "string" && item.ph ? item.ph : "X";
    if (phase === "M" || phase === "s" || phase === "f") {
      continue;
    }

    const startUs = toNonNegativeNumber(item.ts);
    const durationUs = toNonNegativeNumber(item.dur);
    const pid = Math.max(1, Math.trunc(toNonNegativeNumber(item.pid) || 1));
    const tid = Math.max(1, Math.trunc(toNonNegativeNumber(item.tid) || 1));
    const category = typeof item.cat === "string" && item.cat ? item.cat : "task";

    const argsRecord: Record<string, string> = {};
    if (item.args && typeof item.args === "object" && !Array.isArray(item.args)) {
      for (const [key, value] of Object.entries(item.args)) {
        if (value !== undefined && value !== null) {
          argsRecord[key] = String(value);
        }
      }
    }

    if (category === "step") {
      const parentTask = argsRecord.task || "";
      if (parentTask) {
        const step: ParsedTraceStep = {
          name: item.name,
          kind: argsRecord.kind || "step",
          index: argsRecord.index ? Number(argsRecord.index) : 1,
          durationUs,
          status: normalizeTraceStatus(argsRecord.status),
          command: argsRecord.command,
          action: argsRecord.action,
          version: argsRecord.version,
          issue: argsRecord.issue,
        };
        const existing = stepsByTask.get(parentTask) || [];
        existing.push(step);
        stepsByTask.set(parentTask, existing);
      }
      continue;
    }

    if (category !== "task") {
      continue;
    }

    rawSpans.push({
      name: item.name,
      category,
      phase,
      startUs,
      durationUs,
      pid,
      tid,
      status: normalizeTraceStatus(argsRecord.status),
      args: argsRecord,
      source: argsRecord.source,
    });
  }

  rawSpans.sort((a, b) => a.startUs - b.startUs || b.durationUs - a.durationUs);

  const minStartUs = rawSpans.length > 0 ? rawSpans[0].startUs : 0;
  let maxEndUs = minStartUs;
  let totalTaskDurationUs = 0;

  for (const span of rawSpans) {
    const endUs = span.startUs + span.durationUs;
    if (endUs > maxEndUs) {
      maxEndUs = endUs;
    }
    totalTaskDurationUs += span.durationUs;
  }

  const wallDurationUs = Math.max(0, maxEndUs - minStartUs);
  const counts: ParsedTraceCounts = {
    total: rawSpans.length,
    passed: 0,
    failed: 0,
    cancelled: 0,
    unverified: 0,
  };

  const threadSet = new Set<number>();
  const latestByTask = new Map<string, ParsedTraceSpan>();

  const spans: ParsedTraceSpan[] = rawSpans.map((span, index) => {
    const offsetUs = Math.max(0, span.startUs - minStartUs);
    const endOffsetUs = offsetUs + span.durationUs;
    const wallShare = wallDurationUs > 0 ? Math.min(1, span.durationUs / wallDurationUs) : 1;

    threadSet.add(span.tid);
    switch (span.status) {
      case "success":
        counts.passed += 1;
        break;
      case "failed":
        counts.failed += 1;
        break;
      case "cancelled":
        counts.cancelled += 1;
        break;
      case "unverified":
        counts.unverified += 1;
        break;
      default:
        break;
    }

    const taskSteps = (stepsByTask.get(span.name) || []).sort((a, b) => a.index - b.index);

    const parsedSpan: ParsedTraceSpan = {
      index,
      name: span.name,
      category: span.category,
      phase: span.phase,
      startUs: span.startUs,
      durationUs: span.durationUs,
      offsetUs,
      endOffsetUs,
      wallShare,
      pid: span.pid,
      tid: span.tid,
      status: span.status,
      args: span.args,
      source: span.source,
      steps: taskSteps,
    };

    latestByTask.set(span.name, parsedSpan);
    return parsedSpan;
  });

  const threadIds = [...threadSet].sort((a, b) => a - b);
  const parallelismFactor =
    wallDurationUs > 0 ? Math.max(1, totalTaskDurationUs / wallDurationUs) : 1;

  let overallStatus: TraceTaskStatus = "unknown";
  if (counts.failed > 0) {
    overallStatus = "failed";
  } else if (counts.cancelled > 0) {
    overallStatus = "cancelled";
  } else if (counts.unverified > 0) {
    overallStatus = "unverified";
  } else if (counts.passed > 0) {
    overallStatus = "success";
  }

  return {
    filePath,
    modifiedTimeMs,
    wallDurationUs,
    totalTaskDurationUs,
    parallelismFactor,
    threadIds,
    counts,
    overallStatus,
    spans,
    latestByTask,
    runId,
    scope,
    workspaceTrace,
    command,
    targets,
    context,
    outcome,
    modules,
  };
}

export function normalizeTraceStatus(raw: string | undefined): TraceTaskStatus {
  switch (raw?.trim().toLowerCase()) {
    case "success":
    case "ok":
    case "passed":
      return "success";
    case "failed":
    case "error":
      return "failed";
    case "cancelled":
    case "canceled":
      return "cancelled";
    case "unverified":
      return "unverified";
    default:
      return "unknown";
  }
}

export function formatDurationUs(durationUs: number): string {
  if (!Number.isFinite(durationUs) || durationUs <= 0) {
    return "0µs";
  }
  if (durationUs < 1_000) {
    return `${Math.round(durationUs)}µs`;
  }
  const ms = durationUs / 1_000;
  if (ms < 1_000) {
    return `${ms.toFixed(1)}ms`;
  }
  const seconds = ms / 1_000;
  if (seconds < 60) {
    return `${seconds.toFixed(2)}s`;
  }
  const minutes = Math.floor(seconds / 60);
  const remSeconds = seconds - minutes * 60;
  return `${minutes}m ${remSeconds.toFixed(1)}s`;
}

export function formatRelativeTimestamp(mtimeMs: number, nowMs = Date.now()): string {
  const deltaSec = Math.max(0, Math.floor((nowMs - mtimeMs) / 1_000));
  if (deltaSec < 5) {
    return "just now";
  }
  if (deltaSec < 60) {
    return `${deltaSec}s ago`;
  }
  const deltaMin = Math.floor(deltaSec / 60);
  if (deltaMin < 60) {
    return `${deltaMin}m ago`;
  }
  const deltaHours = Math.floor(deltaMin / 60);
  if (deltaHours < 24) {
    return `${deltaHours}h ago`;
  }
  return new Date(mtimeMs).toLocaleDateString();
}

function toNonNegativeNumber(value: unknown): number {
  const num = typeof value === "number" ? value : Number(value);
  return Number.isFinite(num) && num > 0 ? num : 0;
}

