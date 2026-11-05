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
 * 1. Exact path if absolute or relative to workspace.
 * 2. System PATH.
 * 3. Cargo fallback (~/.cargo/bin/<name>).
 */
export function resolveBinaryPath(binaryName: string): string | null {
  if (!binaryName) {
    return null;
  }

  // If path contains separators, resolve directly
  if (binaryName.includes("/") || binaryName.includes("\\")) {
    const resolved = path.isAbsolute(binaryName)
      ? binaryName
      : vscode.workspace.workspaceFolders?.[0]
        ? path.join(vscode.workspace.workspaceFolders[0].uri.fsPath, binaryName)
        : path.resolve(binaryName);

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
    const candidate = path.join(dir, binaryName);
    if (isExecutable(candidate)) {
      return candidate;
    }
  }

  // Fallback to ~/.cargo/bin
  const cargoBin = path.join(os.homedir(), ".cargo", "bin", binaryName);
  if (isExecutable(cargoBin)) {
    return cargoBin;
  }

  return null;
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

