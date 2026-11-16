import * as vscode from "vscode";
import { EveryinitClientManager } from "./client.js";

export class EveryinitStatusBar implements vscode.Disposable {
  private item: vscode.StatusBarItem;
  private clientManager: EveryinitClientManager;
  private disposables: vscode.Disposable[] = [];
  private readonly runningTasks = new Map<string, string>();

  constructor(clientManager: EveryinitClientManager) {
    this.clientManager = clientManager;
    this.item = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 100);
    this.item.command = "everyinit.statusMenu";
    this.disposables.push(this.item);
    this.disposables.push(
      this.clientManager.onDidReceivePipelineEvent((event) => {
        if (event.kind === "run_started") {
          const tasks = event.payload.tasks;
          this.runningTasks.set(
            event.run_id,
            Array.isArray(tasks) && tasks.every((task) => typeof task === "string")
              ? tasks.join(", ")
              : "task"
          );
        } else if (event.kind === "task_started") {
          const name = event.payload.name;
          if (typeof name === "string") {
            this.runningTasks.set(event.run_id, name);
          }
        } else if (event.kind === "run_completed") {
          this.runningTasks.delete(event.run_id);
        }
        this.update(vscode.window.activeTextEditor?.document.languageId);
      })
    );
    this.disposables.push(
      this.clientManager.onDidFinishTaskRun(({ taskName, runId }) => {
        if (runId) {
          this.runningTasks.delete(runId);
        } else {
          for (const [activeRunId, runningTask] of this.runningTasks) {
            if (runningTask === taskName) this.runningTasks.delete(activeRunId);
          }
        }
        this.update(vscode.window.activeTextEditor?.document.languageId);
      })
    );

    // Register status bar menu command
    this.disposables.push(
      vscode.commands.registerCommand("everyinit.statusMenu", async () => {
        const choice = await vscode.window.showQuickPick([
          {
            label: "$(refresh) Restart Language Servers",
            description: "Restarts active FLOW and ESH language servers",
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
            label: "$(graph-line) Show Execution Trace Timeline",
            description: "Inspect the latest .evin/trace.json waterfall",
            command: "everyinit.openTraceTimeline",
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
      const taskRun = [...this.runningTasks.values()].at(-1);
      this.item.text = taskRun
        ? `$(sync~spin) ${taskRun}`
        : running
          ? "$(everyinit-logo) FLOW"
          : "$(circle-slash) FLOW (offline)";
      this.item.tooltip = taskRun
        ? `Everyinit: Running ${taskRun}`
        : "Everyinit: FLOW Language Server";
      this.item.show();
    } else if (languageId === "esh") {
      const running = this.clientManager.isEshRunning();
      this.item.text = running ? "$(terminal) ESH" : "$(circle-slash) ESH (offline)";
      this.item.tooltip = "Everyinit: ESH Language Server";
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
