# Everyinit for Visual Studio Code

Everyinit brings repository commands, dependencies, Git hooks, and delivery steps together into workflows with Evin, alongside automation scripts with ESH.

This extension provides unified editor support for both tools in a single package. It adds syntax highlighting, real-time diagnostic checks, completions, symbol navigation, hover documentation, and terminal execution for Evin workflows (`.flow`) and ESH scripts (`.esh`).

---

## Local development

The extension runs the language servers over standard input and output (`stdio`). During local canary development, it connects directly to the binaries built in your checkout.

### Building the extension

Run the Evin workspace build task to bundle the extension code:

```sh
# From the repository root
cargo evin-dev run vscode:build

# Or directly in editors/vscode
npm run build
```

To watch for changes during active extension development:

```sh
npm run watch
```

### Running in VS Code

1. Open the repository or the `editors/vscode` directory in VS Code.
2. Press `F5` to open an Extension Development Host window with the extension loaded.
3. Open any `.flow` or `.esh` file in the test window to activate language features.

By default, the extension resolves `evin` and `esh` from your `PATH` or `~/.cargo/bin`. You can point directly to local build artifacts in your VS Code settings:

```json
{
  "everyinit.evin.path": "${workspaceFolder}/target/debug/evin",
  "everyinit.esh.path": "${workspaceFolder}/target/debug/esh"
}
```

---

## Evin workflow support (`.flow`)

The extension recognizes all canonical Evin workflow files, including `.evin/workspace.flow`, `.evin/gitrules.flow`, `.evin/release.flow`, and custom module declarations.

- **Real-time diagnostics**: Catches syntax errors, unknown task properties, missing step fields, and circular dependencies in task graphs (`needs: [...]`) as you type.
- **Action schema hover**: Hover over built-in actions (`run:`, `use:`, `with:`) or workflow keys to view documentation, parameter types, and required inputs directly in the editor.
- **Intelligent autocompletion**: Suggests defined stage names, workspace tasks, preset templates, and built-in action options.
- **Workspace navigation**: Inspect tasks and stages in the Outline panel, or jump directly between task definitions and dependency references (`F12`).
- **Direct task execution**: Trigger any defined workflow task through the Everyinit sidebar or Command Palette (`Ctrl+Shift+P` / `Cmd+Shift+P` -> `Everyinit: Run Task in Workspace`).

---

## Activity Bar sidebar & execution traces

Opening an Everyinit workspace adds the **Everyinit** sidebar in the Activity Bar with three views:

- **Tasks**: Lists workspace and module tasks resolved by `evin lsp`, grouped by module or by stage, with live search filtering, `needs` / hook / watch metadata, latest trace status and duration badges, one-click terminal execution (`run`, `--dry-run`, `--parallel`, `--sequential`, `explain`, `check`), and direct navigation to the `.flow` definition.
- **Execution Trace**: Watches `.evin/trace.json` and module-scoped `<module>/.evin/trace.json` files automatically and summarizes wall-clock duration, cumulative task time, concurrency factor, and per-task outcomes. Clicking the summary or timeline button opens an interactive waterfall panel with worker slot concurrency lanes, filtering, sorting, and one-click task re-runs.
- **Workspace & Scripts**: Surfaces quick workspace actions (`evin check -w`, `evin sync`), all discovered `.flow` manifests with live diagnostic counts, and workspace `.esh` scripts with inline run buttons.

---

## ESH script support (`.esh`)

ESH is Everyinit's deterministic shell runtime for project automation scripts.

- **Embedded shell intelligence**: Flags parsing errors, unclosed quotes, and malformed control blocks (`if`/`fi`, `for`/`done`, `fn`) before execution.
- **Contextual autocompletion**: Provides completions for shell keywords, built-in utilities, and declared variables.
- **One-click execution**: Click the Run icon in the top-right editor title bar or use `Everyinit: Run Active ESH Script` to execute the active file in a dedicated terminal.

---

## Multi-instance performance

The extension is designed to run efficiently when you keep multiple VS Code windows open across different projects:

- **On-demand server startup**: Background language servers initialize only when a workspace contains `.evin/workspace.flow`, when you open the Everyinit sidebar, or when you open a `.flow` or `.esh` file. Unrelated project windows spawn zero background processes.
- **Isolated processes**: Each language client runs independently. If a project uses Evin workflows without standalone `.esh` scripts, only `evin lsp` starts.
- **Clean lifecycle management**: Closing tabs, reloading windows, or exiting VS Code terminates child processes immediately with safety timeouts, preventing orphaned background daemons.
- **Single minified bundle**: The extension client code compiles into a single minified bundle (<400 KB) with no runtime `node_modules` overhead.

---

## Commands

Access these commands through the Command Palette (`Ctrl+Shift+P` / `Cmd+Shift+P`):

| Command | Identifier | Description |
| :--- | :--- | :--- |
| **Everyinit: Run Task in Workspace** | `everyinit.runTask` | Selects a workspace or module task and executes `evin run <task>` in a terminal. |
| **Everyinit: Dry-Run Task** | `everyinit.runTaskDryRun` | Executes `evin --dry-run run <task>` in a terminal. |
| **Everyinit: Explain Task** | `everyinit.explainTask` | Renders task execution order and metadata via `evin explain <task>`. |
| **Everyinit: Check Workspace Configuration** | `everyinit.checkWorkspace` | Validates workspace manifests via `evin check -w`. |
| **Everyinit: Sync Git Hooks and Environment** | `everyinit.syncWorkspace` | Synchronizes configured hooks and environment files via `evin sync`. |
| **Everyinit: Show Execution Trace Timeline** | `everyinit.openTraceTimeline` | Opens the interactive `.evin/trace.json` waterfall timeline view. |
| **Everyinit: Run ESH Script** | `everyinit.runScript` | Runs the selected or active `.esh` script using the resolved `esh` executable. |
| **Everyinit: Restart Language Servers** | `everyinit.restartLsp` | Stops and restarts active Evin and ESH language server processes. |
| **Everyinit: Show Language Server Output** | `everyinit.showOutput` | Opens the output channel to view LSP logs and diagnostic traces. |

---

## Configuration settings

Configure extension behavior through VS Code Settings (`Ctrl+,` / `Cmd+,`):

| Setting | Default | Description |
| :--- | :--- | :--- |
| `everyinit.evin.enable` | `true` | Enables the FLOW language server for `.flow` files. |
| `everyinit.evin.path` | `"evin"` | Command or absolute path to the `evin` binary (powers FLOW workflows and LSP). Searches `PATH` and `~/.cargo/bin`. |
| `everyinit.esh.enable` | `true` | Enables the ESH language server for `.esh` files. |
| `everyinit.esh.path` | `"esh"` | Command or absolute path to the `esh` binary. Searches `PATH` and `~/.cargo/bin`. |
| `everyinit.trace.server` | `"off"` | Sets LSP communication tracing level (`"off"`, `"messages"`, or `"verbose"`). |

---

## Status bar indicator & troubleshooting

When you edit a `.flow` or `.esh` document, a status item appears in the bottom status bar:

- `$(everyinit-logo) FLOW`: The FLOW language server is running.
- `$(terminal) ESH`: The ESH language server is running.
- `$(circle-slash) ... (offline)`: The language server could not start.

Clicking the status bar item opens a quick menu to inspect logs, restart servers, or run project commands.

### Troubleshooting an offline server

If the status bar reports an offline server:

1. Confirm the binary runs in your terminal: `evin --version` or `esh --version`.
2. Point `everyinit.evin.path` or `everyinit.esh.path` to your compiled target binary (for example, `${workspaceFolder}/target/debug/evin`).
3. Open `Everyinit: Show Language Server Output` from the Command Palette to inspect startup errors or communication traces.

---

## Licensing

This extension is free software licensed under the [GNU General Public License version 3](https://github.com/dukasn/everyinit/blob/main/LICENSE) (`GPL-3.0-only`). Everyinit logo artwork, editor icons, and the `.woff` icon font in `assets/` are © 2026 Eduardo Marques Santos, licensed under [CC BY 4.0](https://github.com/dukasn/everyinit/blob/main/LICENSES/CC-BY-4.0.txt); see [TRADEMARKS.md](https://github.com/dukasn/everyinit/blob/main/TRADEMARKS.md) for brand guidelines.
