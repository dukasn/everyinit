---
title: Everyinit
description: Orchestrate repository tasks, Git hooks, ESH scripts, and delivery workflows with Evin.
search_prompts:
  - How do I define and run a workflow task?
  - How do I connect Evin tasks to Git hooks?
  - How do I write project automation scripts with ESH?
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

Development, orchestrated.

> [!FEATURE]
>
> ## Define the work once.
>
> Describe tasks, stages, and dependencies in `.evin/workspace.flow`. Evin
> turns that file into a task graph you can run by task, stage, or Git hook.
>
> - [Run your first workflow](/book/first-workflow)
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
> - [Connect workflows to Git hooks](/book/connect-git-hooks)
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
> - [Connect workflows to Git hooks](/book/connect-git-hooks)
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
> Use ESH for project scripts and built-in actions for release steps.
> Evin runs both in the task graph, so a package can depend on the same checks
> your team already runs locally and from Git hooks.
>
> - [Explore built-in actions](/reference/actions)
> - [Learn ESH](/book/esh)
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

Follow a short path from your first task to ESH scripts and Git hooks.

- [The Evin Book](/book) - Turn repository commands into workflows, then connect them to Git and CI.
- [Connect workflows to Git hooks](/book/connect-git-hooks) - Run named tasks from repository events.
- [Learn ESH](/book/esh) - Write scripts with ESH-defined syntax and built-ins.

> [!SPOTLIGHT] holo | ZERO ENVIRONMENT DRIFT
>
> ## One workflow definition. Zero environment drift.
>
> Run the exact same task graph in your local terminal, through Git hooks, and
> across CI pipelines. Evin ensures every check, test, and delivery step
> executes with predictable outcomes everywhere.
>
> [Start with The Evin Book](/book)

## Explore configuration and delivery

Look up the contracts behind your workflow or extend it with built-in delivery actions.

- [Workspace configuration](/reference/workspace-configuration) - See the fields and validation rules for workspace manifests.
- [Git rules and security](/reference/git-rules-configuration) - Set repository policies for commits, branches, and files.
- [Built-in actions](/reference/actions) - Package archives and automate documentation and release work.

## Footer

Free software, built with Rust. Made possible by open-source communities.

### Project

- [Source code](https://github.com/dukasn/everyinit)
- [GPLv3 and documentation licenses](/contributing/licensing)
- [Everyinit name and logo guidelines](https://github.com/dukasn/everyinit/blob/main/TRADEMARKS.md)
- [Acknowledgments](https://github.com/dukasn/everyinit/blob/main/ACKNOWLEDGMENTS.md)
- [Third-party notices](https://github.com/dukasn/everyinit/blob/main/THIRD_PARTY_NOTICES.md)

### Learn

- [The Evin Book](/book)
- [ESH Reference](/reference/esh)
- [Workspace configuration](/reference/workspace-configuration)

### Contribute

- [Development workflow](/contributing/development)
- [Write documentation](/contributing/documentation)
