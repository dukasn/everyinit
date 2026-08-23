---
title: EVIN
description: Define once. Run everywhere.
search_prompts:
  - What would you like to build with EVIN?
  - Need a hand getting your workflow running?
  - Find a guide, command, action, or diagnostic
topbar:
  id: first-public-canary
  variant: holo
  badge: Canary
  message: EVIN’s first public canary release is coming soon.
  action:
    label: Watch releases
    href: https://github.com/dukasn/evin/releases
  dismissible: true
---

# EVIN

The orchestration workspace and portable runtime for modern development workflows.

> [!FEATURE]
>
> ## Deterministic task orchestration
>
> Define stages, dependencies, tasks, and environments in a single declarative file. Run reproducible pipelines locally and in CI with zero drift.
>
> - [Get started in 5 minutes](/book/installation)
> - [Build your first workspace](/book/first-workspace)
>
> ```yaml
> workspace:
>   name: my-app
>   version: "1.0.0"
>
> stages:
>   - Check
>   - Build
>
> tasks:
>   lint:
>     stage: Check
>     description: "Run strict linter"
>     steps:
>       - run: cargo clippy -- -D warnings
>
>   test:
>     stage: Check
>     description: "Run automated test suites"
>     steps:
>       - run: cargo test
>
>   bundle:
>     stage: Build
>     description: "Compile release binary"
>     needs: [lint, test]
>     steps:
>       - run: cargo build --release
> ```

## The EVIN Book

Master workspace architecture, task dependencies, Git hooks, and self-auditing documentation with chapter-by-chapter guides.

- [Workspaces & Environments](/book/workspaces) — Organize tasks, dependencies, environment matrices, and values into clear definitions.
- [Pipelines & Git Hooks](/book/pipelines-and-hooks) — Trigger stage graphs, single tasks, or native Git hooks with automatic dependency resolution.
- [Documentation Site](/book/documentation-site) — Generate documentation, create archives, bump semantic versions, and publish changelogs without third-party tools.

> [!FEATURE]
>
> ## A cross-platform shell runtime
>
> Write automation once and run it reliably across Linux, macOS, and Windows. ESH bundles JSON querying, YAML parsing, hashing, and units without external dependencies.
>
> - [Explore the ESH language](/reference/esh)
> - [Runtime built-in reference](/reference/esh/runtime-reference)
>
> ```tape title="Structured JSON and data units" rows=6 cols=56
> type "json -g release '{\"release\": \"v2.0.0\", \"size_bytes\": 15728640}'"
> enter
> type "numfmt --to=iec 15728640"
> enter
> wait
> ```

> [!FEATURE]
>
> ## Self-auditing documentation that ships with code
>
> Turn Markdown sources into ultra-fast static documentation with binary search indexes, automated link verification, and pre-compiled LSP catalogs.
>
> - [Documentation guide](/book/documentation-site)
> - [Action reference](/reference/actions/documentation-action)
>
> ```tape title="Semver arithmetic and path resolution" rows=6 cols=56
> type "semver -b minor 1.4.2"
> enter
> type "path -j dist releases v1.5.0 evin"
> enter
> wait
> ```

## Technical Reference & Ecosystem

Explore normative RFC specifications, language grammar, and contribute to the open-source platform.

- [ESH Language Reference](/reference/esh/language-reference) — POSIX syntax, parameter expansions, arithmetic operations, and parallel pipelines.
- [Configuration & Diagnostic RFCs](/reference) — Complete normative reference for schema parameters, diagnostics, and modules.
- [Contribute to EVIN](/contributing) — Set up the development toolchain, inspect the core engine, and shape upcoming features.

> [!SPOTLIGHT] holo
>
> ## Ready to unify your workflow?
>
> Bring deterministic pipelines, portable scripting, and self-auditing docs into your repository today.
>
> [Read The EVIN Book](/book)

## Footer

Open-source orchestration for the work around your code.

### Project

- [Source](https://github.com/dukasn/evin)
- [GPLv3 license](https://www.gnu.org/licenses/gpl-3.0.html)

### Learn

- [The EVIN Book](/book)
- [ESH Shell](/reference/esh)
- [Built-ins](/reference/actions)
- [Technical Reference](/reference)

### Contribute

- [Development workflow](/contributing/development)
- [Write documentation](/contributing/documentation)
