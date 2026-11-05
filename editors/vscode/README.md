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
- **Direct task execution**: Trigger any defined workflow task through the Command Palette (`Ctrl+Shift+P` / `Cmd+Shift+P` -> `Everyinit: Run Task in Workspace`).

---

## ESH script support (`.esh`)

ESH is Everyinit's deterministic shell runtime for project automation scripts.

- **Embedded shell intelligence**: Flags parsing errors, unclosed quotes, and malformed control blocks (`if`/`fi`, `for`/`done`, `fn`) before execution.
- **Contextual autocompletion**: Provides completions for shell keywords, built-in utilities, and declared variables.
- **One-click execution**: Click the Run icon in the top-right editor title bar or use `Everyinit: Run Active ESH Script` to execute the active file in a dedicated terminal.

---

## Multi-instance performance

The extension is designed to run efficiently when you keep multiple VS Code windows open across different projects:

- **On-demand server startup**: Background language servers initialize only when you open a `.flow` or `.esh` file. If a project window contains only Rust or documentation files, the extension spawns zero background processes.
- **Isolated processes**: Each language client runs independently. If a project uses Evin workflows without standalone `.esh` scripts, only `evin lsp` starts.
- **Clean lifecycle management**: Closing tabs, reloading windows, or exiting VS Code terminates child processes immediately with safety timeouts, preventing orphaned background daemons.
- **Single minified bundle**: The extension client code compiles into a single minified bundle (<400 KB) with no runtime `node_modules` overhead.

---

## Commands

Access these commands through the Command Palette (`Ctrl+Shift+P` / `Cmd+Shift+P`):

| Command | Identifier | Description |
| :--- | :--- | :--- |
| **Everyinit: Run Task in Workspace** | `everyinit.runTask` | Prompts for a task name and executes `evin run <task>` in a terminal. |
| **Everyinit: Run Active ESH Script** | `everyinit.runScript` | Runs the active `.esh` script using the resolved `esh` executable. |
| **Everyinit: Restart Language Servers** | `everyinit.restartLsp` | Stops and restarts active Evin and ESH language server processes. |
| **Everyinit: Show Language Server Output** | `everyinit.showOutput` | Opens the output channel to view LSP logs and diagnostic traces. |

---

## Configuration settings

Configure extension behavior through VS Code Settings (`Ctrl+,` / `Cmd+,`):

| Setting | Default | Description |
| :--- | :--- | :--- |
| `everyinit.evin.enable` | `true` | Enables the Evin language server for `.flow` files. |
| `everyinit.evin.path` | `"evin"` | Command or absolute path to the `evin` binary. Searches `PATH` and `~/.cargo/bin`. |
| `everyinit.esh.enable` | `true` | Enables the ESH language server for `.esh` files. |
| `everyinit.esh.path` | `"esh"` | Command or absolute path to the `esh` binary. Searches `PATH` and `~/.cargo/bin`. |
| `everyinit.trace.server` | `"off"` | Sets LSP communication tracing level (`"off"`, `"messages"`, or `"verbose"`). |

---

## Status bar indicator & troubleshooting

When you edit a `.flow` or `.esh` document, a status item appears in the bottom status bar:

- `$(symbol-event) Evin`: The Evin language server is running.
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

This extension is free software licensed under the [GNU General Public License version 3](https://github.com/dukasn/everyinit/blob/main/LICENSE) (`GPL-3.0-only`).
