# Everyinit for Visual Studio Code

Everyinit adds editor support for Evin workflow files (`.flow`) and ESH scripts
(`.esh`). It provides syntax highlighting, language server diagnostics and
completions, workflow navigation, task execution, and ESH script execution.

## Install

Install [Everyinit from the Visual Studio Marketplace](https://marketplace.visualstudio.com/items?itemName=everyinit.everyinit).
The extension requires VS Code 1.90 or later.

The extension package runs in VS Code's extension host. Evin provides the
`.flow` language server and workflow commands; ESH provides `.esh` language
features and standalone script execution. Install both tools to use the full
extension. The current official binaries and tested support target Linux
x86_64. Install them with the signed canary installer:

```sh
curl -fsSL https://get.everyinit.dev/all | sh -s -- --package all
```

The installer puts them in `~/.local/bin` by default. Ensure that directory is
on `PATH`, then restart VS Code. See the [installation guide](https://everyinit.dev/docs/book/installation)
for platform details, pinned releases, and source installation. The project is
early-stage canary software; interfaces may change before a stable release.
The canary label applies to the CLI download channel. The Marketplace extension
uses regular extension versioning and is not published as a VS Code pre-release.

## Run the extension from source

Open the repository root in VS Code and press F5 (`Launch Everyinit Extension`).
The pre-launch task builds the `evin` and `esh` debug binaries and bundles the
extension. In the VS Code Flatpak setup, it runs those commands through the
`dev` toolbox with the versions selected by mise. The Extension Development
Host opens the repository root, where the extension finds the freshly built
servers under `target/debug`.
To prepare those binaries and run the extension's checks and tests without
launching the host, run `vscode:e2e` from the Everyinit task view.
It builds Evin and ESH into the configured `target/debug` paths before running
the extension checks.

Task execution uses the Evin server started from `everyinit.evin.path`. When
the default Linux terminal profile enters a Flatpak Toolbox, the extension
runs that configured executable in its managed integrated terminal so task
steps inherit the toolbox environment. The extension test task is launched
through the configured Evin binary; its build dependencies use Cargo to place
fresh servers at the configured paths.

## Try it in a project

In a repository with Evin installed, create `.evin/workspace.flow`. Open the
repository root in VS Code; the extension detects this workspace marker and
starts Evin's workspace features:

```flow
#!evin/v1
workspace:
  name: my-project
  version: "1.0.0"

stages: [Check]

tasks:
  hello:
    stage: Check
    run: esh scripts/hello.esh
```

Then run `Everyinit: Run Task…` from the Command Palette. Choose a task and an
execution mode (normal, dry run, parallel, or sequential). The task tree's Run
button runs immediately in normal mode. The status bar follows active runs and
keeps a failed run visible until the next run; task rows show the last result.
Set `everyinit.execution.promptForMode` to `false` to skip the mode picker.
Evin reads the workspace manifest and runs the `hello` task, which invokes
this ESH script:

```esh
#!/usr/bin/env esh

echo "Hello from ESH"
```

Save the script as `scripts/hello.esh`; you can also run the task in a terminal
with `evin run hello`.

Read the [first workflow guide](https://everyinit.dev/docs/book/first-workflow)
and [first ESH script guide](https://everyinit.dev/docs/book/esh/first-script)
for a complete introduction.

## Language servers and executable paths

The extension starts `evin lsp` for `.flow` files and `esh lsp` for `.esh`
files. By default it searches `PATH`, workspace `target/debug` and
`target/release`, and `~/.cargo/bin`. Set explicit paths in VS Code settings
when the executables are elsewhere:

```json
{
  "everyinit.evin.path": "/path/to/evin",
  "everyinit.esh.path": "/path/to/esh"
}
```

You can use `${workspaceFolder}/target/debug/evin` and
`${workspaceFolder}/target/debug/esh` for binaries built in the opened
repository. The corresponding enable settings are `everyinit.evin.enable` and
`everyinit.esh.enable`.

## Troubleshooting

If the status bar shows an offline language server:

1. Check `evin --version` or `esh --version` in a terminal launched from the
   same environment as VS Code.
2. Confirm the binary's directory is on PATH, or set `everyinit.evin.path` or
   `everyinit.esh.path` to its full path.
3. Restart the language servers with `Everyinit: Restart Language Servers`.
4. Open `Everyinit: Show Language Server Output` from the Command Palette for
   the startup error.

VS Code launched from a desktop menu may not inherit the PATH changes made in
your shell profile. In that case, configure the full executable paths in the
settings above.

## Links

- [Install and compatibility](https://everyinit.dev/docs/book/installation)
- [Documentation](https://everyinit.dev/docs/)
- [Source repository](https://github.com/dukasn/everyinit)
- [Report an issue](https://github.com/dukasn/everyinit/issues)
- [License](https://github.com/dukasn/everyinit/blob/main/LICENSE)

The extension is licensed under GPL-3.0-only. The logo artwork, editor icons,
and icon font are licensed under CC BY 4.0; see the repository's
[license files](https://github.com/dukasn/everyinit/tree/main/LICENSES) and
[trademark policy](https://github.com/dukasn/everyinit/blob/main/TRADEMARKS.md).

## Contributing

See [extension development](https://github.com/dukasn/everyinit/blob/main/docs/contributing/50.vscode-extension.md)
for build, watch, and Extension Development Host instructions.
