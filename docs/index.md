---
title: EVIN
description: EVIN - The Workflow Engine for software repositories.
search_prompts:
  - How do I build a cross-platform workflow?
  - Enforce Git commit rules and block secret leaks
  - Eliminate CI shell drift with ESH
  - Contribute to EVIN or inspect the architecture
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
> ## Declare Once. Orchestrate Everything.
>
> Eliminate sprawling Makefiles, detached npm scripts, and brittle CI
> manifests. `.evin/workspace.flow` brings your entire repository lifecycle
> into one deterministic task graph: modular stages, explicit dependencies,
> and seamless local-to-CI parity.
>
> - [Build your first workspace](/book/first-workspace)
> - [Workspace schema and modules](/reference/workspace-configuration)
>
> ```flow
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
> ## Native Git Governance. Zero Wrappers.
>
> No fragile hook wrappers. No external linters. EVIN evaluates commit
> conventions, branch policies, cryptographic authorship, and secret detection
> directly in memory against live Git state. Run `evin sync` once, and your
> repository governance stays synchronized and unbreakable.
>
> - [Explore Git pipelines & hooks](/book/pipelines-and-hooks)
> - [Configure Git rules](/reference/git-rules-configuration)
>
> ```mermaid "In-memory Git Governance"
> flowchart LR
>     Hook[git commit] --> Inspect[In-Memory Git State]
>     Inspect --> Rules{Git Rules}
>     Rules -->|Verified| Tasks[Execute Tasks]
>     Rules -->|Violation| Diagnostic[Instant Diagnostic]
> ```

> [!FEATURE]
>
> ## Deterministic Scripting. Zero Platform Drift.
>
> Never debug "works on my machine" shell divergence again. Evin Shell (ESH)
> delivers an identical runtime across macOS, Linux, and Windows—handling
> structured JSON, YAML parsing, arithmetic, and concurrency natively without
> dependencies on Bash, WSL, or PowerShell.
>
> - [Write portable ESH scripts](/reference/esh/writing-scripts)
> - [ESH runtime reference](/reference/esh/runtime-reference)
>
> ```tape title="Deterministic scripting with ESH" rows=5 cols=68 speed=60ms
> type "echo '{\"status\": \"ok\", \"artifacts\": 3}' | json -g status && echo \"pipeline ready\""
> enter
> wait
> ```

> [!FEATURE]
>
> ## From Commit to Release. Compiled Right In.
>
> Cut third-party CI plugins, marketplace security vulnerabilities, and network
> flakiness out of your release pipeline. Automated changelogs, semantic version
> bumping, and multi-format release archives are compiled directly into EVIN and
> execute within the same verified task graph.
>
> - [Explore built-in actions](/reference/actions)
> - [Archive & release guide](/reference/actions/archive-action)
>
> ```flow
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

## Guides & Workflows

Practical, hands-on journeys to accelerate your development loop—from creating your initial workspace to automating Git hooks and writing portable shell scripts.

- [The EVIN Book](/book) - Step-by-step guidance to design, test, and run repository workflows.
- [Pipelines & Git Hooks](/book/pipelines-and-hooks) - Connect your task graph to Git lifecycle events and automated checks.
- [Portable ESH Shell](/reference/esh) - Write deterministic cross-platform scripts with built-in JSON and pipes.

> [!SPOTLIGHT] holo | THE SINGLE-BINARY STANDARD
>
> ## Declare once. Run everywhere.
>
> One workflow definition powering your local terminal, your Git hooks, and your production CI. Zero external runtimes, zero platform drift, and complete supply chain certainty.
>
> [Start with The EVIN Book](/book)

## Governance & Architecture

Technical specifications, in-memory Git policy contracts, compiled delivery capabilities, and native contribution paths for teams analyzing safety and scale.

- [Git Rules & Security](/reference/git-rules-configuration) - Enforce conventional commits, branch hygiene, and in-memory secret detection.
- [Built-in Delivery Actions](/reference/actions) - Compiled-in semantic versioning, changelog generation, and archive distribution.
- [Contribute to EVIN](/contributing) - Inspect the modular Rust workspace, Language Server, and contribution workflows.

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
