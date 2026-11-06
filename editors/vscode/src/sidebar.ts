import * as fs from "node:fs";
import * as path from "node:path";
import * as vscode from "vscode";
import { EveryinitClientManager } from "./client.js";
import {
  executeEvinCli,
  extractTaskNameArg,
  openLocationInEditor,
} from "./commands.js";
import {
  formatDurationUs,
  formatRelativeTimestamp,
  loadWorkspaceTraceReport,
  TRACE_RELATIVE_PATH,
} from "./traceModel.js";
import { EveryinitTraceWebviewManager } from "./traceWebview.js";
import {
  CatalogManifestItem,
  CatalogTaskItem,
  ParsedTraceReport,
  ParsedTraceSpan,
  TraceTaskStatus,
  WorkspaceCatalogResult,
} from "./types.js";

export type TaskGroupingMode = "module" | "stage";
export type TraceSortMode = "timeline" | "slowest";

const WORKSPACE_GROUPING_KEY = "everyinit.sidebar.taskGrouping";
const WORKSPACE_TRACE_SORT_KEY = "everyinit.sidebar.traceSort";

export class EveryinitSidebarController implements vscode.Disposable {
  private readonly disposables: vscode.Disposable[] = [];
  private readonly tasksProvider: TasksTreeProvider;
  private readonly traceProvider: TraceTreeProvider;
  private readonly workspaceProvider: WorkspaceTreeProvider;
  private readonly traceWebview: EveryinitTraceWebviewManager;

  private readonly tasksView: vscode.TreeView<TaskTreeNode>;
  private readonly traceView: vscode.TreeView<TraceTreeNode>;
  private readonly workspaceView: vscode.TreeView<WorkspaceTreeNode>;

  private catalog: WorkspaceCatalogResult | null = null;
  private traceReport: ParsedTraceReport | null = null;
  private eshScripts: vscode.Uri[] = [];
  private taskGrouping: TaskGroupingMode;
  private taskFilter = "";
  private traceSort: TraceSortMode;
  private initializedCatalog = false;
  private refreshPromise: Promise<void> | null = null;

  constructor(
    private readonly context: vscode.ExtensionContext,
    private readonly clientManager: EveryinitClientManager
  ) {
    this.taskGrouping = context.workspaceState.get<TaskGroupingMode>(
      WORKSPACE_GROUPING_KEY,
      "module"
    );
    this.traceSort = context.workspaceState.get<TraceSortMode>(
      WORKSPACE_TRACE_SORT_KEY,
      "timeline"
    );

    this.tasksProvider = new TasksTreeProvider(() => ({
      catalog: this.catalog,
      traceReport: this.traceReport,
      grouping: this.taskGrouping,
      filter: this.taskFilter,
    }));

    this.traceProvider = new TraceTreeProvider(() => ({
      traceReport: this.traceReport,
      sortMode: this.traceSort,
    }));

    this.workspaceProvider = new WorkspaceTreeProvider(() => ({
      catalog: this.catalog,
      eshScripts: this.eshScripts,
      hasTrace: this.traceReport !== null,
    }));

    this.traceWebview = new EveryinitTraceWebviewManager({
      onRunTask: (taskName) => {
        void executeEvinCli(["run", taskName]);
      },
      onDryRunTask: (taskName) => {
        void executeEvinCli(["--dry-run", "run", taskName]);
      },
      onOpenTaskDefinition: (taskName) => {
        void this.openTaskByName(taskName);
      },
      onOpenTraceFile: () => {
        void this.openTraceJsonInEditor();
      },
      onRefresh: async () => {
        await this.refreshAll(true);
      },
    });
    this.disposables.push(this.traceWebview);

    this.tasksView = vscode.window.createTreeView("everyinit.tasksView", {
      treeDataProvider: this.tasksProvider,
      showCollapseAll: true,
    });
    this.traceView = vscode.window.createTreeView("everyinit.traceView", {
      treeDataProvider: this.traceProvider,
    });
    this.workspaceView = vscode.window.createTreeView("everyinit.workspaceView", {
      treeDataProvider: this.workspaceProvider,
      showCollapseAll: true,
    });

    this.disposables.push(
      this.tasksView,
      this.traceView,
      this.workspaceView,
      this.tasksProvider,
      this.traceProvider,
      this.workspaceProvider
    );

    this.registerSidebarCommands();
    this.registerWatchers();
    this.syncContextKeys();
    this.updateViewDescriptions();

    // Eagerly hydrate only when `.evin/workspace.flow` exists or a sidebar view is visible,
    // preserving zero background LSP processes in unrelated workspaces.
    if (this.hasWorkspaceManifestOnDisk() || this.isAnyViewVisible()) {
      void this.refreshAll(true);
    } else {
      void this.refreshTraceAndScriptsOnly();
    }
  }

  public dispose(): void {
    for (const disposable of this.disposables) {
      disposable.dispose();
    }
    this.disposables.length = 0;
  }

  private registerSidebarCommands(): void {
    this.disposables.push(
      vscode.commands.registerCommand("everyinit.refreshSidebar", async () => {
        await this.refreshAll(true);
      }),

      vscode.commands.registerCommand("everyinit.toggleTaskGrouping", async () => {
        this.taskGrouping = this.taskGrouping === "module" ? "stage" : "module";
        await this.context.workspaceState.update(WORKSPACE_GROUPING_KEY, this.taskGrouping);
        this.syncContextKeys();
        this.updateViewDescriptions();
        this.tasksProvider.refresh();
      }),

      vscode.commands.registerCommand("everyinit.filterTasks", async () => {
        const value = await vscode.window.showInputBox({
          title: "Everyinit: Filter Tasks",
          prompt: "Filter tasks by name, stage, description, or hook (leave empty to clear)",
          value: this.taskFilter,
          placeHolder: "e.g. check, vscode, pre-commit",
        });
        if (value === undefined) {
          return;
        }
        this.taskFilter = value.trim();
        this.syncContextKeys();
        this.updateViewDescriptions();
        this.tasksProvider.refresh();
      }),

      vscode.commands.registerCommand("everyinit.clearTaskFilter", () => {
        if (!this.taskFilter) {
          return;
        }
        this.taskFilter = "";
        this.syncContextKeys();
        this.updateViewDescriptions();
        this.tasksProvider.refresh();
      }),

      vscode.commands.registerCommand("everyinit.toggleTraceSort", async () => {
        this.traceSort = this.traceSort === "timeline" ? "slowest" : "timeline";
        await this.context.workspaceState.update(WORKSPACE_TRACE_SORT_KEY, this.traceSort);
        this.syncContextKeys();
        this.updateViewDescriptions();
        this.traceProvider.refresh();
      }),

      vscode.commands.registerCommand("everyinit.openTaskDefinition", async (arg?: unknown) => {
        const taskName = extractTaskNameArg(arg);
        if (!taskName) {
          return;
        }
        await this.openTaskByName(taskName);
      }),

      vscode.commands.registerCommand("everyinit.openTraceTimeline", async (arg?: unknown) => {
        const focusTask = extractTaskNameArg(arg) ?? undefined;
        if (!this.traceReport) {
          await this.reloadTrace();
        }
        this.traceWebview.show(
          this.traceReport,
          this.catalog?.workspaceName ?? null,
          focusTask
        );
      }),

      vscode.commands.registerCommand("everyinit.openTraceFile", async () => {
        await this.openTraceJsonInEditor();
      })
    );
  }

  private registerWatchers(): void {
    const onVisibilityChange = (event: vscode.TreeViewVisibilityChangeEvent) => {
      if (event.visible && !this.initializedCatalog) {
        void this.refreshAll(true);
      }
    };

    this.disposables.push(
      this.tasksView.onDidChangeVisibility(onVisibilityChange),
      this.traceView.onDidChangeVisibility(onVisibilityChange),
      this.workspaceView.onDidChangeVisibility(onVisibilityChange)
    );

    this.disposables.push(
      this.clientManager.onDidChangeState(() => {
        if (this.initializedCatalog || this.hasWorkspaceManifestOnDisk() || this.isAnyViewVisible()) {
          void this.reloadCatalog();
        }
      })
    );

    const traceWatcher = vscode.workspace.createFileSystemWatcher("**/.evin/trace.json");
    const handleTraceEvent = () => {
      void this.reloadTrace();
    };
    this.disposables.push(
      traceWatcher,
      traceWatcher.onDidCreate(handleTraceEvent),
      traceWatcher.onDidChange(handleTraceEvent),
      traceWatcher.onDidDelete(handleTraceEvent)
    );

    const eshWatcher = vscode.workspace.createFileSystemWatcher("**/*.esh");
    const handleEshEvent = () => {
      void this.reloadEshScripts();
    };
    this.disposables.push(
      eshWatcher,
      eshWatcher.onDidCreate(handleEshEvent),
      eshWatcher.onDidDelete(handleEshEvent)
    );

    const manifestWatcher = vscode.workspace.createFileSystemWatcher("**/.evin/workspace.flow");
    const handleManifestEvent = () => {
      this.syncContextKeys();
      void this.refreshAll(this.hasWorkspaceManifestOnDisk() || this.isAnyViewVisible());
    };
    this.disposables.push(
      manifestWatcher,
      manifestWatcher.onDidCreate(handleManifestEvent),
      manifestWatcher.onDidDelete(handleManifestEvent)
    );
  }

  private async refreshAll(allowStartLsp: boolean): Promise<void> {
    if (this.refreshPromise) {
      await this.refreshPromise;
      return;
    }

    this.refreshPromise = (async () => {
      this.syncContextKeys();
      await Promise.all([
        allowStartLsp && (this.hasWorkspaceManifestOnDisk() || this.clientManager.isEvinRunning())
          ? this.reloadCatalog()
          : Promise.resolve(),
        this.reloadTrace(),
        this.reloadEshScripts(),
      ]);
    })();

    try {
      await this.refreshPromise;
    } finally {
      this.refreshPromise = null;
    }
  }

  private async refreshTraceAndScriptsOnly(): Promise<void> {
    await Promise.all([this.reloadTrace(), this.reloadEshScripts()]);
  }

  private async reloadCatalog(): Promise<void> {
    this.initializedCatalog = true;
    this.catalog = await this.clientManager.fetchWorkspaceCatalog();
    this.updateViewDescriptions();
    this.traceWebview.updateData(this.traceReport, this.catalog?.workspaceName ?? null);
    this.tasksProvider.refresh();
    this.workspaceProvider.refresh();
  }

  private async reloadTrace(): Promise<void> {
    const workspaceRoot = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
    this.traceReport = await loadWorkspaceTraceReport(workspaceRoot);
    this.updateViewDescriptions();
    this.traceWebview.updateData(this.traceReport, this.catalog?.workspaceName ?? null);
    this.traceProvider.refresh();
    this.tasksProvider.refresh();
    this.workspaceProvider.refresh();
  }

  private async reloadEshScripts(): Promise<void> {
    const uris = await vscode.workspace.findFiles(
      "**/*.esh",
      "{**/node_modules/**,**/target/**,**/dist/**,**/.git/**}"
    );
    uris.sort((a, b) => a.fsPath.localeCompare(b.fsPath));
    this.eshScripts = uris;
    this.workspaceProvider.refresh();
  }

  private async openTaskByName(taskName: string): Promise<void> {
    if (!this.catalog) {
      await this.reloadCatalog();
    }
    const task = this.catalog?.tasks.find((item) => item.name === taskName);
    if (task?.location) {
      await openLocationInEditor(task.location);
      return;
    }
    const workspaceRoot = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
    if (task && workspaceRoot) {
      const fallbackUri = vscode.Uri.file(path.join(workspaceRoot, task.sourcePath));
      const doc = await vscode.workspace.openTextDocument(fallbackUri);
      await vscode.window.showTextDocument(doc, { preview: false });
      return;
    }
    vscode.window.showWarningMessage(`Could not locate definition for task '${taskName}'.`);
  }

  private async openTraceJsonInEditor(): Promise<void> {
    const workspaceRoot = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
    if (!workspaceRoot) {
      return;
    }
    const tracePath = path.join(workspaceRoot, TRACE_RELATIVE_PATH);
    if (!fs.existsSync(tracePath)) {
      vscode.window.showInformationMessage(
        "No .evin/trace.json file exists yet. Run an Evin task first."
      );
      return;
    }
    const doc = await vscode.workspace.openTextDocument(vscode.Uri.file(tracePath));
    await vscode.window.showTextDocument(doc, { preview: false });
  }

  private hasWorkspaceManifestOnDisk(): boolean {
    const workspaceRoot = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
    return (
      Boolean(workspaceRoot) &&
      fs.existsSync(path.join(workspaceRoot!, ".evin", "workspace.flow"))
    );
  }

  private isAnyViewVisible(): boolean {
    return this.tasksView.visible || this.traceView.visible || this.workspaceView.visible;
  }

  private syncContextKeys(): void {
    void vscode.commands.executeCommand(
      "setContext",
      "everyinit.hasWorkspaceManifest",
      this.hasWorkspaceManifestOnDisk()
    );
    void vscode.commands.executeCommand(
      "setContext",
      "everyinit.hasTaskFilter",
      this.taskFilter.length > 0
    );
    void vscode.commands.executeCommand(
      "setContext",
      "everyinit.taskGrouping",
      this.taskGrouping
    );
    void vscode.commands.executeCommand(
      "setContext",
      "everyinit.traceSort",
      this.traceSort
    );
  }

  private updateViewDescriptions(): void {
    const parts: string[] = [];
    if (this.catalog) {
      parts.push(`${this.catalog.tasks.length} tasks`);
      parts.push(this.taskGrouping === "module" ? "by module" : "by stage");
      if (this.catalog.stale) {
        parts.push("stale");
      }
    }
    if (this.taskFilter) {
      parts.push(`filter: "${this.taskFilter}"`);
    }
    this.tasksView.description = parts.join(" · ");

    if (this.traceReport) {
      this.traceView.description = `${formatDurationUs(this.traceReport.wallDurationUs)} · ${
        this.traceSort === "slowest" ? "slowest" : "timeline"
      }`;
    } else {
      this.traceView.description = undefined;
    }

    if (this.catalog?.workspaceName) {
      this.workspaceView.description = this.catalog.workspaceVersion
        ? `${this.catalog.workspaceName} v${this.catalog.workspaceVersion}`
        : this.catalog.workspaceName;
    } else {
      this.workspaceView.description = undefined;
    }
  }
}

// ============================================================================
// 1. Tasks Tree View
// ============================================================================

interface TasksTreeState {
  catalog: WorkspaceCatalogResult | null;
  traceReport: ParsedTraceReport | null;
  grouping: TaskGroupingMode;
  filter: string;
}

type TaskTreeNode = TaskGroupNode | TaskItemNode | TaskDetailNode;

class TaskGroupNode extends vscode.TreeItem {
  public readonly kind = "group" as const;

  constructor(
    label: string,
    public readonly tasks: readonly CatalogTaskItem[],
    subtitle: string,
    iconId: string
  ) {
    super(label, vscode.TreeItemCollapsibleState.Expanded);
    this.description = subtitle;
    this.iconPath = new vscode.ThemeIcon(iconId);
    this.contextValue = "taskGroup";
  }
}

export class TaskItemNode extends vscode.TreeItem {
  public readonly kind = "task" as const;
  public readonly taskName: string;

  constructor(
    public readonly task: CatalogTaskItem,
    displayLabel: string,
    latestSpan: ParsedTraceSpan | undefined
  ) {
    const hasDetails =
      task.needs.length > 0 || task.hooks.length > 0 || task.watchPaths.length > 0;
    super(
      displayLabel,
      hasDetails
        ? vscode.TreeItemCollapsibleState.Collapsed
        : vscode.TreeItemCollapsibleState.None
    );

    this.taskName = task.name;
    this.contextValue = "taskItem";
    this.iconPath = resolveTaskIcon(task, latestSpan);

    const badges: string[] = [];
    if (task.stage) {
      badges.push(task.stage);
    }
    if (task.hooks.length > 0) {
      badges.push(`hook: ${task.hooks.join(", ")}`);
    }
    if (task.hasWatch) {
      badges.push("watch");
    }
    if (latestSpan) {
      badges.push(formatDurationUs(latestSpan.durationUs));
    }
    this.description = badges.join(" · ");
    this.tooltip = buildTaskTooltip(task, latestSpan);

    if (!hasDetails) {
      this.command = {
        command: "everyinit.openTaskDefinition",
        title: "Open Task Definition",
        arguments: [task.name],
      };
    }
  }
}

class TaskDetailNode extends vscode.TreeItem {
  public readonly kind = "detail" as const;
  public readonly taskName?: string;

  constructor(
    label: string,
    description: string,
    iconId: string,
    linkedTaskName?: string
  ) {
    super(label, vscode.TreeItemCollapsibleState.None);
    this.description = description;
    this.iconPath = new vscode.ThemeIcon(iconId);
    if (linkedTaskName) {
      this.taskName = linkedTaskName;
      this.contextValue = "taskDependency";
      this.command = {
        command: "everyinit.openTaskDefinition",
        title: "Open Dependency Definition",
        arguments: [linkedTaskName],
      };
    } else {
      this.contextValue = "taskMetadata";
    }
  }
}

class TasksTreeProvider implements vscode.TreeDataProvider<TaskTreeNode>, vscode.Disposable {
  private readonly emitter = new vscode.EventEmitter<TaskTreeNode | undefined | void>();
  public readonly onDidChangeTreeData = this.emitter.event;

  constructor(private readonly getState: () => TasksTreeState) {}

  public refresh(): void {
    this.emitter.fire();
  }

  public dispose(): void {
    this.emitter.dispose();
  }

  public getTreeItem(element: TaskTreeNode): vscode.TreeItem {
    return element;
  }

  public getChildren(element?: TaskTreeNode): TaskTreeNode[] {
    const { catalog, traceReport, grouping, filter } = this.getState();
    if (!catalog) {
      return [];
    }

    if (!element) {
      const filtered = filterTasks(catalog.tasks, filter);
      if (filtered.length === 0) {
        return [];
      }

      if (grouping === "stage") {
        return buildStageGroups(filtered, catalog.stages);
      }
      return buildModuleGroups(filtered, catalog.workspaceName);
    }

    if (element.kind === "group") {
      return element.tasks.map((task) => {
        const displayLabel = grouping === "module" ? task.shortName : task.name;
        const latestSpan = traceReport?.latestByTask.get(task.name);
        return new TaskItemNode(task, displayLabel, latestSpan);
      });
    }

    if (element.kind === "task") {
      const children: TaskDetailNode[] = [];
      for (const dep of element.task.needs) {
        children.push(new TaskDetailNode(dep, "needs", "references", dep));
      }
      for (const hook of element.task.hooks) {
        children.push(new TaskDetailNode(hook, "git hook", "git-commit"));
      }
      for (const glob of element.task.watchPaths) {
        children.push(new TaskDetailNode(glob, "watch", "eye"));
      }
      return children;
    }

    return [];
  }
}

function filterTasks(
  tasks: readonly CatalogTaskItem[],
  rawQuery: string
): CatalogTaskItem[] {
  const query = rawQuery.trim().toLowerCase();
  if (!query) {
    return [...tasks];
  }
  return tasks.filter(
    (task) =>
      task.name.toLowerCase().includes(query) ||
      (task.stage?.toLowerCase().includes(query) ?? false) ||
      (task.description?.toLowerCase().includes(query) ?? false) ||
      task.hooks.some((hook) => hook.toLowerCase().includes(query))
  );
}

function buildModuleGroups(
  tasks: readonly CatalogTaskItem[],
  workspaceName: string | null
): TaskGroupNode[] {
  const rootTasks: CatalogTaskItem[] = [];
  const byModule = new Map<string, CatalogTaskItem[]>();

  for (const task of tasks) {
    if (!task.module) {
      rootTasks.push(task);
    } else {
      const list = byModule.get(task.module) ?? [];
      list.push(task);
      byModule.set(task.module, list);
    }
  }

  const groups: TaskGroupNode[] = [];
  if (rootTasks.length > 0) {
    const label = workspaceName ? `Workspace (${workspaceName})` : "Workspace";
    groups.push(new TaskGroupNode(label, rootTasks, `${rootTasks.length}`, "root-folder"));
  }

  for (const [moduleName, moduleTasks] of [...byModule.entries()].sort((a, b) =>
    a[0].localeCompare(b[0])
  )) {
    groups.push(
      new TaskGroupNode(moduleName, moduleTasks, `${moduleTasks.length}`, "package")
    );
  }
  return groups;
}

function buildStageGroups(
  tasks: readonly CatalogTaskItem[],
  declaredStages: readonly string[]
): TaskGroupNode[] {
  const byStage = new Map<string, CatalogTaskItem[]>();
  const unstaged: CatalogTaskItem[] = [];

  for (const task of tasks) {
    if (!task.stage) {
      unstaged.push(task);
    } else {
      const list = byStage.get(task.stage) ?? [];
      list.push(task);
      byStage.set(task.stage, list);
    }
  }

  const orderedStageNames = [
    ...declaredStages.filter((stage) => byStage.has(stage)),
    ...[...byStage.keys()].filter((stage) => !declaredStages.includes(stage)).sort(),
  ];

  const groups = orderedStageNames.map((stageName) => {
    const stageTasks = byStage.get(stageName) ?? [];
    return new TaskGroupNode(stageName, stageTasks, `${stageTasks.length}`, "layers");
  });

  if (unstaged.length > 0) {
    groups.push(new TaskGroupNode("Unstaged", unstaged, `${unstaged.length}`, "circle-outline"));
  }
  return groups;
}

function resolveTaskIcon(
  task: CatalogTaskItem,
  latestSpan: ParsedTraceSpan | undefined
): vscode.ThemeIcon {
  if (latestSpan) {
    return statusThemeIcon(latestSpan.status);
  }
  if (task.hasWatch) {
    return new vscode.ThemeIcon("eye");
  }
  if (task.hooks.length > 0) {
    return new vscode.ThemeIcon("git-commit");
  }
  return new vscode.ThemeIcon("everyinit-logo");
}

function buildTaskTooltip(
  task: CatalogTaskItem,
  latestSpan: ParsedTraceSpan | undefined
): vscode.MarkdownString {
  const md = new vscode.MarkdownString(undefined, true);
  md.isTrusted = false;
  md.appendMarkdown(`**\`${task.name}\`**`);
  if (task.stage) {
    md.appendMarkdown(` — *${task.stage}*`);
  }
  md.appendMarkdown("\n\n");
  if (task.description) {
    md.appendMarkdown(`${task.description}\n\n`);
  }
  md.appendMarkdown(`- **Source:** \`${task.sourcePath}\`\n`);
  if (task.needs.length > 0) {
    md.appendMarkdown(`- **Needs:** \`${task.needs.join("`, `")}\`\n`);
  }
  if (task.hooks.length > 0) {
    md.appendMarkdown(`- **Hooks:** \`${task.hooks.join("`, `")}\`\n`);
  }
  if (task.watchPaths.length > 0) {
    md.appendMarkdown(`- **Watch:** \`${task.watchPaths.join("`, `")}\`\n`);
  }
  if (task.presets.length > 0) {
    md.appendMarkdown(`- **Presets:** \`${task.presets.join("`, `")}\`\n`);
  }
  if (latestSpan) {
    md.appendMarkdown(
      `- **Latest Trace:** ${latestSpan.status} in \`${formatDurationUs(latestSpan.durationUs)}\`\n`
    );
  }
  return md;
}

// ============================================================================
// 2. Execution Trace Tree View (.evin/trace.json)
// ============================================================================

interface TraceTreeState {
  traceReport: ParsedTraceReport | null;
  sortMode: TraceSortMode;
}

type TraceTreeNode = TraceSummaryNode | TraceSpanNode;

class TraceSummaryNode extends vscode.TreeItem {
  public readonly kind = "summary" as const;

  constructor(report: ParsedTraceReport) {
    const statusWord =
      report.counts.failed > 0
        ? `${report.counts.failed} failed`
        : `${report.counts.passed} passed`;
    super(
      `${statusWord} · ${formatDurationUs(report.wallDurationUs)}`,
      vscode.TreeItemCollapsibleState.None
    );

    const parallelNote =
      report.parallelismFactor >= 1.15
        ? ` · ${report.parallelismFactor.toFixed(1)}× parallel`
        : "";
    this.description = `${formatRelativeTimestamp(report.modifiedTimeMs)}${parallelNote}`;
    this.iconPath = statusThemeIcon(report.overallStatus);
    this.contextValue = "traceSummary";
    this.tooltip = new vscode.MarkdownString(
      [
        "**Execution Trace Summary** (click to open Waterfall Timeline)",
        "",
        `- **Wall Duration:** \`${formatDurationUs(report.wallDurationUs)}\``,
        `- **Cumulative Task Time:** \`${formatDurationUs(report.totalTaskDurationUs)}\` (${report.parallelismFactor.toFixed(2)}×)`,
        `- **Tasks:** ${report.counts.total} total (${report.counts.passed} passed, ${report.counts.failed} failed)`,
        `- **Worker Threads:** ${report.threadIds.length}`,
        `- **Updated:** ${new Date(report.modifiedTimeMs).toLocaleString()}`,
      ].join("\n")
    );
    this.command = {
      command: "everyinit.openTraceTimeline",
      title: "Open Execution Trace Timeline",
    };
  }
}

export class TraceSpanNode extends vscode.TreeItem {
  public readonly kind = "span" as const;
  public readonly taskName: string;

  constructor(
    public readonly span: ParsedTraceSpan,
    wallDurationUs: number
  ) {
    super(span.name, vscode.TreeItemCollapsibleState.None);
    this.taskName = span.name;
    this.contextValue = "traceSpan";
    this.iconPath = statusThemeIcon(span.status);

    const sharePct =
      wallDurationUs > 0 ? Math.round((span.durationUs / wallDurationUs) * 100) : 100;
    this.description = `${formatDurationUs(span.durationUs)} (${sharePct}%) · T${span.tid}`;
    this.tooltip = new vscode.MarkdownString(
      [
        `**\`${span.name}\`** — ${span.status}`,
        "",
        `- **Duration:** \`${formatDurationUs(span.durationUs)}\` (${span.durationUs.toLocaleString()} µs)`,
        `- **Start Offset:** \`+${formatDurationUs(span.offsetUs)}\``,
        `- **Thread:** \`tid ${span.tid}\` (\`pid ${span.pid}\`)`,
        "",
        "*Click to inspect in Waterfall Timeline*",
      ].join("\n")
    );
    this.command = {
      command: "everyinit.openTraceTimeline",
      title: "Inspect in Execution Trace Timeline",
      arguments: [span.name],
    };
  }
}

class TraceTreeProvider implements vscode.TreeDataProvider<TraceTreeNode>, vscode.Disposable {
  private readonly emitter = new vscode.EventEmitter<TraceTreeNode | undefined | void>();
  public readonly onDidChangeTreeData = this.emitter.event;

  constructor(private readonly getState: () => TraceTreeState) {}

  public refresh(): void {
    this.emitter.fire();
  }

  public dispose(): void {
    this.emitter.dispose();
  }

  public getTreeItem(element: TraceTreeNode): vscode.TreeItem {
    return element;
  }

  public getChildren(element?: TraceTreeNode): TraceTreeNode[] {
    if (element) {
      return [];
    }
    const { traceReport, sortMode } = this.getState();
    if (!traceReport || traceReport.spans.length === 0) {
      return [];
    }

    const spans = [...traceReport.spans];
    if (sortMode === "slowest") {
      spans.sort((a, b) => b.durationUs - a.durationUs);
    } else {
      spans.sort((a, b) => a.offsetUs - b.offsetUs || b.durationUs - a.durationUs);
    }

    return [
      new TraceSummaryNode(traceReport),
      ...spans.map((span) => new TraceSpanNode(span, traceReport.wallDurationUs)),
    ];
  }
}

// ============================================================================
// 3. Workspace & Scripts Tree View
// ============================================================================

interface WorkspaceTreeState {
  catalog: WorkspaceCatalogResult | null;
  eshScripts: readonly vscode.Uri[];
  hasTrace: boolean;
}

type WorkspaceTreeNode =
  | WorkspaceSectionNode
  | QuickActionNode
  | ManifestItemNode
  | EshScriptItemNode;

class WorkspaceSectionNode extends vscode.TreeItem {
  public readonly kind = "section" as const;

  constructor(
    public readonly sectionId: "actions" | "manifests" | "scripts",
    label: string,
    countText: string | undefined,
    iconId: string
  ) {
    super(label, vscode.TreeItemCollapsibleState.Expanded);
    this.description = countText;
    this.iconPath = new vscode.ThemeIcon(iconId);
    this.contextValue = "workspaceSection";
  }
}

class QuickActionNode extends vscode.TreeItem {
  public readonly kind = "action" as const;

  constructor(label: string, description: string, iconId: string, commandId: string) {
    super(label, vscode.TreeItemCollapsibleState.None);
    this.description = description;
    this.iconPath = new vscode.ThemeIcon(iconId);
    this.contextValue = "quickAction";
    this.command = {
      command: commandId,
      title: label,
    };
  }
}

class ManifestItemNode extends vscode.TreeItem {
  public readonly kind = "manifest" as const;

  constructor(public readonly manifest: CatalogManifestItem) {
    super(manifest.path, vscode.TreeItemCollapsibleState.None);
    this.resourceUri = vscode.Uri.parse(manifest.uri);
    this.contextValue = "manifestItem";

    const statusParts = [manifest.kind];
    if (manifest.errors > 0) {
      statusParts.push(`${manifest.errors} error${manifest.errors === 1 ? "" : "s"}`);
      this.iconPath = new vscode.ThemeIcon(
        "error",
        new vscode.ThemeColor("testing.iconFailed")
      );
    } else if (manifest.warnings > 0) {
      statusParts.push(`${manifest.warnings} warning${manifest.warnings === 1 ? "" : "s"}`);
      this.iconPath = new vscode.ThemeIcon(
        "warning",
        new vscode.ThemeColor("editorWarning.foreground")
      );
    } else {
      this.iconPath = new vscode.ThemeIcon("file-code");
    }

    this.description = statusParts.join(" · ");
    this.command = {
      command: "vscode.open",
      title: "Open Flow Manifest",
      arguments: [this.resourceUri],
    };
  }
}

export class EshScriptItemNode extends vscode.TreeItem {
  public readonly kind = "script" as const;

  constructor(uri: vscode.Uri, workspaceRoot: string | undefined) {
    const relPath = workspaceRoot
      ? path.relative(workspaceRoot, uri.fsPath).replace(/\\/g, "/")
      : path.basename(uri.fsPath);
    const baseName = path.posix.basename(relPath);
    const dirName = path.posix.dirname(relPath);

    super(baseName, vscode.TreeItemCollapsibleState.None);
    this.resourceUri = uri;
    this.description = dirName === "." ? undefined : dirName;
    this.iconPath = new vscode.ThemeIcon("terminal");
    this.contextValue = "eshScriptItem";
    this.tooltip = relPath;
    this.command = {
      command: "vscode.open",
      title: "Open ESH Script",
      arguments: [uri],
    };
  }
}

class WorkspaceTreeProvider
  implements vscode.TreeDataProvider<WorkspaceTreeNode>, vscode.Disposable
{
  private readonly emitter = new vscode.EventEmitter<WorkspaceTreeNode | undefined | void>();
  public readonly onDidChangeTreeData = this.emitter.event;

  constructor(private readonly getState: () => WorkspaceTreeState) {}

  public refresh(): void {
    this.emitter.fire();
  }

  public dispose(): void {
    this.emitter.dispose();
  }

  public getTreeItem(element: WorkspaceTreeNode): vscode.TreeItem {
    return element;
  }

  public getChildren(element?: WorkspaceTreeNode): WorkspaceTreeNode[] {
    const { catalog, eshScripts, hasTrace } = this.getState();
    const workspaceRoot = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;

    if (!element) {
      const sections: WorkspaceSectionNode[] = [
        new WorkspaceSectionNode("actions", "Workspace Actions", undefined, "zap"),
      ];
      if (catalog && catalog.manifests.length > 0) {
        sections.push(
          new WorkspaceSectionNode(
            "manifests",
            "Flow Manifests",
            `${catalog.manifests.length}`,
            "files"
          )
        );
      }
      if (eshScripts.length > 0) {
        sections.push(
          new WorkspaceSectionNode(
            "scripts",
            "ESH Scripts",
            `${eshScripts.length}`,
            "terminal-bash"
          )
        );
      }
      return sections;
    }

    if (element.kind === "section") {
      switch (element.sectionId) {
        case "actions": {
          const actions = [
            new QuickActionNode(
              "Check Workspace",
              "evin check -w",
              "check-all",
              "everyinit.checkWorkspace"
            ),
            new QuickActionNode(
              "Sync Hooks & Environment",
              "evin sync",
              "sync",
              "everyinit.syncWorkspace"
            ),
            new QuickActionNode(
              "Run Task...",
              "evin run",
              "play",
              "everyinit.runTask"
            ),
          ];
          if (hasTrace) {
            actions.push(
              new QuickActionNode(
                "Open Trace Waterfall",
                ".evin/trace.json",
                "graph-line",
                "everyinit.openTraceTimeline"
              )
            );
          }
          return actions;
        }
        case "manifests":
          return (catalog?.manifests ?? []).map((manifest) => new ManifestItemNode(manifest));
        case "scripts":
          return eshScripts.map((uri) => new EshScriptItemNode(uri, workspaceRoot));
      }
    }

    return [];
  }
}

function statusThemeIcon(status: TraceTaskStatus): vscode.ThemeIcon {
  switch (status) {
    case "success":
      return new vscode.ThemeIcon("pass", new vscode.ThemeColor("testing.iconPassed"));
    case "failed":
      return new vscode.ThemeIcon("error", new vscode.ThemeColor("testing.iconFailed"));
    case "unverified":
      return new vscode.ThemeIcon("warning", new vscode.ThemeColor("editorWarning.foreground"));
    case "cancelled":
      return new vscode.ThemeIcon("circle-slash", new vscode.ThemeColor("descriptionForeground"));
    default:
      return new vscode.ThemeIcon("circle-outline");
  }
}

