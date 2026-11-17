import * as vscode from "vscode";
import { EveryinitClientManager } from "./client.js";
import { registerCommands, syncTerminalEnvironment } from "./commands.js";
import { EveryinitSidebarController } from "./sidebar.js";
import { EveryinitStatusBar } from "./statusBar.js";

let clientManager: EveryinitClientManager | null = null;

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  clientManager = new EveryinitClientManager();
  context.subscriptions.push(clientManager);

  const statusBar = new EveryinitStatusBar(clientManager);
  context.subscriptions.push(statusBar);

  syncTerminalEnvironment(context);
  registerCommands(context, clientManager);

  const sidebar = new EveryinitSidebarController(context, clientManager);
  context.subscriptions.push(sidebar);

  context.subscriptions.push(
    clientManager.onDidChangeState(() => {
      statusBar.update(vscode.window.activeTextEditor?.document.languageId);
    })
  );

  // Lazy startup: listen for documents opening
  context.subscriptions.push(
    vscode.workspace.onDidOpenTextDocument(async (document) => {
      await clientManager?.ensureClientForDocument(document);
      statusBar.update(vscode.window.activeTextEditor?.document.languageId);
    })
  );

  // Listen for active editor changes
  context.subscriptions.push(
    vscode.window.onDidChangeActiveTextEditor(async (editor) => {
      const document = editor?.document;
      const languageId = document?.languageId;
      if (document) {
        await clientManager?.ensureClientForDocument(document);
      }
      statusBar.update(languageId);
    })
  );

  // Immediately check the currently active editor
  const activeDocument = vscode.window.activeTextEditor?.document;
  if (activeDocument) {
    void clientManager.ensureClientForDocument(activeDocument).then(() => {
      statusBar.update(vscode.window.activeTextEditor?.document.languageId);
    });
  }

  // Check already opened documents (in case files were restored on startup)
  for (const document of vscode.workspace.textDocuments) {
    void clientManager.ensureClientForDocument(document).then(() => {
      statusBar.update(vscode.window.activeTextEditor?.document.languageId);
    });
  }

  // React to configuration changes
  context.subscriptions.push(
    vscode.workspace.onDidChangeConfiguration(async (event) => {
      if (event.affectsConfiguration("everyinit")) {
        syncTerminalEnvironment(context);
        await clientManager?.restart();
        statusBar.update(vscode.window.activeTextEditor?.document.languageId);
      }
    })
  );
}

export async function deactivate(): Promise<void> {
  if (clientManager) {
    await clientManager.stopAll();
    clientManager = null;
  }
}
