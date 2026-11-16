import * as fs from "node:fs";
import * as path from "node:path";
import * as vscode from "vscode";
import { EveryinitClientManager } from "./client.js";
import { getConfiguration, resolveBinaryPath, resolveConfiguredBinaryDirs } from "./config.js";
import { LspLocation } from "./types.js";

interface ManagedTerminal {
  readonly terminal: vscode.Terminal;
  readonly fallbackEnv: Readonly<Record<string, string>>;
}

let taskTerminal: ManagedTerminal | null = null;
let scriptTerminal: ManagedTerminal | null = null;

const NEW_TERMINAL_READY_TIMEOUT_MS = 350;

interface LinuxTerminalProfileConfig {
  readonly path?: unknown;
  readonly args?: unknown;
  readonly env?: unknown;
}

export function syncTerminalEnvironment(context: vscode.ExtensionContext): void {
  const collection = context.environmentVariableCollection;
  collection.clear();
  const dirs = resolveConfiguredBinaryDirs();
  if (dirs.length > 0) {
    collection.prepend("PATH", `${dirs.join(path.delimiter)}${path.delimiter}`);
  }
  if (!isNoColorEnabled()) {
    collection.replace("COLORTERM", resolvePreferredColorTerm());
  }
}

export async function executeEvinCli(subcommandArgs: readonly string[]): Promise<void> {
  const config = getConfiguration();
  const binaryPath = resolveBinaryPath(config.evinPath) ?? config.evinPath;
  const binDirs = resolveConfiguredBinaryDirs();

  taskTerminal = await runInManagedTerminal(taskTerminal, "Everyinit Task", (fallbackEnv) =>
    formatCommandWithPath(binDirs, [binaryPath, ...subcommandArgs], fallbackEnv)
  );
}

export async function executeEvinTask(
  clientManager: EveryinitClientManager,
  taskName: string,
  options: { readonly dryRun?: boolean; readonly parallel?: boolean; readonly sequential?: boolean } = {}
): Promise<void> {
  const result = await clientManager.runTask(taskName, options);
  if (result) {
    if (!result.success && !result.cancelled) {
      vscode.window.showErrorMessage(
        `Evin task '${taskName}' failed${result.exitCode === null ? "" : ` (exit code ${result.exitCode})`}.`
      );
    }
    return;
  }

  const args = options.dryRun
    ? ["--dry-run", "run", taskName]
    : ["run", ...(options.parallel ? ["--parallel"] : []), ...(options.sequential ? ["--sequential"] : []), taskName];
  await executeEvinCli(args);
}

export async function executeEshFile(scriptFsPath: string): Promise<void> {
  const config = getConfiguration();
  const binaryPath = resolveBinaryPath(config.eshPath) ?? config.eshPath;
  const binDirs = resolveConfiguredBinaryDirs();

  scriptTerminal = await runInManagedTerminal(scriptTerminal, "ESH Script", (fallbackEnv) =>
    formatCommandWithPath(binDirs, [binaryPath, scriptFsPath], fallbackEnv)
  );
}

export async function openLocationInEditor(location: LspLocation): Promise<void> {
  const uri = vscode.Uri.parse(location.uri);
  const start = new vscode.Position(location.range.start.line, location.range.start.character);
  const end = new vscode.Position(location.range.end.line, location.range.end.character);
  const range = new vscode.Range(start, end);
  const doc = await vscode.workspace.openTextDocument(uri);
  await vscode.window.showTextDocument(doc, {
    selection: range,
    preview: false,
  });
}

export function extractTaskNameArg(arg: unknown): string | null {
  if (typeof arg === "string" && arg.trim()) {
    return arg.trim();
  }
  if (arg && typeof arg === "object") {
    const record = arg as Record<string, unknown>;
    if (typeof record.taskName === "string" && record.taskName.trim()) {
      return record.taskName.trim();
    }
    if (record.task && typeof record.task === "object") {
      const task = record.task as Record<string, unknown>;
      if (typeof task.name === "string" && task.name.trim()) {
        return task.name.trim();
      }
    }
    if (record.span && typeof record.span === "object") {
      const span = record.span as Record<string, unknown>;
      if (typeof span.name === "string" && span.name.trim()) {
        return span.name.trim();
      }
    }
  }
  return null;
}

export function extractUriArg(arg: unknown): vscode.Uri | null {
  if (arg instanceof vscode.Uri) {
    return arg;
  }
  if (arg && typeof arg === "object") {
    const record = arg as Record<string, unknown>;
    if (record.resourceUri instanceof vscode.Uri) {
      return record.resourceUri;
    }
    if (typeof record.uri === "string" && record.uri) {
      return vscode.Uri.parse(record.uri);
    }
  }
  return null;
}

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

  // Command: Run Evin Task (accepts optional task argument from Sidebar / Webview)
  context.subscriptions.push(
    vscode.commands.registerCommand("everyinit.runTask", async (arg?: unknown) => {
      const directTask = extractTaskNameArg(arg);
      const taskName = directTask ?? (await promptForTaskName(clientManager));
      if (!taskName) {
        return;
      }
      await executeEvinTask(clientManager, taskName);
    })
  );

  // Command: Dry-Run Task
  context.subscriptions.push(
    vscode.commands.registerCommand("everyinit.runTaskDryRun", async (arg?: unknown) => {
      const taskName = extractTaskNameArg(arg) ?? (await promptForTaskName(clientManager));
      if (!taskName) {
        return;
      }
      await executeEvinTask(clientManager, taskName, { dryRun: true });
    })
  );

  // Command: Run Task in Parallel
  context.subscriptions.push(
    vscode.commands.registerCommand("everyinit.runTaskParallel", async (arg?: unknown) => {
      const taskName = extractTaskNameArg(arg) ?? (await promptForTaskName(clientManager));
      if (!taskName) {
        return;
      }
      await executeEvinTask(clientManager, taskName, { parallel: true });
    })
  );

  // Command: Run Task Sequentially
  context.subscriptions.push(
    vscode.commands.registerCommand("everyinit.runTaskSequential", async (arg?: unknown) => {
      const taskName = extractTaskNameArg(arg) ?? (await promptForTaskName(clientManager));
      if (!taskName) {
        return;
      }
      await executeEvinTask(clientManager, taskName, { sequential: true });
    })
  );

  // Command: Explain Task
  context.subscriptions.push(
    vscode.commands.registerCommand("everyinit.explainTask", async (arg?: unknown) => {
      const taskName = extractTaskNameArg(arg) ?? (await promptForTaskName(clientManager));
      if (!taskName) {
        return;
      }
      await executeEvinCli(["explain", taskName]);
    })
  );

  // Command: Check Single Task
  context.subscriptions.push(
    vscode.commands.registerCommand("everyinit.checkTask", async (arg?: unknown) => {
      const taskName = extractTaskNameArg(arg) ?? (await promptForTaskName(clientManager));
      if (!taskName) {
        return;
      }
      await executeEvinCli(["check", taskName]);
    })
  );

  // Command: Check Workspace
  context.subscriptions.push(
    vscode.commands.registerCommand("everyinit.checkWorkspace", async () => {
      await executeEvinCli(["check", "-w"]);
    })
  );

  // Command: Sync Workspace Hooks & Environment
  context.subscriptions.push(
    vscode.commands.registerCommand("everyinit.syncWorkspace", async () => {
      await executeEvinCli(["sync"]);
    })
  );

  // Command: Initialize Workspace
  context.subscriptions.push(
    vscode.commands.registerCommand("everyinit.initWorkspace", async () => {
      await executeEvinCli(["init", "-w"]);
    })
  );

  // Command: Run ESH Script (accepts Uri or tree item from Sidebar / Editor Title)
  context.subscriptions.push(
    vscode.commands.registerCommand("everyinit.runScript", async (arg?: unknown) => {
      const targetUri = extractUriArg(arg) ?? vscode.window.activeTextEditor?.document.uri;
      if (!targetUri || !targetUri.fsPath.endsWith(".esh")) {
        vscode.window.showWarningMessage("Open or select an .esh script to run with ESH.");
        return;
      }
      await executeEshFile(targetUri.fsPath);
    })
  );

  // Command: Run Flow Task (Editor Title Button, Context Menu, CodeLens, or Command Palette)
  context.subscriptions.push(
    vscode.commands.registerCommand("everyinit.runFlowTask", async (arg?: unknown) => {
      // 1. If explicit task argument is provided, execute directly
      const directTask = extractTaskNameArg(arg);
      if (directTask) {
        await executeEvinTask(clientManager, directTask);
        return;
      }

      const editor = vscode.window.activeTextEditor;
      if (
        !editor ||
        (!editor.document.fileName.endsWith(".flow") && editor.document.languageId !== "flow")
      ) {
        // Fallback to general task picker if not inside a flow file
        const taskName = await promptForTaskName(clientManager);
        if (taskName) {
          await executeEvinTask(clientManager, taskName);
        }
        return;
      }

      const codeLenses =
        (await vscode.commands.executeCommand<vscode.CodeLens[]>(
          "vscode.executeCodeLensProvider",
          editor.document.uri
        )) ?? [];
      const taskByName = new Map<string, { name: string; startLine: number }>();
      for (const lens of codeLenses) {
        const taskName = extractTaskNameArg(lens.command?.arguments?.[0]);
        if (taskName) {
          taskByName.set(taskName, { name: taskName, startLine: lens.range.start.line });
        }
      }
      const tasks = [...taskByName.values()];
      const cursorLine = editor.selection.active.line;
      const taskAtCursor = tasks.find((task) => task.startLine === cursorLine);

      // 2. If cursor is inside a task declaration, run that task immediately
      if (taskAtCursor) {
        await executeEvinTask(clientManager, taskAtCursor.name);
        return;
      }

      // 3. If there is only one task in the file, run it
      if (tasks.length === 1) {
        await executeEvinTask(clientManager, tasks[0].name);
        return;
      }

      // 4. If multiple tasks exist in this file, prompt with this file's tasks at top
      if (tasks.length > 1) {
        const picked = await vscode.window.showQuickPick(
          tasks.map((t) => ({
            label: t.name,
            taskName: t.name,
          })),
          {
            title: "Everyinit: Run Flow Task",
            placeHolder: "Select a task defined in this Flow file to run",
          }
        );
        if (picked?.taskName) {
          await executeEvinTask(clientManager, picked.taskName);
        }
        return;
      }

      // 5. If no tasks defined in this file, prompt for any workspace task
      const fallbackTask = await promptForTaskName(clientManager);
      if (fallbackTask) {
        await executeEvinTask(clientManager, fallbackTask);
      }
    })
  );
}

async function promptForTaskName(clientManager: EveryinitClientManager): Promise<string | null> {
  const workspaceRoot = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
  const hasWorkspaceManifest =
    Boolean(workspaceRoot) &&
    fs.existsSync(path.join(workspaceRoot!, ".evin", "workspace.flow"));

  if (hasWorkspaceManifest || clientManager.isEvinRunning()) {
    const catalog = await clientManager.fetchWorkspaceCatalog();
    if (catalog && catalog.tasks.length > 0) {
      const picked = await vscode.window.showQuickPick(
        catalog.tasks.map((task) => ({
          label: task.name,
          description: task.stage ?? undefined,
          detail: task.description ?? task.sourcePath,
          taskName: task.name,
        })),
        {
          title: "Everyinit: Run Task",
          placeHolder: "Select a workspace or module task to run",
          matchOnDescription: true,
          matchOnDetail: true,
        }
      );
      return picked?.taskName ?? null;
    }
  }

  const entered = await vscode.window.showInputBox({
    title: "Everyinit: Run Task",
    prompt: "Enter the task name to execute (e.g. check, test, build, verify)",
    placeHolder: "task-name",
  });
  return entered?.trim() || null;
}

async function runInManagedTerminal(
  existing: ManagedTerminal | null,
  name: string,
  buildCommand: (fallbackEnv: Readonly<Record<string, string>>) => string
): Promise<ManagedTerminal> {
  const isNewTerminal = !existing || existing.terminal.exitStatus !== undefined;
  const managed = isNewTerminal ? createEveryinitTerminal(name) : existing;

  managed.terminal.show();
  if (isNewTerminal) {
    await waitForTerminalReady(managed.terminal);
  }

  const commandLine = buildCommand(managed.fallbackEnv);
  if (managed.terminal.shellIntegration) {
    managed.terminal.shellIntegration.executeCommand(commandLine);
  } else {
    managed.terminal.sendText(commandLine);
  }

  return managed;
}

function createEveryinitTerminal(name: string): ManagedTerminal {
  const options: vscode.TerminalOptions = {
    name,
    cwd: vscode.workspace.workspaceFolders?.[0]?.uri.fsPath,
  };

  const noColor = isNoColorEnabled();
  const baseEnv: Record<string, string | null | undefined> = noColor
    ? {}
    : { COLORTERM: resolvePreferredColorTerm() };

  const flatpakShell = resolveFlatpakProfileOverrides();
  if (flatpakShell) {
    options.shellPath = flatpakShell.shellPath;
    options.shellArgs = flatpakShell.shellArgs;
    Object.assign(baseEnv, flatpakShell.env);
  }

  if (Object.keys(baseEnv).length > 0) {
    options.env = baseEnv;
  }

  // Only fall back to per-command TERM/COLORTERM when running inside Flatpak with an
  // opaque wrapper profile where `--env=` could not be injected at terminal creation.
  const needsInlineColorFallback =
    !noColor && !flatpakShell && (isRunningInFlatpak() || process.env.TERM === "dumb");

  const fallbackEnv: Record<string, string> = needsInlineColorFallback
    ? {
        TERM: resolvePreferredTerm(),
        COLORTERM: resolvePreferredColorTerm(),
      }
    : {};

  return {
    terminal: vscode.window.createTerminal(options),
    fallbackEnv,
  };
}

/**
 * `flatpak-spawn --host` resets `TERM` to `dumb` and strips `COLORTERM` unless
 * explicitly forwarded via `--env=`, which disables ANSI colors in Evin/Cargo
 * and triggers GNU Readline `horizontal-scroll-mode` prompt corruption.
 */
function resolveFlatpakProfileOverrides(): {
  shellPath: string;
  shellArgs: string[];
  env: Record<string, string | null>;
} | null {
  if (process.platform !== "linux") {
    return null;
  }

  const termConfig = vscode.workspace.getConfiguration("terminal.integrated");
  const defaultProfileName = termConfig.get<string>("defaultProfile.linux");
  if (!defaultProfileName) {
    return null;
  }

  const profiles = termConfig.get<Record<string, LinuxTerminalProfileConfig | null>>(
    "profiles.linux"
  );
  const profile = profiles?.[defaultProfileName];
  if (!profile || typeof profile !== "object") {
    return null;
  }

  const configuredPath =
    typeof profile.path === "string"
      ? profile.path
      : Array.isArray(profile.path)
        ? profile.path.find((entry): entry is string => typeof entry === "string")
        : undefined;

  if (!configuredPath || path.basename(configuredPath) !== "flatpak-spawn") {
    return null;
  }

  const rawArgs = Array.isArray(profile.args)
    ? profile.args.filter((arg): arg is string => typeof arg === "string")
    : [];

  const injectedFlags: string[] = [];
  if (!rawArgs.some((arg) => arg.startsWith("--env=TERM="))) {
    injectedFlags.push(`--env=TERM=${resolvePreferredTerm()}`);
  }
  if (!isNoColorEnabled() && !rawArgs.some((arg) => arg.startsWith("--env=COLORTERM="))) {
    injectedFlags.push(`--env=COLORTERM=${resolvePreferredColorTerm()}`);
  }

  const profileEnv: Record<string, string | null> = {};
  if (profile.env && typeof profile.env === "object" && !Array.isArray(profile.env)) {
    for (const [key, value] of Object.entries(profile.env)) {
      if (typeof value === "string" || value === null) {
        profileEnv[key] = value;
      }
    }
  }

  return {
    shellPath: configuredPath,
    shellArgs: [...injectedFlags, ...rawArgs],
    env: profileEnv,
  };
}

async function waitForTerminalReady(terminal: vscode.Terminal): Promise<void> {
  await terminal.processId;
  if (terminal.shellIntegration) {
    return;
  }

  await new Promise<void>((resolve) => {
    let settled = false;
    let subscription: vscode.Disposable | undefined;

    const finish = () => {
      if (settled) {
        return;
      }
      settled = true;
      clearTimeout(timer);
      subscription?.dispose();
      resolve();
    };

    const timer = setTimeout(finish, NEW_TERMINAL_READY_TIMEOUT_MS);
    if (typeof vscode.window.onDidChangeTerminalShellIntegration === "function") {
      subscription = vscode.window.onDidChangeTerminalShellIntegration((event) => {
        if (event.terminal === terminal) {
          finish();
        }
      });
    }
  });
}

function isRunningInFlatpak(): boolean {
  return (
    process.platform === "linux" &&
    (Boolean(process.env.FLATPAK_ID) || fs.existsSync("/.flatpak-info"))
  );
}

function isNoColorEnabled(): boolean {
  const value = process.env.NO_COLOR;
  return value !== undefined && value !== "";
}

function resolvePreferredTerm(): string {
  const envTerm = process.env.TERM?.trim();
  return envTerm && envTerm !== "dumb" ? envTerm : "xterm-256color";
}

function resolvePreferredColorTerm(): string {
  const envColorTerm = process.env.COLORTERM?.trim();
  return envColorTerm ? envColorTerm : "truecolor";
}

function formatCommandWithPath(
  binDirs: readonly string[],
  args: readonly string[],
  extraEnv: Readonly<Record<string, string>> = {}
): string {
  const quotedArgs = args.map(quoteShellArg).join(" ");
  if (process.platform === "win32") {
    return quotedArgs;
  }

  // Inline `env` survives Flatpak/Toolbx/container host-spawn wrappers that strip
  // TerminalOptions.env while staying compatible across bash/zsh/fish/sh.
  const envAssignments: string[] = [];
  if (binDirs.length > 0) {
    const escapedDirs = binDirs.map((dir) => dir.replace(/(["\\$`])/g, "\\$1")).join(":");
    envAssignments.push(`PATH="${escapedDirs}:$PATH"`);
  }
  for (const [key, value] of Object.entries(extraEnv)) {
    envAssignments.push(`${key}=${quoteShellArg(value)}`);
  }

  if (envAssignments.length === 0) {
    return quotedArgs;
  }
  return `env ${envAssignments.join(" ")} ${quotedArgs}`;
}

function quoteShellArg(arg: string): string {
  if (/^[a-zA-Z0-9_./:@=-]+$/.test(arg)) {
    return arg;
  }
  if (process.platform === "win32") {
    return `"${arg.replace(/"/g, '\\"')}"`;
  }
  return `'${arg.replace(/'/g, "'\\''")}'`;
}
