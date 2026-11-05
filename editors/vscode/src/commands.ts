import * as vscode from "vscode";
import { EveryinitClientManager } from "./client.js";
import { getConfiguration, resolveBinaryPath } from "./config.js";

let taskTerminal: vscode.Terminal | null = null;
let scriptTerminal: vscode.Terminal | null = null;

export function registerCommands(
  context: vscode.ExtensionContext,
  clientManager: EveryinitClientManager
): void {
  // Command: Restart LSP
  context.subscriptions.push(
    vscode.commands.registerCommand("everyinit.restartLsp", async () => {
      await clientManager.restart();
    })
  );

  // Command: Show Output
  context.subscriptions.push(
    vscode.commands.registerCommand("everyinit.showOutput", () => {
      clientManager.showOutputChannel();
    })
  );

  // Command: Run Evin Task
  context.subscriptions.push(
    vscode.commands.registerCommand("everyinit.runTask", async () => {
      const config = getConfiguration();
      const binaryPath = resolveBinaryPath(config.evinPath) ?? config.evinPath;

      const taskName = await vscode.window.showInputBox({
        title: "Everyinit: Run Task",
        prompt: "Enter the task name to execute (e.g. check, test, build, verify)",
        placeHolder: "task-name",
      });

      if (!taskName) {
        return;
      }

      if (!taskTerminal || taskTerminal.exitStatus !== undefined) {
        taskTerminal = vscode.window.createTerminal({
          name: "Everyinit Task",
          cwd: vscode.workspace.workspaceFolders?.[0]?.uri.fsPath,
        });
      }

      taskTerminal.show();
      taskTerminal.sendText(`${binaryPath} run ${taskName}`);
    })
  );

  // Command: Run ESH Script
  context.subscriptions.push(
    vscode.commands.registerCommand("everyinit.runScript", (fileUri?: vscode.Uri) => {
      const targetUri = fileUri ?? vscode.window.activeTextEditor?.document.uri;
      if (!targetUri || !targetUri.fsPath.endsWith(".esh")) {
        vscode.window.showWarningMessage("Open or select an .esh script to run with ESH.");
        return;
      }

      const config = getConfiguration();
      const binaryPath = resolveBinaryPath(config.eshPath) ?? config.eshPath;

      if (!scriptTerminal || scriptTerminal.exitStatus !== undefined) {
        scriptTerminal = vscode.window.createTerminal({
          name: "ESH Script",
          cwd: vscode.workspace.workspaceFolders?.[0]?.uri.fsPath,
        });
      }

      scriptTerminal.show();
      scriptTerminal.sendText(`${binaryPath} "${targetUri.fsPath}"`);
    })
  );
}

