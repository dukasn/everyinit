export type TraceLevel = "off" | "messages" | "verbose";

export interface EveryinitConfig {
  evinEnable: boolean;
  evinPath: string;
  eshEnable: boolean;
  eshPath: string;
  trace: TraceLevel;
}

export type SupportedLanguage = "flow" | "esh" | "json";

export interface LspPosition {
  line: number;
  character: number;
}

export interface LspRange {
  start: LspPosition;
  end: LspPosition;
}

export interface LspLocation {
  uri: string;
  range: LspRange;
}

export interface CatalogDiagnosticCounts {
  errors: number;
  warnings: number;
}

export interface CatalogManifestItem {
  path: string;
  uri: string;
  kind: "workspace" | "module" | "namespace" | "policy" | "environment" | "unknown" | string;
  errors: number;
  warnings: number;
}

export interface CatalogTaskItem {
  name: string;
  shortName: string;
  module: string | null;
  description: string | null;
  stage: string | null;
  needs: string[];
  hooks: string[];
  hasWatch: boolean;
  watchPaths: string[];
  presets: string[];
  exposed: boolean;
  sourcePath: string;
  location: LspLocation | null;
}

export interface WorkspaceCatalogResult {
  workspaceName: string | null;
  workspaceVersion: string | null;
  stages: string[];
  stale: boolean;
  diagnostics: CatalogDiagnosticCounts;
  manifests: CatalogManifestItem[];
  tasks: CatalogTaskItem[];
}

export type TraceTaskStatus = "success" | "failed" | "cancelled" | "unverified" | "unknown";

export interface RawTraceEvent {
  name?: unknown;
  cat?: unknown;
  ph?: unknown;
  ts?: unknown;
  dur?: unknown;
  pid?: unknown;
  tid?: unknown;
  args?: unknown;
}

export interface RawTraceDocument {
  otherData?: unknown;
  traceEvents?: unknown;
  displayTimeUnit?: unknown;
}

export interface ParsedTraceStep {
  name: string;
  kind: string;
  index: number;
  durationUs: number;
  status: TraceTaskStatus;
  command?: string;
  action?: string;
  version?: string;
  issue?: string;
}

export interface ParsedTraceSpan {
  index: number;
  name: string;
  category: string;
  phase: string;
  startUs: number;
  durationUs: number;
  offsetUs: number;
  endOffsetUs: number;
  wallShare: number;
  pid: number;
  tid: number;
  status: TraceTaskStatus;
  args: Record<string, string>;
  source?: string;
  steps?: ParsedTraceStep[];
}

export interface ParsedTraceCounts {
  total: number;
  passed: number;
  failed: number;
  cancelled: number;
  unverified: number;
}

export interface ParsedTraceReport {
  filePath: string;
  modifiedTimeMs: number;
  wallDurationUs: number;
  totalTaskDurationUs: number;
  parallelismFactor: number;
  threadIds: number[];
  counts: ParsedTraceCounts;
  overallStatus: TraceTaskStatus;
  spans: ParsedTraceSpan[];
  latestByTask: ReadonlyMap<string, ParsedTraceSpan>;
  runId?: string;
  scope?: { kind: string; module?: string; root?: string };
  workspaceTrace?: string;
  command?: string;
  targets?: string[];
  context?: string;
  outcome?: string;
  modules?: string[];
}
