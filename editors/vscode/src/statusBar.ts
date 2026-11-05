import * as vscode from "vscode";
import { EveryinitClientManager } from "./client.js";

export class EveryinitStatusBar implements vscode.Disposable {
  private item: vscode.StatusBarItem;
  private clientManager: EveryinitClientManager;
  private disposables: vscode.Disposable[] = [];

  constructor(clientManager: EveryinitClientManager) {
    this.clientManager = clientManager;
    this.item = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 100);
    this.item.command = "everyinit.statusMenu";
    this.disposables.push(this.item);

    // Register status bar menu command
    this.disposables.push(
      vscode.commands.registerCommand("everyinit.statusMenu", async () => {
        const choice = await vscode.window.showQuickPick([
          {
            label: "$(refresh) Restart Language Servers",
            description: "Restarts active Evin and ESH LSP instances",
            command: "everyinit.restartLsp",
          },
          {
            label: "$(output) Show Output Logs",
            description: "View LSP server console output",
            command: "everyinit.showOutput",
          },
          {
            label: "$(play) Run Workspace Task",
            description: "Execute an Evin workflow task",
            command: "everyinit.runTask",
          },
          {
            label: "$(terminal) Run ESH Script",
            description: "Execute the active script with ESH",
            command: "everyinit.runScript",
          },
        ]);

        if (choice?.command) {
          vscode.commands.executeCommand(choice.command);
        }
      })
    );

    // Update on active editor change
    this.disposables.push(
      vscode.window.onDidChangeActiveTextEditor((editor) => {
        this.update(editor?.document.languageId);
      })
    );

    this.update(vscode.window.activeTextEditor?.document.languageId);
  }

  public update(languageId?: string): void {
    if (languageId === "flow") {
      const running = this.clientManager.isEvinRunning();
      this.item.text = running ? "$(symbol-event) Evin" : "$(circle-slash) Evin (offline)";
      this.item.tooltip = "Everyinit: Evin Flow LSP";
      this.item.show();
    } else if (languageId === "esh") {
      const running = this.clientManager.isEshRunning();
      this.item.text = running ? "$(terminal) ESH" : "$(circle-slash) ESH (offline)";
      this.item.tooltip = "Everyinit: ESH Shell LSP";
      this.item.show();
    } else {
      this.item.hide();
    }
  }

  public dispose(): void {
    for (const d of this.disposables) {
      d.dispose();
    }
    this.disposables = [];
  }
}

