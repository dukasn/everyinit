import * as vscode from "vscode";
import { EveryinitClientManager } from "./client.js";

export class EveryinitStatusBar implements vscode.Disposable {
  private item: vscode.StatusBarItem;
  private clientManager: EveryinitClientManager;
  private disposables: vscode.Disposable[] = [];
  private readonly runningTasks = new Map<string, string>();
  private readonly runLabels = new Map<string, string>();
  private lastRun: { readonly task: string; readonly succeeded: boolean } | null = null;

  constructor(clientManager: EveryinitClientManager) {
    this.clientManager = clientManager;
    this.item = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 100);
    this.item.command = "everyinit.statusMenu";
    this.disposables.push(this.item);
    this.disposables.push(
      this.clientManager.onDidReceivePipelineEvent((event) => {
        if (event.kind === "run_started") {
          const tasks = event.payload.tasks;
          const taskLabel = Array.isArray(tasks) && tasks.every((task) => typeof task === "string")
            ? tasks.length === 1
              ? tasks[0]
              : tasks.length > 1
                ? `${tasks[0]} +${tasks.length - 1}`
                : "Workspace task"
            : "Workspace task";
          this.runLabels.set(event.run_id, taskLabel);
          this.runningTasks.set(
            event.run_id,
            shortenTaskLabel(taskLabel)
          );
        } else if (event.kind === "task_started") {
          const name = event.payload.name;
          if (typeof name === "string") {
            this.runningTasks.set(event.run_id, shortenTaskLabel(name));
          }
        } else if (event.kind === "task_step_started") {
          const name = event.payload.name;
          if (typeof name === "string") {
            this.runningTasks.set(
              event.run_id,
              `${shortenTaskLabel(name)} ${String(event.payload.step_number ?? "?")}/${String(event.payload.step_count ?? "?")}`
            );
          }
        } else if (event.kind === "run_completed") {
          this.lastRun = {
            task: this.runLabels.get(event.run_id) ?? "Workspace task",
            succeeded: event.payload.succeeded === true,
          };
          this.runningTasks.delete(event.run_id);
          this.runLabels.delete(event.run_id);
        }
        this.update(vscode.window.activeTextEditor?.document.languageId);
      })
    );
    this.disposables.push(
      this.clientManager.onDidFinishTaskRun(({ taskName, runId, success }) => {
        if (runId) {
          if (success !== undefined && this.runLabels.has(runId)) {
            this.lastRun = {
              task: this.runLabels.get(runId) ?? taskName,
              succeeded: success,
            };
          }
          this.runningTasks.delete(runId);
          this.runLabels.delete(runId);
        } else {
          for (const [activeRunId, runningTask] of this.runningTasks) {
            if (runningTask === taskName) {
              this.runningTasks.delete(activeRunId);
              this.runLabels.delete(activeRunId);
            }
          }
        }
        this.update(vscode.window.activeTextEditor?.document.languageId);
      })
    );

    // Register status bar menu command
    this.disposables.push(
      vscode.commands.registerCommand("everyinit.statusMenu", async () => {
        const actions = [
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
            label: "$(play) Run Task…",
            description: "Choose a task and how to run it",
            command: "everyinit.runTask",
          },
          {
            label: "$(check-all) Check Workspace",
            description: "Validate workspace configuration",
            command: "everyinit.checkWorkspace",
          },
          {
            label: "$(check) Show Problems",
            description: "Open diagnostics from Evin and ESH",
            command: "workbench.actions.view.problems",
          },
          {
            label: "$(graph-line) Show Execution Trace Timeline",
            description: "Inspect the latest .evin/trace.json waterfall",
            command: "everyinit.openTraceTimeline",
          },
        ];
        if (vscode.window.activeTextEditor?.document.languageId === "esh") {
          actions.push({
            label: "$(terminal) Run ESH Script",
            description: "Execute the active script with ESH",
            command: "everyinit.runScript",
          });
        }
        const choice = await vscode.window.showQuickPick(actions);

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
    if (!vscode.workspace.workspaceFolders?.length && languageId !== "flow" && languageId !== "esh") {
      this.item.hide();
      return;
    }

    const activeTask = [...this.runningTasks.values()].at(-1);
    if (activeTask) {
      this.item.text = `$(sync~spin) ${activeTask}`;
      this.item.tooltip = `Everyinit: Running ${activeTask}\nClick for workspace actions`;
      this.item.show();
      return;
    }

    if (this.lastRun?.succeeded === false) {
      this.item.text = "$(error) Evin run failed";
      this.item.tooltip = `Everyinit: ${this.lastRun.task} failed. Open the trace or Problems panel for details.`;
      this.item.show();
      return;
    }

    if (languageId === "flow") {
      const running = this.clientManager.isEvinRunning();
      this.item.text = running ? "$(everyinit-logo) FLOW ready" : "$(circle-slash) FLOW offline";
      this.item.tooltip = running
        ? "Evin language server is ready. Click for workspace actions."
        : "Evin language server is offline. Open a .flow file or restart the server.";
    } else if (languageId === "esh") {
      const running = this.clientManager.isEshRunning();
      this.item.text = running ? "$(terminal) ESH ready" : "$(circle-slash) ESH offline";
      this.item.tooltip = running
        ? "ESH language server is ready. Click for workspace actions."
        : "ESH language server is offline. Open a .esh file or restart the server.";
    } else {
      const evinRunning = this.clientManager.isEvinRunning();
      this.item.text = this.lastRun?.succeeded
        ? "$(check) Evin ready"
        : "$(everyinit-logo) Evin";
      if (this.lastRun?.succeeded) {
        this.item.tooltip = `Last run succeeded: ${this.lastRun.task}. Click for workspace actions.`;
      } else if (evinRunning) {
        this.item.tooltip = "Everyinit is ready. Click for workspace actions.";
      } else {
        this.item.tooltip =
          "Everyinit workspace actions. Open a .flow or .esh file to start its language server.";
      }
    }
    if (this.lastRun?.succeeded && (languageId === "flow" || languageId === "esh")) {
      this.item.tooltip = `Last run succeeded: ${this.lastRun.task}. ${this.item.tooltip}`;
    }
    this.item.show();
  }

  public dispose(): void {
    for (const d of this.disposables) {
      d.dispose();
    }
    this.disposables = [];
  }
}

function shortenTaskLabel(label: string): string {
  return label.length > 28 ? `${label.slice(0, 25)}…` : label;
}
