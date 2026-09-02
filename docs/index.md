---
title: EVIN
description: EVIN - The Workflow Engine for software repositories.
search_prompts:
  - What would you like to build with EVIN?
  - Need a hand getting your workflow running?
  - Find a guide, command, action, or diagnostic
topbar:
  id: first-public-canary
  variant: holo
  badge: Canary
  message: EVIN's first public canary release is coming soon.
  action:
    label: Watch releases
    href: https://github.com/dukasn/evin/releases
  dismissible: true
---

# EVIN

The Workflow Engine

> [!FEATURE]
>
> ## Declare Workflows Once
>
> Stop scattering shell scripts, CI configs, and task runners across your
> repository. `.evin/workspace.yaml` is the single declarative home for your
> project's lifecycle: tasks, stages, dependencies, environments, and modular
> components.
>
> - [Build your first workspace](/book/first-workspace)
> - [Workspace schema and modules](/reference/workspace-configuration)
>
> ```yaml
> workspace:
>   name: storefront
>   version: "0.1.0"
>   execution: linear
>   modules: [apps/web, crates/api]
>
> stages:
>   - Check
>   - Test
>   - Release
>
> tasks:
>   lint:
>     stage: Check
>     steps:
>       - run: cargo clippy -- -D warnings
>
>   test:
>     stage: Test
>     needs: [lint]
>     steps:
>       - run: cargo test
> ```

> [!FEATURE]
>
> ## Git, Built In
>
> EVIN has exactly one external expectation: Git. It extends what Git
> already provides rather than wrapping it behind adapters. Hook shims are
> generated and kept in sync by `evin sync`. Repository policies covering commit
> message formats, branch naming, staged-file rules, authorship, and secret
> detection are evaluated in-memory against the live Git state before tasks
> run.
>
> - [Pipelines & Git Hooks](/book/pipelines-and-hooks)
> - [Git Rules configuration](/reference/git-rules-configuration)
>
> ```yaml
> tasks:
>   check:
>     stage: Check
>     on:
>       hooks: [pre-commit]
>     steps:
>       - run: cargo clippy -- -D warnings
> ```

> [!FEATURE]
>
> ## Portable Scripting with ESH
>
> Evin Shell (ESH) eliminates platform-specific shell drift across Linux,
> macOS, and Windows. One deterministic runtime handles pipelines, structured
> JSON/YAML, arithmetic, redirections, and concurrent work without depending
> on `sh`, Bash, `cmd.exe`, or WSL.
>
> - [Write portable ESH scripts](/reference/esh/writing-scripts)
> - [ESH built-ins and operators](/reference/esh/runtime-reference)
>
> ```tape title="Pipe and branch in one command" rows=6 cols=76
> type "json -g bytes '{\"bytes\": 15728640}' | numfmt --to=iec && { echo \"artifact is ready\"; } || { echo \"format failed\"; }"
> enter
> wait
> ```

> [!FEATURE]
>
> ## Zero-Dependency Delivery
>
> No CI plugins. No external services. Standard delivery tasks including
> semantic versioning, changelog generation, release archiving, and Git
> releases are compiled directly into the EVIN binary and run inside the same
> task graph as everyday checks.
>
> - [Built-in actions](/reference/actions)
> - [Archive action](/reference/actions/archive-action)
>
> ```yaml
> tasks:
>   package-release:
>     stage: Release
>     needs: [build]
>     steps:
>       - uses: evin/archive@v1
>         with:
>           path: dist/evin.tar.gz
>           include: [target/release/evin, LICENSE]
>           prefix: evin
>           layout: relative
> ```

## Reference & Guides

The full technical story covering workspace contracts, ESH language
semantics, built-in action schemas, diagnostic references, and the
contribution path.

- [The EVIN Book](/book) - Learn to define and run repository workflows.
- [Pipelines & Git Hooks](/book/pipelines-and-hooks) - Connect the task graph to Git and CI.
- [ESH Language Reference](/reference/esh/language-reference) - Syntax, expansion, arithmetic, pipes, and built-ins.
- [Built-in Actions](/reference/actions) - Delivery capabilities compiled into the binary.
- [Configuration & Schemas](/reference) - Workspace manifests, environment contracts, and diagnostics.
- [Contribute to EVIN](/contributing) - Set up the native toolchain and shape upcoming features.

> [!SPOTLIGHT] holo
>
> ## Declare once. Run everywhere.
>
> One workflow definition. Your laptop, your Git hooks, your CI.
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
