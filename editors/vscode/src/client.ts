import * as vscode from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  Trace,
} from "vscode-languageclient/node.js";
import { getConfiguration, resolveBinaryPath } from "./config.js";
import { TraceLevel } from "./types.js";

export class EveryinitClientManager implements vscode.Disposable {
  private evinClient: LanguageClient | null = null;
  private eshClient: LanguageClient | null = null;
  private evinStarting = false;
  private eshStarting = false;
  private evinMissingNotified = false;
  private eshMissingNotified = false;

  private outputChannel: vscode.OutputChannel;

  constructor() {
    this.outputChannel = vscode.window.createOutputChannel("Everyinit Language Server");
  }

  public showOutputChannel(): void {
    this.outputChannel.show(true);
  }

  public async ensureClientForLanguage(languageId: string): Promise<void> {
    if (languageId === "flow") {
      await this.startEvinClient();
    } else if (languageId === "esh") {
      await this.startEshClient();
    }
  }

  public async startEvinClient(): Promise<void> {
    const config = getConfiguration();
    if (!config.evinEnable || this.evinClient || this.evinStarting) {
      return;
    }

    const binaryPath = resolveBinaryPath(config.evinPath);
    if (!binaryPath) {
      if (!this.evinMissingNotified) {
        this.evinMissingNotified = true;
        this.notifyMissingBinary(
          "Evin",
          config.evinPath,
          "cargo install --path crates/evin",
          "everyinit.evin.path"
        );
      }
      return;
    }

    this.evinStarting = true;
    try {
      this.outputChannel.appendLine(`[evin] Starting language server: ${binaryPath} lsp`);

      const serverOptions: ServerOptions = {
        command: binaryPath,
        args: ["lsp"],
        options: {
          env: {
            ...process.env,
            RUST_BACKTRACE: "1",
          },
        },
      };

      const clientOptions: LanguageClientOptions = {
        documentSelector: [{ scheme: "file", language: "flow" }],
        synchronize: {
          fileEvents: vscode.workspace.createFileSystemWatcher("**/*.flow"),
        },
        outputChannel: this.outputChannel,
        traceOutputChannel: this.outputChannel,
      };

      const client = new LanguageClient(
        "everyinit.evin",
        "Evin Flow Language Server",
        serverOptions,
        clientOptions
      );

      client.setTrace(this.resolveTrace(config.trace));
      await client.start();
      this.evinClient = client;
      this.outputChannel.appendLine("[evin] Language server started successfully.");
    } catch (error) {
      this.outputChannel.appendLine(`[evin] Failed to start language server: ${String(error)}`);
    } finally {
      this.evinStarting = false;
    }
  }

  public async startEshClient(): Promise<void> {
    const config = getConfiguration();
    if (!config.eshEnable || this.eshClient || this.eshStarting) {
      return;
    }

    const binaryPath = resolveBinaryPath(config.eshPath);
    if (!binaryPath) {
      if (!this.eshMissingNotified) {
        this.eshMissingNotified = true;
        this.notifyMissingBinary(
          "ESH",
          config.eshPath,
          "cargo install --path crates/esh",
          "everyinit.esh.path"
        );
      }
      return;
    }

    this.eshStarting = true;
    try {
      this.outputChannel.appendLine(`[esh] Starting language server: ${binaryPath} lsp`);

      const serverOptions: ServerOptions = {
        command: binaryPath,
        args: ["lsp"],
        options: {
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

      client.setTrace(this.resolveTrace(config.trace));
      await client.start();
      this.eshClient = client;
      this.outputChannel.appendLine("[esh] Language server started successfully.");
    } catch (error) {
      this.outputChannel.appendLine(`[esh] Failed to start language server: ${String(error)}`);
    } finally {
      this.eshStarting = false;
    }
  }

  public async restart(): Promise<void> {
    this.outputChannel.appendLine("[everyinit] Restarting language servers...");
    await this.stopAll();

    // Check currently open documents to restart active servers
    for (const doc of vscode.workspace.textDocuments) {
      await this.ensureClientForLanguage(doc.languageId);
    }

    vscode.window.showInformationMessage("Everyinit language servers restarted.");
  }

  public async stopAll(): Promise<void> {
    const stops: Promise<void>[] = [];

    if (this.evinClient) {
      const client = this.evinClient;
      this.evinClient = null;
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
  }

  public isEvinRunning(): boolean {
    return this.evinClient !== null;
  }

  public isEshRunning(): boolean {
    return this.eshClient !== null;
  }

  public dispose(): void {
    void this.stopAll();
    this.outputChannel.dispose();
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

