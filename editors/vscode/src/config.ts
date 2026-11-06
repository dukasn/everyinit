import * as vscode from "vscode";
import * as fs from "node:fs";
import * as path from "node:path";
import * as os from "node:os";
import { EveryinitConfig, TraceLevel } from "./types.js";

export function getConfiguration(): EveryinitConfig {
  const config = vscode.workspace.getConfiguration("everyinit");
  return {
    evinEnable: config.get<boolean>("evin.enable", true),
    evinPath: config.get<string>("evin.path", "evin").trim(),
    eshEnable: config.get<boolean>("esh.enable", true),
    eshPath: config.get<string>("esh.path", "esh").trim(),
    trace: config.get<TraceLevel>("trace.server", "off"),
  };
}

/**
 * Resolve the full path to a command/binary.
 * Searches:
 * 1. Exact path (with ${workspaceFolder} and ~ variable expansion).
 * 2. System PATH.
 * 3. Workspace target directories (target/debug/<name>, target/release/<name>).
 * 4. Cargo fallback (~/.cargo/bin/<name>).
 */
export function resolveBinaryPath(binaryName: string): string | null {
  if (!binaryName) {
    return null;
  }

  const workspaceRoot = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
  let expanded = binaryName.trim();

  // Expand ${workspaceFolder} and ${workspaceRoot}
  if (workspaceRoot) {
    expanded = expanded
      .replace(/\${workspaceFolder}/g, workspaceRoot)
      .replace(/\${workspaceRoot}/g, workspaceRoot);
  }

  // Expand ~ to user home directory
  if (expanded === "~" || expanded.startsWith("~/")) {
    expanded = path.join(os.homedir(), expanded.slice(expanded === "~" ? 1 : 2));
  }

  // If path contains directory separators, resolve directly
  if (expanded.includes("/") || expanded.includes("\\")) {
    const resolved = path.isAbsolute(expanded)
      ? expanded
      : workspaceRoot
        ? path.join(workspaceRoot, expanded)
        : path.resolve(expanded);

    if (isExecutable(resolved)) {
      return resolved;
    }
    return null;
  }

  // Check PATH environment variable
  const envPath = process.env.PATH ?? "";
  const pathDirs = envPath.split(path.delimiter);
  for (const dir of pathDirs) {
    if (!dir) continue;
    const candidate = path.join(dir, expanded);
    if (isExecutable(candidate)) {
      return candidate;
    }
  }

  // Check workspace target/debug and target/release fallback
  if (workspaceRoot) {
    const debugCandidate = path.join(workspaceRoot, "target", "debug", expanded);
    if (isExecutable(debugCandidate)) {
      return debugCandidate;
    }
    const releaseCandidate = path.join(workspaceRoot, "target", "release", expanded);
    if (isExecutable(releaseCandidate)) {
      return releaseCandidate;
    }
  }

  // Fallback to ~/.cargo/bin
  const cargoBin = path.join(os.homedir(), ".cargo", "bin", expanded);
  if (isExecutable(cargoBin)) {
    return cargoBin;
  }

  return null;
}

/**
 * Return deduplicated parent directories of the configured `evin` and `esh`
 * binaries so integrated terminals and child workflow steps can invoke both
 * tools on PATH.
 */
export function resolveConfiguredBinaryDirs(): string[] {
  const config = getConfiguration();
  const dirs: string[] = [];

  for (const candidate of [
    config.evinEnable ? resolveBinaryPath(config.evinPath) : null,
    config.eshEnable ? resolveBinaryPath(config.eshPath) : null,
  ]) {
    if (!candidate) {
      continue;
    }
    const dir = path.dirname(candidate);
    if (dir && !dirs.includes(dir)) {
      dirs.push(dir);
    }
  }

  return dirs;
}

function isExecutable(filePath: string): boolean {
  try {
    const stats = fs.statSync(filePath);
    if (!stats.isFile()) {
      return false;
    }
    // Check execute bit on POSIX systems
    if (process.platform !== "win32") {
      return (stats.mode & 0o111) !== 0;
    }
    return true;
  } catch {
    return false;
  }
}
