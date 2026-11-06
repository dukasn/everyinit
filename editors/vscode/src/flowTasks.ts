import * as vscode from "vscode";

export interface FlowDocumentTask {
  readonly name: string;
  readonly shortName: string;
  readonly module: string | null;
  readonly startLine: number;
  readonly endLine: number;
  readonly stage: string | null;
  readonly description: string | null;
}

export function parseFlowDocumentTasks(document: {
  getText(): string;
}): FlowDocumentTask[] {
  const text = document.getText();
  const lines = text.split(/\r?\n/);
  let moduleName: string | null = null;
  let inTasks = false;
  let tasksIndent = -1;
  const tasks: FlowDocumentTask[] = [];

  // 1. Detect module or namespace name if declared
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const modMatch = line.match(
      /^\s*(?:module|namespace):\s*(?:\{[^}]*name:\s*([a-zA-Z0-9_-]+)|$)/
    );
    if (modMatch && modMatch[1]) {
      moduleName = modMatch[1];
      break;
    } else if (/^\s*(?:module|namespace):/.test(line)) {
      for (let j = i + 1; j < Math.min(lines.length, i + 6); j++) {
        const nameMatch = lines[j].match(/^\s+name:\s*["']?([a-zA-Z0-9_-]+)["']?/);
        if (nameMatch) {
          moduleName = nameMatch[1];
          break;
        }
      }
      if (moduleName) break;
    }
  }

  // 2. Scan tasks section
  let currentTask: {
    name: string;
    shortName: string;
    module: string | null;
    startLine: number;
    endLine: number;
    indent: number;
    stage: string | null;
    description: string | null;
  } | null = null;

  for (let i = 0; i < lines.length; i++) {
    const rawLine = lines[i];
    const trimmed = rawLine.trim();

    if (!trimmed || trimmed.startsWith("#")) {
      continue;
    }

    const indent = rawLine.search(/\S/);

    if (!inTasks) {
      if (/^tasks:\s*(?:#.*)?$/.test(trimmed)) {
        inTasks = true;
        tasksIndent = indent;
      }
      continue;
    }

    // Indentation dropped to or below tasks: indentation -> tasks block ended
    if (indent <= tasksIndent && !trimmed.startsWith("-")) {
      inTasks = false;
      if (currentTask) {
        currentTask.endLine = i - 1;
        tasks.push({ ...currentTask });
        currentTask = null;
      }
      continue;
    }

    // Task entry under tasks: (e.g. "  build:")
    const taskMatch = rawLine.match(
      /^(\s*)([a-zA-Z0-9_-]+|"[^"]+"|'[^']+'):\s*(?:#.*)?$/
    );
    if (taskMatch) {
      const taskIndent = taskMatch[1].length;
      if (currentTask === null || taskIndent <= currentTask.indent) {
        if (currentTask) {
          currentTask.endLine = i - 1;
          tasks.push({ ...currentTask });
        }
        const shortName = taskMatch[2].replace(/^["']|["']$/g, "");
        const qualifiedName = moduleName ? `${moduleName}:${shortName}` : shortName;
        currentTask = {
          name: qualifiedName,
          shortName,
          module: moduleName,
          startLine: i,
          endLine: lines.length - 1,
          indent: taskIndent,
          stage: null,
          description: null,
        };
        continue;
      }
    }

    if (currentTask) {
      const stageMatch = trimmed.match(/^stage:\s*["']?([^"'#\n\r]+)["']?/);
      if (stageMatch) {
        currentTask.stage = stageMatch[1].trim();
      }
      const descMatch = trimmed.match(/^description:\s*["']?([^"'#\n\r]+)["']?/);
      if (descMatch) {
        currentTask.description = descMatch[1].trim();
      }
    }
  }

  if (currentTask) {
    tasks.push({ ...currentTask });
  }

  return tasks;
}

export function findFlowTaskAtLine(
  tasks: readonly FlowDocumentTask[],
  line: number
): FlowDocumentTask | null {
  for (const task of tasks) {
    if (line >= task.startLine && line <= task.endLine) {
      return task;
    }
  }
  return null;
}

export class FlowTaskCodeLensProvider implements vscode.CodeLensProvider {
  private readonly onDidChangeCodeLensesEmitter = new vscode.EventEmitter<void>();
  public readonly onDidChangeCodeLenses = this.onDidChangeCodeLensesEmitter.event;

  public refresh(): void {
    this.onDidChangeCodeLensesEmitter.fire();
  }

  public provideCodeLenses(document: vscode.TextDocument): vscode.CodeLens[] {
    if (document.languageId !== "flow" && !document.fileName.endsWith(".flow")) {
      return [];
    }

    const tasks = parseFlowDocumentTasks(document);
    const lenses: vscode.CodeLens[] = [];

    for (const task of tasks) {
      const range = new vscode.Range(task.startLine, 0, task.startLine, 0);

      // ▶ Run Task
      lenses.push(
        new vscode.CodeLens(range, {
          title: "▶ Run Task",
          command: "everyinit.runTask",
          arguments: [task.name],
          tooltip: `Execute task '${task.name}' with Evin`,
        })
      );

      // Dry Run
      lenses.push(
        new vscode.CodeLens(range, {
          title: "Dry Run",
          command: "everyinit.runTaskDryRun",
          arguments: [task.name],
          tooltip: `Simulate task '${task.name}' without executing commands`,
        })
      );
    }

    return lenses;
  }
}
