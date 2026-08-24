---
title: EVIN
description: Evin — The Workflow Engine for software repositories.
search_prompts:
  - What would you like to build with EVIN?
  - Need a hand getting your workflow running?
  - Find a guide, command, action, or diagnostic
topbar:
  id: first-public-canary
  variant: holo
  badge: Canary
  message: Evin’s first public canary release is coming soon.
  action:
    label: Watch releases
    href: https://github.com/dukasn/evin/releases
  dismissible: true
---

# EVIN

The Workflow Engine

> [!FEATURE]
>
> ## Repository workflows, defined once
>
> Define tasks, stages, dependencies, and environments in repository
> configuration. Evin resolves the work needed for a target and makes its path
> visible whether it runs locally, from a Git hook, or in CI.
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

## Learn Evin

Learn how to organize repository work, automate Git lifecycle checks, and keep
delivery routines close to the code they support.

- [Workspaces & Environments](/book/workspaces) — Organize tasks, dependencies, environment matrices, and values into clear definitions.
- [Pipelines & Git Hooks](/book/pipelines-and-hooks) — Trigger stage graphs, single tasks, or native Git hooks with automatic dependency resolution.
- [Documentation Site](/book/documentation-site) — Add documentation publishing to the same declared workflow.

> [!FEATURE]
>
> ## Portable automation through ESH
>
> ESH is Evin's portable, deterministic runtime for project automation. Use it
> inside Evin workflows or directly in scripts across Linux, macOS, and Windows.
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
> ## Built-in actions for delivery work
>
> Keep recurring delivery work in the same task graph. Built-in actions cover
> documentation, versions, changelogs, archives, and Git releases.
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

## Technical reference and ecosystem

Explore the contracts behind repository workflows, ESH, configuration, and
diagnostics, then help improve the project in the open.

- [ESH Language Reference](/reference/esh/language-reference) — POSIX syntax, parameter expansions, arithmetic operations, and parallel pipelines.
- [Configuration & Diagnostic RFCs](/reference) — Complete normative reference for schema parameters, diagnostics, and modules.
- [Contribute to EVIN](/contributing) — Set up the development toolchain, inspect the core engine, and shape upcoming features.

> [!SPOTLIGHT] holo
>
> ## Make repository work repeatable
>
> Define the work around your code once, then run it through one visible,
> portable workflow.
>
> [Read The EVIN Book](/book)

## Footer

Open-source workflow automation for software repositories.

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
