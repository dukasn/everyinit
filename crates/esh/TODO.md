# `esh` (Evin Shell) - Roadmap & Next Steps

This document outlines the roadmap and future development milestones for the `esh` language runtime, organized by priority.

---

## 🎯 Milestone 1: Stream Redirections & Cancellation Architecture

- [x] **File Descriptor Redirections**
  - [x] Implement file routing for `>`, `>>`, `<`, `2>`, `2>>`, `&>`, `&>>` to actual filesystem paths.
  - [x] Support pipeline stream piping and I/O capturing.
  - [x] Support `/dev/null` and `NUL` cross-platform virtualization.
- [x] **Cancellation & Signal Telemetry**
  - [x] Integrate `tokio_util::sync::CancellationToken` into `ExecutionContext`.
  - [x] Propagate graceful cancellation across `JoinSet` parallel branches (`&|`) and child external processes.
  - [x] Support SIGINT / Ctrl+C handling returning exit code `130`.

---

## 🛠️ Milestone 2: Lexical Traps & Extended Control Flow

- [x] **Lexical Trap Handlers**
  - [x] Implement `trap <handler> <SIGNAL>` and clear trap support.
  - [x] Scoped trap registry (`ScopeFrame`) with auto-drop and restoration on subshell exit.
  - [x] Trap recursion prevention (`is_in_trap`).
  - [x] Telemetry injection: `$ESH_FAILED_CMD`, `$ESH_FAILED_LINE`, and `$ESH_FAILED_MSG`.
- [x] **Positional & Parameter Built-ins**
  - [x] Implement `shift [n]` built-in for positional argument rotation.
  - [x] Implement `set [--] args...` for positional argument reconfiguration.
  - [x] Implement `export VAR=val` and `export VAR` for environment promotion.
  - [x] Support special variables `$#` (count), `$*` / `$@` (all arguments), `$?` (exit code), `$0`..`$9`.
- [x] **Parameter Expansion Modifiers**
  - [x] `${VAR:-default}`, `${VAR:=default}`, `${VAR:?error}`, `${VAR:+alt}`, `${#VAR}`, `${#@}`.

---

## 📦 Milestone 3: Extended Portable Utilities

Implement virtual, in-process, cross-platform utilities specified in `LANGUAGE_SPECIFICATION.md`:

- [x] **`mktemp`**
  - [x] Cross-platform temporary directory/file generator.
  - [x] RAII auto-cleanup on scope exit unless explicitly detached.
- [x] **`path`**
  - [x] Subcommands: `path dirname`, `path basename`, `path ext`, `path join`, `path abs`, `path rel`, `path exists`, `path is_dir`, `path is_file`.
- [x] **`str`**
  - [x] Subcommands: `str replace`, `str trim`, `str split`, `str lower`, `str upper`, `str contains`, `str starts_with`, `str ends_with`, `str len`.
- [x] **`json` / `yaml`**
  - [x] Querying and extracting structured data values without external tools (`jq`) via dot-path indexing and YAML-to-JSON conversion.
- [x] **`http`**
  - [x] In-process async HTTP utility (`http get`, `http post`).
- [x] **`prompt`**
  - [x] Interactive prompts: confirm (`y/n`), select list, input string with default fallback.

---

## 🚀 Milestone 4: Evin Ecosystem Integration & CLI Binary

- [x] **`evin-pipeline` Shell Runner Integration**
  - [x] Replace `sh -c` / `cmd.exe /c` fallback in `crates/evin-pipeline` with `esh::Engine`.
  - [x] Wire line-buffered multiplexing and secret redaction into `evin-action-sdk` action logs.
  - [x] Support `--dry-run` to output lowered `ExecutionPlan` without filesystem mutation.
- [x] **Standalone `esh` CLI**
  - [x] Command line options: `esh -c '<script>'`, `esh <file.esh> [args...]`, `esh --dry-run`.
  - [x] SIGINT / Ctrl+C handler wiring into `CancellationToken`.

---

## 🔬 Milestone 5: Performance Profiling & Criterion Benchmarks

- [x] **Criterion Benchmark Suite (`crates/esh/benches/`)**
  - [x] `bench_lexer`: Zero-copy scanning and tokenization throughput using 256-byte `ASCII_LUT`.
  - [x] `bench_parser`: Pratt precedence parser and `bumpalo::Bump` contiguous linear allocation throughput.
  - [x] `bench_runtime`: In-process virtual built-in execution (`mkdir`, `path`, `json`, `str`) vs OS process forks.
  - [x] `bench_cache`: `PlanCache` XXHash3 hash key lookup validation.
