---
title: Everyinit
description: Define repository workflows in .flow, run them locally or in CI, and keep shell automation portable with ESH.
search_prompts:
  - How do I define and run a workflow task?
  - How do I connect Evin tasks to Git hooks?
  - How do I write portable project scripts with ESH?
  - How do I package a release with Evin?
topbar:
  id: everyinit-release-updates
  variant: holo
  badge: Canary
  message: Follow Everyinit releases for the latest updates.
  action:
    label: Watch releases
    href: https://github.com/dukasn/everyinit/releases
  dismissible: true
---

# Everyinit

Orchestrated Development Workflows

> [!FEATURE]
>
> ## Define the work once.
>
> Describe tasks, stages, and dependencies in `.evin/workspace.flow`. Evin
> turns that file into a task graph you can run by task, stage, or Git hook.
>
> - [Build your first workspace](/book/first-workspace)
> - [Explore workspace configuration](/reference/workspace-configuration)
>
> ```flow
> workspace:
>   name: storefront
>   version: "1.0.0"
>   execution: linear
>
> stages: [Check, Test]
>
> tasks:
>   fmt:
>     stage: Check
>     description: "Check Rust formatting"
>     run: cargo fmt --all -- --check
>
>   test:
>     stage: Test
>     needs: [fmt]
>     run: cargo test
> ```

> [!FEATURE]
>
> ## Run a task and see each step.
>
> Run one target with `evin run <task>`. Evin resolves its configuration,
> executes the task and its dependencies, then reports the result in the same
> terminal workflow. The recording runs the `fmt` task shown above.
>
> - [Run tasks and Git hooks](/book/pipelines-and-hooks)
> - [See the CLI reference](/reference/cli-reference)
>
> ```tape title="Run the fmt task" runtime=system rows=auto cols=64 speed=45ms
> setup "cargo build --quiet --bin evin"
> alias "evin" "cargo --quiet evin-dev"
> type "evin run fmt"
> enter
> wait
> ```

> [!FEATURE]
>
> ## Connect repository events to those tasks.
>
> Bind checks to Git hooks in `.evin/workspace.flow`, and keep commit, branch,
> and file policies in `.evin/gitrules.flow`. Run `evin sync` to install or
> update the hooks; they select the same tasks you can run yourself.
>
> - [Connect tasks to Git hooks](/book/pipelines-and-hooks)
> - [Configure Git rules](/reference/git-rules-configuration)
>
> ```flow
> workspace:
>   name: storefront
>   version: "1.0.0"
>   rules: .evin/gitrules.flow
>
> stages: [Check]
>
> tasks:
>   fmt:
>     stage: Check
>     description: "Check formatting before commit"
>     on:
>       hooks: [pre-commit]
>     run: cargo fmt --all -- --check
> ```

> [!FEATURE]
>
> ## Carry a successful workflow into delivery.
>
> Use ESH for portable project scripts and built-in actions for release steps.
> Evin runs both in the task graph, so a package can depend on the same checks
> your team already runs locally and from Git hooks.
>
> - [Explore built-in actions](/reference/actions)
> - [Write portable ESH scripts](/reference/esh)
>
> ```flow
> stages: [Test, Release]
>
> tasks:
>   test:
>     stage: Test
>     run: cargo test
>
>   package:
>     stage: Release
>     needs: [test]
>     steps:
>       - run: esh scripts/build.esh
>       - uses: evin/archive@v1
>         with:
>           path: dist/storefront.tar.gz
>           include: [target/release/storefront, LICENSE]
>           prefix: storefront
>           layout: relative
> ```

## Start building workflows

Follow a short path from your first task to portable scripts and Git hooks.

- [The Evin Book](/book) - Create a workspace, compose tasks, and connect checks to Git.
- [Pipelines and Git hooks](/book/pipelines-and-hooks) - Select tasks by target and run them from repository events.
- [ESH Shell](/reference/esh) - Write project scripts that behave consistently across platforms.

> [!SPOTLIGHT] holo | FROM ONE TASK TO THE WHOLE WORKFLOW
>
> ## Start with a task. Grow into a workflow.
>
> Put a command in `.evin/workspace.flow`, then add dependencies, Git hooks,
> and release steps as your project needs them. Evin runs the same task graph
> locally and in CI.
>
> [Build your first workspace](/book/first-workspace)

## Explore configuration and delivery

Look up the contracts behind your workflow or extend it with built-in delivery actions.

- [Workspace configuration](/reference/workspace-configuration) - See the fields and validation rules for workspace manifests.
- [Git rules and security](/reference/git-rules-configuration) - Set repository policies for commits, branches, and files.
- [Built-in actions](/reference/actions) - Package archives and automate documentation and release work.

## Footer

Everyinit brings repository workflows, portable scripts, and delivery actions together.

### Project

- [Source code](https://github.com/dukasn/everyinit)
- [GPLv3 license](https://www.gnu.org/licenses/gpl-3.0.html)

### Learn

- [The Evin Book](/book)
- [ESH Shell](/reference/esh)
- [Workspace configuration](/reference/workspace-configuration)

### Contribute

- [Development workflow](/contributing/development)
- [Write documentation](/contributing/documentation)
