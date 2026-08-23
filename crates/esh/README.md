# ESH — Evin Shell

ESH is Evin's deterministic, embeddable shell for project automation. It runs one language runtime on Linux, macOS, and Windows, so an ESH script does not depend on `sh`, Bash, `cmd.exe`, PowerShell, MSYS2, or WSL semantics.

It is an implemented parser, planner, runtime, CLI, and language server. Its built-in command and keyword documentation is compiled into the runtime and used by editor hover help.

## Why ESH

ESH keeps the parts of shell scripting that work well for automation while making the surprising parts explicit:

- Scalar expansion preserves argument boundaries; there is no implicit word splitting or globbing.
- Pipelines, logical chains, blocks, functions, redirections, command substitution, loops, `case`, and traps are parsed by ESH.
- `&|` joins independent work concurrently.
- Filesystem, data, archive, cryptography, HTTP, time, and text tools run in-process.
- Failures carry command and line telemetry for diagnostics and error handling.

```esh
#!/usr/bin/env esh

set -e
TARGET="${1:-./dist}"
mkdir -p "$TARGET"

{
  cargo fmt --check &| \
  cargo clippy --all-targets -- -D warnings &| \
  cargo test
} || {
  echo "failed: $ESH_FAILED_CMD at line $ESH_FAILED_LINE" >&2
  exit 1
}
```

## CLI

```console
esh file.esh [args...]          # execute a script
esh -c 'echo hello'             # execute an inline script
esh --dry-run file.esh          # print a plan without side effects
esh help json                   # print built-in documentation
esh lsp                         # run the language server over stdio
```

## Documentation

- [ESH overview](../../docs/reference/esh/00.index.md) — an introduction and learning path.
- [Writing ESH scripts](../../docs/reference/esh/10.writing-scripts.md) — files, values, arguments, and execution.
- [Control flow and failures](../../docs/reference/esh/30.control-flow-and-failures.md) — decisions, cleanup, and diagnostics.
- [ESH Language Reference](../../docs/reference/esh/90.language-reference.3000.md) — exact language syntax and execution semantics.
- [ESH Runtime Reference](../../docs/reference/esh/100.runtime-reference.3010.md) — built-in commands and keywords; the source for LSP documentation.

## Development

Run the ESH test suite with:

```console
cargo test -p esh
```

The runtime catalog is generated from `docs/reference/esh/100.runtime-reference.3010.md`. Run `cargo test -p evin-action-docs` after changing a catalog entry so `crates/esh/assets/builtins-catalog.json` stays current.

ESH is licensed under GNU GPLv3, like the rest of this workspace.
