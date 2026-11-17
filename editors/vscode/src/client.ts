import * as path from "node:path";
import * as vscode from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  NotificationType,
  ServerOptions,
  Trace,
} from "vscode-languageclient/node";
import { getConfiguration, resolveBinaryPath } from "./config.js";
import { FlowSemanticTokensFeature } from "./semanticTokens.js";
import {
  CatalogManifestItem,
  CatalogTaskItem,
  LspLocation,
  TraceLevel,
  WorkspaceCatalogResult,
} from "./types.js";

interface LspWorkspaceSymbol {
  name: string;
  kind: number;
  location: LspLocation;
  containerName?: string | null;
}

export interface PipelineEventEnvelope {
  readonly schema_version: number;
  readonly run_id: string;
  readonly seq: number;
  readonly timestamp: string;
  readonly kind: string;
  readonly payload: Readonly<Record<string, unknown>>;
}

export interface RunTaskResult {
  readonly runId: string;
  readonly success: boolean;
  readonly cancelled: boolean;
  readonly exitCode: number | null;
}

export class EveryinitClientManager implements vscode.Disposable {
  private evinClient: LanguageClient | null = null;
  private eshClient: LanguageClient | null = null;
  private evinStartPromise: Promise<void> | null = null;
  private eshStartPromise: Promise<void> | null = null;
  private evinMissingNotified = false;
  private eshMissingNotified = false;
  private stateChangeTimer: ReturnType<typeof setTimeout> | null = null;
  private pipelineEventListener: vscode.Disposable | null = null;

  private readonly stateEmitter = new vscode.EventEmitter<void>();
  public readonly onDidChangeState: vscode.Event<void> = this.stateEmitter.event;
  private readonly pipelineEventEmitter = new vscode.EventEmitter<PipelineEventEnvelope>();
  public readonly onDidReceivePipelineEvent: vscode.Event<PipelineEventEnvelope> =
    this.pipelineEventEmitter.event;
  private readonly taskRunFinishedEmitter = new vscode.EventEmitter<{
    readonly taskName: string;
    readonly runId?: string;
    readonly success?: boolean;
    readonly cancelled?: boolean;
  }>();
  public readonly onDidFinishTaskRun = this.taskRunFinishedEmitter.event;

  private outputChannel: vscode.OutputChannel;

  constructor() {
    this.outputChannel = vscode.window.createOutputChannel("Everyinit Language Server");
  }

  public showOutputChannel(): void {
    this.outputChannel.show(true);
  }

  public async ensureClientForDocument(document: vscode.TextDocument): Promise<void> {
    const languageId = document.languageId;
    const evinJsonManifest =
      languageId === "json" &&
      /[\\/]\.evin[\\/](?:policy|environment)\.json$/.test(document.uri.fsPath);
    if (languageId === "flow" || evinJsonManifest) {
      await this.startEvinClient();
    } else if (languageId === "esh") {
      await this.startEshClient();
    }
  }

  public async startEvinClient(): Promise<void> {
    const config = getConfiguration();
    if (!config.evinEnable || this.evinClient) {
      return;
    }
    if (this.evinStartPromise) {
      await this.evinStartPromise;
      return;
    }

    this.evinStartPromise = this.doStartEvinClient(config.evinPath, config.trace);
    try {
      await this.evinStartPromise;
    } finally {
      this.evinStartPromise = null;
    }
  }

  private async doStartEvinClient(evinPath: string, trace: TraceLevel): Promise<void> {
    const binaryPath = resolveBinaryPath(evinPath);
    if (!binaryPath) {
      this.outputChannel.appendLine(
        `[evin] Binary could not be resolved from configured path: '${evinPath}'`
      );
      if (!this.evinMissingNotified) {
        this.evinMissingNotified = true;
        this.notifyMissingBinary(
          "Evin",
          evinPath,
          "cargo install --path crates/evin",
          "everyinit.evin.path"
        );
      }
      return;
    }

    try {
      this.outputChannel.appendLine(`[evin] Starting language server: ${binaryPath} lsp`);

      const workspaceFolder = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
      const serverOptions: ServerOptions = {
        command: binaryPath,
        args: ["lsp"],
        options: {
          cwd: workspaceFolder,
          env: {
            ...process.env,
            RUST_BACKTRACE: "1",
          },
        },
      };

      const clientOptions: LanguageClientOptions = {
        documentSelector: [
          { scheme: "file", language: "flow" },
          { scheme: "file", language: "json", pattern: "**/.evin/policy.json" },
          { scheme: "file", language: "json", pattern: "**/.evin/environment.json" }
        ],
        synchronize: {
          fileEvents: vscode.workspace.createFileSystemWatcher("**/*.flow"),
        },
        middleware: {
          handleDiagnostics: (uri, diagnostics, next) => {
            next(uri, diagnostics);
            this.scheduleStateChange();
          },
        },
        outputChannel: this.outputChannel,
        traceOutputChannel: this.outputChannel,
      };

      const client = new LanguageClient(
        "everyinit.evin",
        "Flow Language Server",
        serverOptions,
        clientOptions
      );

      client.registerFeature(new FlowSemanticTokensFeature(client));
      client.setTrace(this.resolveTrace(trace));
      await client.start();
      this.evinClient = client;
      this.pipelineEventListener = client.onNotification(
        new NotificationType<PipelineEventEnvelope>("evin/pipelineEvent"),
        (event) => this.pipelineEventEmitter.fire(event)
      );
      this.outputChannel.appendLine("[evin] Language server started successfully.");
      this.scheduleStateChange();
    } catch (error) {
      this.outputChannel.appendLine(`[evin] Failed to start language server: ${String(error)}`);
    }
  }

  public async startEshClient(): Promise<void> {
    const config = getConfiguration();
    if (!config.eshEnable || this.eshClient) {
      return;
    }
    if (this.eshStartPromise) {
      await this.eshStartPromise;
      return;
    }

    this.eshStartPromise = this.doStartEshClient(config.eshPath, config.trace);
    try {
      await this.eshStartPromise;
    } finally {
      this.eshStartPromise = null;
    }
  }

  private async doStartEshClient(eshPath: string, trace: TraceLevel): Promise<void> {
    const binaryPath = resolveBinaryPath(eshPath);
    if (!binaryPath) {
      this.outputChannel.appendLine(
        `[esh] Binary could not be resolved from configured path: '${eshPath}'`
      );
      if (!this.eshMissingNotified) {
        this.eshMissingNotified = true;
        this.notifyMissingBinary(
          "ESH",
          eshPath,
          "cargo install --path crates/esh",
          "everyinit.esh.path"
        );
      }
      return;
    }

    try {
      this.outputChannel.appendLine(`[esh] Starting language server: ${binaryPath} lsp`);

      const workspaceFolder = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
      const serverOptions: ServerOptions = {
        command: binaryPath,
        args: ["lsp"],
        options: {
          cwd: workspaceFolder,
          env: {
            ...process.env,
            RUST_BACKTRACE: "1",
          },
        },
      };

      const clientOptions: LanguageClientOptions = {
        documentSelector: [{ scheme: "file", language: "esh" }],
        synchronize: {
          fileEvents: vscode.workspace.createFileSystemWatcher("**/*.esh"),
        },
        outputChannel: this.outputChannel,
        traceOutputChannel: this.outputChannel,
      };

      const client = new LanguageClient(
        "everyinit.esh",
        "ESH Language Server",
        serverOptions,
        clientOptions
      );

      client.setTrace(this.resolveTrace(trace));
      await client.start();
      this.eshClient = client;
      this.outputChannel.appendLine("[esh] Language server started successfully.");
      this.scheduleStateChange();
    } catch (error) {
      this.outputChannel.appendLine(`[esh] Failed to start language server: ${String(error)}`);
    }
  }

  /**
   * Query the active Evin language server for the workspace catalog (tasks,
   * stages, manifests, and diagnostic counts). Falls back to `workspace/symbol`
   * if connected to an older `evin` binary.
   */
  public async fetchWorkspaceCatalog(): Promise<WorkspaceCatalogResult | null> {
    await this.startEvinClient();
    const client = this.evinClient;
    if (!client) {
      return null;
    }

    try {
      return await client.sendRequest<WorkspaceCatalogResult>("everyinit/workspaceCatalog", {});
    } catch (error) {
      this.outputChannel.appendLine(
        `[evin] workspaceCatalog fallback (${error instanceof Error ? error.message : String(error)})`
      );
      return this.fetchFallbackCatalog(client);
    }
  }

  public async runTask(
    taskName: string,
    options: {
      readonly dryRun?: boolean;
      readonly includeOutput?: boolean;
      readonly parallel?: boolean;
      readonly sequential?: boolean;
    } = {}
  ): Promise<RunTaskResult | null> {
    await this.startEvinClient();
    const client = this.evinClient;
    if (!client) {
      return null;
    }
    let result: RunTaskResult | null = null;
    try {
      result = await client.sendRequest<RunTaskResult>("workspace/executeCommand", {
        command: options.dryRun ? "everyinit.lspRunTaskDryRun" : "everyinit.lspRunTask",
        arguments: [{
          taskName,
          includeOutput: options.includeOutput ?? false,
          parallel: options.parallel ?? false,
          sequential: options.sequential ?? false,
        }],
      });
      return result;
    } catch (error) {
      this.outputChannel.appendLine(
        `[evin] LSP task execution failed: ${error instanceof Error ? error.message : String(error)}`
      );
      return null;
    } finally {
      this.taskRunFinishedEmitter.fire({
        taskName,
        runId: result?.runId,
        success: result?.success,
        cancelled: result?.cancelled,
      });
    }
  }

  private async fetchFallbackCatalog(client: LanguageClient): Promise<WorkspaceCatalogResult | null> {
    try {
      const symbols =
        (await client.sendRequest<LspWorkspaceSymbol[]>("workspace/symbol", { query: "" })) ?? [];
      const workspaceRoot = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;

      const tasks: CatalogTaskItem[] = symbols
        .filter((symbol) => symbol.kind === 12)
        .map((symbol) => {
          const colonIdx = symbol.name.indexOf(":");
          const moduleName = colonIdx > 0 ? symbol.name.slice(0, colonIdx) : null;
          const shortName = colonIdx > 0 ? symbol.name.slice(colonIdx + 1) : symbol.name;
          const fsPath = vscode.Uri.parse(symbol.location.uri).fsPath;
          const sourcePath = workspaceRoot
            ? path.relative(workspaceRoot, fsPath).replace(/\\/g, "/")
            : fsPath;

          return {
            name: symbol.name,
            shortName,
            module: moduleName,
            description: null,
            stage: null,
            needs: [],
            hooks: [],
            hasWatch: false,
            watchPaths: [],
            presets: [],
            exposed: false,
            sourcePath,
            location: symbol.location,
          };
        })
        .sort((a, b) => a.name.localeCompare(b.name));

      const flowUris = await vscode.workspace.findFiles(
        "**/*.flow",
        "{**/node_modules/**,**/target/**,**/dist/**,**/.git/**}"
      );
      const manifests: CatalogManifestItem[] = flowUris
        .map((uri) => {
          const relPath = workspaceRoot
            ? path.relative(workspaceRoot, uri.fsPath).replace(/\\/g, "/")
            : uri.fsPath;
          return {
            path: relPath,
            uri: uri.toString(),
            kind: inferManifestKind(relPath),
            errors: 0,
            warnings: 0,
          };
        })
        .sort((a, b) => a.path.localeCompare(b.path));

      return {
        workspaceName: null,
        workspaceVersion: null,
        stages: [],
        stale: false,
        diagnostics: { errors: 0, warnings: 0 },
        manifests,
        tasks,
      };
    } catch {
      return null;
    }
  }

  public async restart(): Promise<void> {
    const evinWasRunning = this.evinClient !== null;
    const eshWasRunning = this.eshClient !== null;

    this.outputChannel.appendLine("[everyinit] Restarting language servers...");
    await this.stopAll();

    if (evinWasRunning) {
      await this.startEvinClient();
    }
    if (eshWasRunning) {
      await this.startEshClient();
    }

    for (const doc of vscode.workspace.textDocuments) {
      await this.ensureClientForDocument(doc);
    }

    this.scheduleStateChange();
    vscode.window.showInformationMessage("Everyinit language servers restarted.");
  }

  public async stopAll(): Promise<void> {
    const stops: Promise<void>[] = [];

    if (this.evinClient) {
      const client = this.evinClient;
      this.evinClient = null;
      this.pipelineEventListener?.dispose();
      this.pipelineEventListener = null;
      stops.push(
        client.stop(2000).catch((err) => {
          this.outputChannel.appendLine(`[evin] Error stopping client: ${String(err)}`);
        })
      );
    }

    if (this.eshClient) {
      const client = this.eshClient;
      this.eshClient = null;
      stops.push(
        client.stop(2000).catch((err) => {
          this.outputChannel.appendLine(`[esh] Error stopping client: ${String(err)}`);
        })
      );
    }

    await Promise.all(stops);
    this.scheduleStateChange();
  }

  public isEvinRunning(): boolean {
    return this.evinClient !== null;
  }

  public isEshRunning(): boolean {
    return this.eshClient !== null;
  }

  public dispose(): void {
    if (this.stateChangeTimer) {
      clearTimeout(this.stateChangeTimer);
      this.stateChangeTimer = null;
    }
    void this.stopAll();
    this.stateEmitter.dispose();
    this.pipelineEventEmitter.dispose();
    this.taskRunFinishedEmitter.dispose();
    this.outputChannel.dispose();
  }

  private scheduleStateChange(): void {
    if (this.stateChangeTimer) {
      clearTimeout(this.stateChangeTimer);
    }
    this.stateChangeTimer = setTimeout(() => {
      this.stateChangeTimer = null;
      this.stateEmitter.fire();
    }, 120);
  }

  private resolveTrace(trace: TraceLevel): Trace {
    switch (trace) {
      case "messages":
        return Trace.Messages;
      case "verbose":
        return Trace.Verbose;
      default:
        return Trace.Off;
    }
  }

  private notifyMissingBinary(
    name: string,
    configuredPath: string,
    installHint: string,
    settingKey: string
  ): void {
    const openSettings = "Open Settings";
    const copyInstall = "Copy Install Command";

    vscode.window
      .showWarningMessage(
        `Everyinit: '${name}' binary not found at '${configuredPath}' or on PATH. Some language features will be disabled.`,
        copyInstall,
        openSettings
      )
      .then((choice) => {
        if (choice === copyInstall) {
          vscode.env.clipboard.writeText(installHint);
          vscode.window.showInformationMessage(`Copied: '${installHint}'`);
        } else if (choice === openSettings) {
          vscode.commands.executeCommand("workbench.action.openSettings", settingKey);
        }
      });
  }
}

function inferManifestKind(relPath: string): string {
  const base = path.posix.basename(relPath);
  if (base === "workspace.flow") return "workspace";
  if (base === "module.flow") return "module";
  if (base === "policy.json") return "policy";
  if (base === "environment.json") return "environment";
  return "namespace";
}
