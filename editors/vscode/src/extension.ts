import * as vscode from "vscode";
import { EveryinitClientManager } from "./client.js";
import { registerCommands } from "./commands.js";
import { EveryinitStatusBar } from "./statusBar.js";

let clientManager: EveryinitClientManager | null = null;

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  clientManager = new EveryinitClientManager();
  context.subscriptions.push(clientManager);

  const statusBar = new EveryinitStatusBar(clientManager);
  context.subscriptions.push(statusBar);

  registerCommands(context, clientManager);

  // Lazy startup: listen for documents opening
  context.subscriptions.push(
    vscode.workspace.onDidOpenTextDocument(async (document) => {
      if (document.languageId === "flow" || document.languageId === "esh") {
        await clientManager?.ensureClientForLanguage(document.languageId);
        statusBar.update(vscode.window.activeTextEditor?.document.languageId);
      }
    })
  );

  // Listen for active editor changes
  context.subscriptions.push(
    vscode.window.onDidChangeActiveTextEditor(async (editor) => {
      const languageId = editor?.document.languageId;
      if (languageId === "flow" || languageId === "esh") {
        await clientManager?.ensureClientForLanguage(languageId);
      }
      statusBar.update(languageId);
    })
  );

  // Immediately check the currently active editor
  const activeLanguage = vscode.window.activeTextEditor?.document.languageId;
  if (activeLanguage === "flow" || activeLanguage === "esh") {
    void clientManager.ensureClientForLanguage(activeLanguage).then(() => {
      statusBar.update(vscode.window.activeTextEditor?.document.languageId);
    });
  }

  // Check already opened documents (in case files were restored on startup)
  for (const document of vscode.workspace.textDocuments) {
    if (document.languageId === "flow" || document.languageId === "esh") {
      void clientManager.ensureClientForLanguage(document.languageId).then(() => {
        statusBar.update(vscode.window.activeTextEditor?.document.languageId);
      });
    }
  }

  // React to configuration changes
  context.subscriptions.push(
    vscode.workspace.onDidChangeConfiguration(async (event) => {
      if (event.affectsConfiguration("everyinit")) {
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

