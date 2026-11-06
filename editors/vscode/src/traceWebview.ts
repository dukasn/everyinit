import * as crypto from "node:crypto";
import * as vscode from "vscode";
import { formatDurationUs, formatRelativeTimestamp } from "./traceModel.js";
import { ParsedTraceReport } from "./types.js";

export interface TraceWebviewActions {
  onRunTask(taskName: string): void;
  onDryRunTask(taskName: string): void;
  onOpenTaskDefinition(taskName: string): void;
  onOpenTraceFile(): void;
  onRefresh(): Promise<void>;
}

interface WebviewInboundMessage {
  type?: unknown;
  taskName?: unknown;
}

export class EveryinitTraceWebviewManager implements vscode.Disposable {
  private panel: vscode.WebviewPanel | null = null;
  private latestReport: ParsedTraceReport | null = null;
  private latestWorkspaceName: string | null = null;
  private selectedTaskName: string | null = null;

  constructor(private readonly actions: TraceWebviewActions) {}

  public updateData(report: ParsedTraceReport | null, workspaceName: string | null): void {
    this.latestReport = report;
    this.latestWorkspaceName = workspaceName;
    if (this.panel) {
      this.render();
    }
  }

  public show(
    report: ParsedTraceReport | null,
    workspaceName: string | null,
    focusTaskName?: string
  ): void {
    this.latestReport = report;
    this.latestWorkspaceName = workspaceName;
    if (focusTaskName) {
      this.selectedTaskName = focusTaskName;
    }

    if (this.panel) {
      this.panel.reveal(vscode.ViewColumn.Active);
      this.render();
      return;
    }

    const panel = vscode.window.createWebviewPanel(
      "everyinit.traceTimeline",
      "Everyinit Execution Trace",
      vscode.ViewColumn.Active,
      {
        enableScripts: true,
        retainContextWhenHidden: true,
      }
    );

    panel.onDidDispose(() => {
      this.panel = null;
    });

    panel.webview.onDidReceiveMessage((raw: WebviewInboundMessage) => {
      if (!raw || typeof raw !== "object" || typeof raw.type !== "string") {
        return;
      }
      const taskName = typeof raw.taskName === "string" ? raw.taskName.trim() : "";
      switch (raw.type) {
        case "runTask":
          if (taskName) this.actions.onRunTask(taskName);
          break;
        case "dryRunTask":
          if (taskName) this.actions.onDryRunTask(taskName);
          break;
        case "openTaskDefinition":
          if (taskName) this.actions.onOpenTaskDefinition(taskName);
          break;
        case "openTraceFile":
          this.actions.onOpenTraceFile();
          break;
        case "refresh":
          void this.actions.onRefresh();
          break;
        case "selectTask":
          this.selectedTaskName = taskName || null;
          break;
        default:
          break;
      }
    });

    this.panel = panel;
    this.render();
  }

  public dispose(): void {
    if (this.panel) {
      this.panel.dispose();
      this.panel = null;
    }
  }

  private render(): void {
    if (!this.panel) {
      return;
    }
    const nonce = crypto.randomBytes(16).toString("base64");
    const report = this.latestReport;
    this.panel.title = this.latestWorkspaceName
      ? `Trace: ${this.latestWorkspaceName}`
      : "Everyinit Execution Trace";
    this.panel.webview.html = buildTraceHtml(
      this.panel.webview,
      nonce,
      report,
      this.latestWorkspaceName,
      this.selectedTaskName
    );
  }
}

export function buildTraceHtml(
  webview: vscode.Webview,
  nonce: string,
  report: ParsedTraceReport | null,
  workspaceName: string | null,
  selectedTaskName: string | null
): string {
  const payload = report
    ? {
        workspaceName,
        filePath: report.filePath,
        modifiedTimeMs: report.modifiedTimeMs,
        relativeUpdated: formatRelativeTimestamp(report.modifiedTimeMs),
        formattedTimestamp: new Date(report.modifiedTimeMs).toLocaleTimeString(),
        wallDurationUs: report.wallDurationUs,
        wallDurationFormatted: formatDurationUs(report.wallDurationUs),
        totalTaskDurationUs: report.totalTaskDurationUs,
        totalTaskDurationFormatted: formatDurationUs(report.totalTaskDurationUs),
        parallelismFactor: report.parallelismFactor,
        threadCount: report.threadIds.length,
        counts: report.counts,
        overallStatus: report.overallStatus,
        selectedTaskName,
        ticks: [0, 0.25, 0.5, 0.75, 1].map((ratio) => ({
          pct: ratio * 100,
          label: formatDurationUs(report.wallDurationUs * ratio),
        })),
        spans: report.spans.map((span) => {
          const leftPct =
            report.wallDurationUs > 0
              ? Math.min(99, Math.max(0, (span.offsetUs / report.wallDurationUs) * 100))
              : 0;
          const maxAllowedWidth = 100 - leftPct;
          const naturalWidthPct =
            report.wallDurationUs > 0
              ? (span.durationUs / report.wallDurationUs) * 100
              : 100;
          const widthPct = Math.min(
            maxAllowedWidth,
            Math.max(naturalWidthPct, Math.min(1.25, maxAllowedWidth))
          );

          return {
            index: span.index,
            name: span.name,
            category: span.category,
            phase: span.phase,
            durationUs: span.durationUs,
            durationFormatted: formatDurationUs(span.durationUs),
            offsetUs: span.offsetUs,
            offsetFormatted: `+${formatDurationUs(span.offsetUs)}`,
            endOffsetFormatted: `+${formatDurationUs(span.endOffsetUs)}`,
            wallSharePct:
              report.wallDurationUs > 0
                ? Number(((span.durationUs / report.wallDurationUs) * 100).toFixed(1))
                : 100,
            cpuSharePct:
              report.totalTaskDurationUs > 0
                ? Number(((span.durationUs / report.totalTaskDurationUs) * 100).toFixed(1))
                : 100,
            leftPct,
            widthPct,
            pid: span.pid,
            tid: span.tid,
            status: span.status,
            args: span.args,
          };
        }),
      }
    : null;

  const serializedData = JSON.stringify(payload).replace(/</g, "\\u003c");

  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src ${webview.cspSource} 'nonce-${nonce}' 'unsafe-inline'; script-src 'nonce-${nonce}';" />
  <title>Everyinit Execution Trace</title>
  <style nonce="${nonce}">
    :root {
      --evin-accent: #1ed4be;
      --evin-accent-dim: rgba(30, 212, 190, 0.16);
      --evin-pass: var(--vscode-testing-iconPassed, #2ea043);
      --evin-fail: var(--vscode-testing-iconFailed, #f85149);
      --evin-warn: var(--vscode-editorWarning-foreground, #d29922);
      --evin-muted: var(--vscode-descriptionForeground, #8b949e);
      --evin-surface: var(--vscode-sideBar-background, rgba(127, 127, 127, 0.06));
      --evin-border: var(--vscode-panel-border, rgba(127, 127, 127, 0.22));
      --evin-hover: var(--vscode-list-hoverBackground, rgba(127, 127, 127, 0.1));
      --evin-selected: var(--vscode-list-activeSelectionBackground, rgba(30, 212, 190, 0.14));
    }

    * {
      box-sizing: border-box;
    }

    body {
      margin: 0;
      padding: 20px 24px 32px;
      font-family: var(--vscode-font-family, system-ui, -apple-system, sans-serif);
      font-size: var(--vscode-font-size, 13px);
      color: var(--vscode-foreground);
      background: var(--vscode-editor-background);
      line-height: 1.45;
    }

    .header {
      display: flex;
      align-items: flex-start;
      justify-content: space-between;
      gap: 16px;
      flex-wrap: wrap;
      margin-bottom: 18px;
    }

    .title-group h1 {
      margin: 0;
      font-size: 18px;
      font-weight: 600;
      display: flex;
      align-items: center;
      gap: 10px;
    }

    .subtitle {
      margin-top: 4px;
      color: var(--evin-muted);
      font-size: 12px;
    }

    .btn-row {
      display: flex;
      gap: 8px;
      flex-wrap: wrap;
    }

    button {
      font-family: inherit;
      font-size: 12px;
      padding: 5px 11px;
      border-radius: 4px;
      border: 1px solid var(--evin-border);
      background: var(--vscode-button-secondaryBackground, var(--evin-surface));
      color: var(--vscode-button-secondaryForeground, var(--vscode-foreground));
      cursor: pointer;
      display: inline-flex;
      align-items: center;
      gap: 6px;
      transition: background 0.1s ease, border-color 0.1s ease;
    }

    button:hover {
      background: var(--vscode-button-secondaryHoverBackground, var(--evin-hover));
    }

    button.primary {
      background: var(--vscode-button-background);
      color: var(--vscode-button-foreground);
      border-color: transparent;
      font-weight: 500;
    }

    button.primary:hover {
      background: var(--vscode-button-hoverBackground);
    }

    .kpi-grid {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(155px, 1fr));
      gap: 10px;
      margin-bottom: 18px;
    }

    .kpi-card {
      padding: 11px 14px;
      border: 1px solid var(--evin-border);
      border-radius: 6px;
      background: var(--evin-surface);
    }

    .kpi-label {
      font-size: 11px;
      text-transform: uppercase;
      letter-spacing: 0.04em;
      color: var(--evin-muted);
      margin-bottom: 4px;
    }

    .kpi-value {
      font-size: 17px;
      font-weight: 600;
      font-variant-numeric: tabular-nums;
    }

    .kpi-sub {
      font-size: 11px;
      color: var(--evin-muted);
      margin-top: 2px;
    }

    .status-pill {
      display: inline-flex;
      align-items: center;
      gap: 5px;
      padding: 2px 8px;
      border-radius: 999px;
      font-size: 11px;
      font-weight: 600;
      text-transform: capitalize;
    }

    .status-success {
      color: var(--evin-pass);
      background: rgba(46, 160, 67, 0.15);
    }

    .status-failed {
      color: var(--evin-fail);
      background: rgba(248, 81, 73, 0.16);
    }

    .status-unverified {
      color: var(--evin-warn);
      background: rgba(210, 153, 34, 0.16);
    }

    .status-cancelled, .status-unknown {
      color: var(--evin-muted);
      background: rgba(139, 148, 158, 0.16);
    }

    .toolbar {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
      flex-wrap: wrap;
      margin-bottom: 12px;
    }

    .toolbar-left, .toolbar-right {
      display: flex;
      align-items: center;
      gap: 8px;
      flex-wrap: wrap;
    }

    input[type="search"], select {
      font-family: inherit;
      font-size: 12px;
      padding: 5px 9px;
      border-radius: 4px;
      border: 1px solid var(--vscode-input-border, var(--evin-border));
      background: var(--vscode-input-background);
      color: var(--vscode-input-foreground);
      outline: none;
    }

    input[type="search"] {
      min-width: 210px;
    }

    input[type="search"]:focus, select:focus {
      border-color: var(--vscode-focusBorder, var(--evin-accent));
    }

    .chip {
      padding: 4px 9px;
      border-radius: 999px;
      font-size: 11px;
      border: 1px solid var(--evin-border);
      background: transparent;
      color: var(--evin-muted);
      cursor: pointer;
    }

    .chip.active {
      color: var(--vscode-foreground);
      border-color: var(--evin-accent);
      background: var(--evin-accent-dim);
      font-weight: 500;
    }

    .waterfall {
      border: 1px solid var(--evin-border);
      border-radius: 6px;
      overflow: hidden;
      background: var(--evin-surface);
    }

    .wf-header, .wf-row {
      display: grid;
      grid-template-columns: minmax(190px, 260px) 1fr 120px;
      align-items: center;
      gap: 14px;
      padding: 8px 14px;
    }

    .wf-header {
      font-size: 11px;
      font-weight: 600;
      color: var(--evin-muted);
      border-bottom: 1px solid var(--evin-border);
      background: var(--vscode-editorWidget-background, var(--evin-surface));
      letter-spacing: 0.03em;
      text-transform: uppercase;
    }

    .wf-ruler {
      position: relative;
      height: 18px;
      line-height: 18px;
      user-select: none;
    }

    .wf-tick {
      position: absolute;
      top: 0;
      font-size: 11px;
      color: var(--evin-muted);
      font-variant-numeric: tabular-nums;
      white-space: nowrap;
    }

    .wf-row {
      border-bottom: 1px solid var(--evin-border);
      border-left: 3px solid transparent;
      cursor: pointer;
      transition: background 0.08s ease, border-left-color 0.08s ease;
    }

    .wf-row:last-child {
      border-bottom: none;
    }

    .wf-row:hover {
      background: var(--evin-hover);
    }

    .wf-row.selected {
      background: var(--evin-selected);
      border-left-color: var(--evin-accent);
    }

    .task-cell {
      display: flex;
      align-items: center;
      gap: 8px;
      min-width: 0;
    }

    .status-dot {
      width: 8px;
      height: 8px;
      border-radius: 50%;
      flex-shrink: 0;
    }

    .dot-success { background: var(--evin-pass); }
    .dot-failed { background: var(--evin-fail); }
    .dot-unverified { background: var(--evin-warn); }
    .dot-cancelled, .dot-unknown { background: var(--evin-muted); }

    .task-name {
      font-family: var(--vscode-editor-font-family, monospace);
      font-size: 12px;
      white-space: nowrap;
      overflow: hidden;
      text-overflow: ellipsis;
    }

    .tid-badge {
      margin-left: auto;
      font-size: 10px;
      padding: 1px 5px;
      border-radius: 3px;
      color: var(--evin-muted);
      border: 1px solid var(--evin-border);
      font-variant-numeric: tabular-nums;
      flex-shrink: 0;
    }

    .track-cell {
      position: relative;
      height: 22px;
      border-radius: 4px;
      background-color: rgba(127, 127, 127, 0.05);
      background-image: linear-gradient(
        to right,
        var(--evin-border) 1px,
        transparent 1px
      );
      background-size: 25% 100%;
      overflow: hidden;
    }

    .bar {
      position: absolute;
      top: 3px;
      height: 16px;
      border-radius: 3px;
      min-width: 4px;
      box-shadow: 0 1px 3px rgba(0, 0, 0, 0.22);
      transition: filter 0.1s ease, box-shadow 0.1s ease;
    }

    .bar:hover {
      filter: brightness(1.2);
      box-shadow: 0 2px 6px rgba(0, 0, 0, 0.35);
    }

    .bar-success {
      background: linear-gradient(90deg, #1ed4be 0%, #2ea043 100%);
    }

    .bar-failed {
      background: linear-gradient(90deg, #f85149 0%, #cf222e 100%);
    }

    .bar-unverified {
      background: linear-gradient(90deg, #d29922 0%, #bb8009 100%);
    }

    .bar-cancelled, .bar-unknown {
      background: linear-gradient(90deg, #8b949e 0%, #6e7681 100%);
    }

    .dur-cell {
      text-align: right;
      font-family: var(--vscode-editor-font-family, monospace);
      font-size: 11.5px;
      font-variant-numeric: tabular-nums;
      white-space: nowrap;
    }

    .dur-share {
      color: var(--evin-muted);
      font-size: 10.5px;
      margin-left: 5px;
    }

    .inspector {
      margin-top: 16px;
      padding: 14px 16px;
      border: 1px solid var(--evin-border);
      border-radius: 6px;
      background: var(--evin-surface);
    }

    .inspector-header {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 12px;
      flex-wrap: wrap;
      margin-bottom: 12px;
    }

    .inspector-title {
      font-family: var(--vscode-editor-font-family, monospace);
      font-size: 14px;
      font-weight: 600;
      display: flex;
      align-items: center;
      gap: 8px;
    }

    .detail-grid {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
      gap: 12px;
    }

    .detail-item {
      font-size: 12px;
    }

    .detail-key {
      color: var(--evin-muted);
      font-size: 11px;
    }

    .detail-val {
      font-family: var(--vscode-editor-font-family, monospace);
      margin-top: 3px;
    }

    .empty-state {
      text-align: center;
      padding: 48px 20px;
      border: 1px dashed var(--evin-border);
      border-radius: 8px;
      color: var(--evin-muted);
    }

    .empty-state h2 {
      color: var(--vscode-foreground);
      font-size: 15px;
      margin: 0 0 8px;
    }
  </style>
</head>
<body>
  <div id="app"></div>
  <script nonce="${nonce}">
    const vscode = acquireVsCodeApi();
    const traceData = ${serializedData};

    const state = {
      query: "",
      statusFilter: "all",
      sortBy: "timeline",
      selectedIndex: resolveInitialSelectedIndex(traceData),
    };

    function resolveInitialSelectedIndex(data) {
      if (!data || !data.spans || data.spans.length === 0) return null;
      if (data.selectedTaskName) {
        const match = data.spans.find((s) => s.name === data.selectedTaskName);
        if (match) return match.index;
      }
      const failed = data.spans.find((s) => s.status === "failed");
      return failed ? failed.index : data.spans[0].index;
    }

    function escapeHtml(text) {
      return String(text)
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;")
        .replace(/"/g, "&quot;");
    }

    function getFilteredSpans() {
      if (!traceData) return [];
      const q = state.query.trim().toLowerCase();
      const filtered = traceData.spans.filter((span) => {
        if (state.statusFilter === "passed" && span.status !== "success") return false;
        if (state.statusFilter === "failed" && span.status !== "failed") return false;
        if (state.statusFilter === "other" && (span.status === "success" || span.status === "failed")) {
          return false;
        }
        if (q && !span.name.toLowerCase().includes(q) && !span.category.toLowerCase().includes(q)) {
          return false;
        }
        return true;
      });

      const sorted = [...filtered];
      if (state.sortBy === "slowest") {
        sorted.sort((a, b) => b.durationUs - a.durationUs);
      } else if (state.sortBy === "thread") {
        sorted.sort((a, b) => a.tid - b.tid || a.offsetUs - b.offsetUs);
      } else if (state.sortBy === "name") {
        sorted.sort((a, b) => a.name.localeCompare(b.name));
      } else {
        sorted.sort((a, b) => a.offsetUs - b.offsetUs || b.durationUs - a.durationUs);
      }
      return sorted;
    }

    function renderApp() {
      const root = document.getElementById("app");
      if (!traceData || traceData.spans.length === 0) {
        root.innerHTML = \`
          <div class="header">
            <div class="title-group">
              <h1>Execution Trace</h1>
              <div class="subtitle">.evin/trace.json</div>
            </div>
            <div class="btn-row">
              <button data-action="refresh">Refresh</button>
            </div>
          </div>
          <div class="empty-state">
            <h2>No execution trace recorded yet</h2>
            <p>Run any Evin task in this workspace to populate <code>.evin/trace.json</code> and inspect its waterfall timeline.</p>
          </div>
        \`;
        bindGlobalButtons();
        return;
      }

      const visibleSpans = getFilteredSpans();
      const selectedSpan =
        traceData.spans.find((s) => s.index === state.selectedIndex) || visibleSpans[0] || null;

      const wsLabel = traceData.workspaceName ? \` · workspace \${escapeHtml(traceData.workspaceName)}\` : "";
      const ticksHtml = traceData.ticks
        .map((t, idx) => {
          let posStyle = \`left: \${t.pct}%; transform: translateX(-50%); text-align: center;\`;
          if (idx === 0) {
            posStyle = "left: 0; transform: none; text-align: left;";
          } else if (idx === traceData.ticks.length - 1) {
            posStyle = "right: 0; left: auto; transform: none; text-align: right;";
          }
          return \`<span class="wf-tick" style="\${posStyle}">\${escapeHtml(t.label)}</span>\`;
        })
        .join("");

      const rowsHtml =
        visibleSpans.length === 0
          ? \`<div style="padding: 24px; text-align: center; color: var(--evin-muted);">No trace spans match the current filter.</div>\`
          : visibleSpans
              .map((span) => {
                const isSelected = selectedSpan && span.index === selectedSpan.index;
                const tooltipText = \`\${escapeHtml(span.name)}: \${escapeHtml(span.durationFormatted)} (\${escapeHtml(span.offsetFormatted)} → \${escapeHtml(span.endOffsetFormatted)})\`;
                return \`
                  <div class="wf-row \${isSelected ? "selected" : ""}" data-span-index="\${span.index}">
                    <div class="task-cell">
                      <span class="status-dot dot-\${escapeHtml(span.status)}"></span>
                      <span class="task-name" title="\${escapeHtml(span.name)}">\${escapeHtml(span.name)}</span>
                      <span class="tid-badge" title="Worker thread \${span.tid}">T\${span.tid}</span>
                    </div>
                    <div class="track-cell">
                      <div class="bar bar-\${escapeHtml(span.status)}" style="left: \${span.leftPct}%; width: \${span.widthPct}%;" title="\${tooltipText}"></div>
                    </div>
                    <div class="dur-cell">
                      \${escapeHtml(span.durationFormatted)}<span class="dur-share">\${span.wallSharePct}%</span>
                    </div>
                  </div>
                \`;
              })
              .join("");

      const inspectorHtml = selectedSpan
        ? \`
          <div class="inspector">
            <div class="inspector-header">
              <div class="inspector-title">
                <span>\${escapeHtml(selectedSpan.name)}</span>
                <span class="status-pill status-\${escapeHtml(selectedSpan.status)}">\${escapeHtml(selectedSpan.status)}</span>
              </div>
              <div class="btn-row">
                <button class="primary" data-action="runTask" data-task="\${escapeHtml(selectedSpan.name)}">▶ Re-run Task</button>
                <button data-action="dryRunTask" data-task="\${escapeHtml(selectedSpan.name)}">Dry-Run</button>
                <button data-action="openTaskDefinition" data-task="\${escapeHtml(selectedSpan.name)}">Open in .flow</button>
              </div>
            </div>
            <div class="detail-grid">
              <div class="detail-item">
                <div class="detail-key">Duration</div>
                <div class="detail-val">\${escapeHtml(selectedSpan.durationFormatted)} (\${selectedSpan.durationUs.toLocaleString()} µs)</div>
              </div>
              <div class="detail-item">
                <div class="detail-key">Timeline Window</div>
                <div class="detail-val">\${escapeHtml(selectedSpan.offsetFormatted)} → \${escapeHtml(selectedSpan.endOffsetFormatted)}</div>
              </div>
              <div class="detail-item">
                <div class="detail-key">Share of Run</div>
                <div class="detail-val">\${selectedSpan.wallSharePct}% wall · \${selectedSpan.cpuSharePct}% cumulative</div>
              </div>
              <div class="detail-item">
                <div class="detail-key">Execution Lane</div>
                <div class="detail-val">cat: \${escapeHtml(selectedSpan.category)} · pid \${selectedSpan.pid} · tid \${selectedSpan.tid}</div>
              </div>
            </div>
          </div>
        \`
        : "";

      root.innerHTML = \`
        <div class="header">
          <div class="title-group">
            <h1>
              Execution Trace
              <span class="status-pill status-\${escapeHtml(traceData.overallStatus)}">\${escapeHtml(traceData.overallStatus)}</span>
            </h1>
            <div class="subtitle">Updated \${escapeHtml(traceData.relativeUpdated)} (\${escapeHtml(traceData.formattedTimestamp)})\${wsLabel}</div>
          </div>
          <div class="btn-row">
            <button data-action="openTraceFile">Open trace.json</button>
            <button data-action="refresh">Refresh</button>
          </div>
        </div>

        <div class="kpi-grid">
          <div class="kpi-card">
            <div class="kpi-label">Wall Time</div>
            <div class="kpi-value">\${escapeHtml(traceData.wallDurationFormatted)}</div>
            <div class="kpi-sub">End-to-end elapsed</div>
          </div>
          <div class="kpi-card">
            <div class="kpi-label">Cumulative Time</div>
            <div class="kpi-value">\${escapeHtml(traceData.totalTaskDurationFormatted)}</div>
            <div class="kpi-sub">\${traceData.parallelismFactor.toFixed(2)}× concurrency factor</div>
          </div>
          <div class="kpi-card">
            <div class="kpi-label">Tasks Recorded</div>
            <div class="kpi-value">\${traceData.counts.total}</div>
            <div class="kpi-sub">\${traceData.counts.passed} passed · \${traceData.counts.failed} failed</div>
          </div>
          <div class="kpi-card">
            <div class="kpi-label">Worker Threads</div>
            <div class="kpi-value">\${traceData.threadCount}</div>
            <div class="kpi-sub">Active execution lanes</div>
          </div>
        </div>

        <div class="toolbar">
          <div class="toolbar-left">
            <input id="search-input" type="search" placeholder="Filter tasks..." value="\${escapeHtml(state.query)}" />
            <button class="chip \${state.statusFilter === "all" ? "active" : ""}" data-status="all">All (\${traceData.counts.total})</button>
            <button class="chip \${state.statusFilter === "passed" ? "active" : ""}" data-status="passed">Passed (\${traceData.counts.passed})</button>
            <button class="chip \${state.statusFilter === "failed" ? "active" : ""}" data-status="failed">Failed (\${traceData.counts.failed})</button>
          </div>
          <div class="toolbar-right">
            <label for="sort-select" style="color: var(--evin-muted); font-size: 11.5px;">Sort:</label>
            <select id="sort-select">
              <option value="timeline" \${state.sortBy === "timeline" ? "selected" : ""}>Timeline (Start)</option>
              <option value="slowest" \${state.sortBy === "slowest" ? "selected" : ""}>Slowest First</option>
              <option value="thread" \${state.sortBy === "thread" ? "selected" : ""}>Worker Thread</option>
              <option value="name" \${state.sortBy === "name" ? "selected" : ""}>Task Name</option>
            </select>
          </div>
        </div>

        <div class="waterfall">
          <div class="wf-header">
            <div>Task</div>
            <div class="wf-ruler">\${ticksHtml}</div>
            <div style="text-align: right;">Duration</div>
          </div>
          \${rowsHtml}
        </div>

        \${inspectorHtml}
      \`;

      bindGlobalButtons();

      const searchInput = document.getElementById("search-input");
      if (searchInput) {
        searchInput.addEventListener("input", (e) => {
          const cursorPos = e.target.selectionStart;
          state.query = e.target.value;
          renderApp();
          const nextInput = document.getElementById("search-input");
          if (nextInput) {
            nextInput.focus();
            nextInput.setSelectionRange(cursorPos, cursorPos);
          }
        });
      }

      const sortSelect = document.getElementById("sort-select");
      if (sortSelect) {
        sortSelect.addEventListener("change", (e) => {
          state.sortBy = e.target.value;
          renderApp();
        });
      }

      document.querySelectorAll("[data-status]").forEach((btn) => {
        btn.addEventListener("click", () => {
          state.statusFilter = btn.getAttribute("data-status") || "all";
          renderApp();
        });
      });

      document.querySelectorAll("[data-span-index]").forEach((row) => {
        row.addEventListener("click", () => {
          const idx = Number(row.getAttribute("data-span-index"));
          state.selectedIndex = idx;
          const span = traceData.spans.find((s) => s.index === idx);
          vscode.postMessage({ type: "selectTask", taskName: span ? span.name : "" });
          renderApp();
        });
      });
    }

    function bindGlobalButtons() {
      document.querySelectorAll("[data-action]").forEach((btn) => {
        btn.addEventListener("click", (e) => {
          e.stopPropagation();
          const type = btn.getAttribute("data-action");
          const taskName = btn.getAttribute("data-task") || "";
          vscode.postMessage({ type, taskName });
        });
      });
    }

    window.addEventListener("keydown", (e) => {
      if (e.target && (e.target.tagName === "INPUT" || e.target.tagName === "SELECT")) {
        return;
      }
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        const visibleSpans = getFilteredSpans();
        if (visibleSpans.length === 0) return;
        const currentIdx = visibleSpans.findIndex((s) => s.index === state.selectedIndex);
        let nextIdx;
        if (e.key === "ArrowDown") {
          nextIdx = currentIdx >= 0 && currentIdx < visibleSpans.length - 1 ? currentIdx + 1 : 0;
        } else {
          nextIdx = currentIdx > 0 ? currentIdx - 1 : visibleSpans.length - 1;
        }
        state.selectedIndex = visibleSpans[nextIdx].index;
        vscode.postMessage({ type: "selectTask", taskName: visibleSpans[nextIdx].name });
        renderApp();
        e.preventDefault();
      }
    });

    renderApp();
  </script>
</body>
</html>`;
}
