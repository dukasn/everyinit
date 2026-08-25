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
> ## Start with the workspace
>
> `.evin/workspace.yaml` gives repository automation one declared home. Name
> the workflow, choose its default execution policy, and load module-local
> tasks without scattering scripts across the repository.
>
> - [Build your first workspace](/book/first-workspace)
> - [Explore the workspace schema](/reference/workspace-configuration)
>
> ```yaml
> workspace:
>   # Give this repository workflow a stable identity.
>   name: storefront
>   version: "0.1.0"
>
>   # Choose how ready tasks run unless a command overrides it.
>   execution: linear
>
>   # Load task definitions that live with repository components.
>   modules: [apps/web, crates/api]
> ```

## Learn EVIN

Learn how to organize repository work, automate Git lifecycle checks, and keep
delivery routines close to the code they support.

- [Workspaces & Environments](/book/workspaces) - Organize tasks, dependencies, environment matrices, and values into clear definitions.
- [Pipelines & Git Hooks](/book/pipelines-and-hooks) - Trigger stage graphs, single tasks, or native Git hooks with automatic dependency resolution.
- [Documentation Site](/book/documentation-site) - Add documentation publishing to the same declared workflow.

> [!FEATURE]
>
> ## Transform project data with ESH
>
> ESH keeps routine automation readable: query structured release data, pipe a
> numeric value into a formatter, then branch on that command's result without
> leaving the prompt. The same portable commands work inside an Evin task or as
> a directly invoked ESH script.
>
> - [Write an ESH script](/reference/esh/writing-scripts)
> - [Browse ESH built-ins](/reference/esh/runtime-reference)
>
> ```tape title="Pipe and branch in one command" rows=6 cols=76
> type "json -g bytes '{\"bytes\": 15728640}' | numfmt --to=iec && { echo \"artifact is ready\"; } || { echo \"format failed\"; }"
> enter
> wait
> ```

> [!FEATURE]
>
> ## Built-in actions for delivery work
>
> Keep recurring delivery work in the same task graph. This task packages the
> selected release files into one archive; the same action layer also handles
> documentation, changelogs, version updates, and release commits.
>
> - [Browse built-in actions](/reference/actions)
> - [Archive action details](/reference/actions/archive-action)
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

## Technical reference and ecosystem

Explore the contracts behind repository workflows, ESH, configuration, and
diagnostics, then help improve the project in the open.

- [ESH Language Reference](/reference/esh/language-reference) - POSIX syntax, parameter expansions, arithmetic operations, and parallel pipelines.
- [Configuration & Diagnostic RFCs](/reference) - Complete normative reference for schema parameters, diagnostics, and modules.
- [Contribute to EVIN](/contributing) - Set up the development toolchain, inspect the core engine, and shape upcoming features.

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
