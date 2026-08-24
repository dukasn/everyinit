# Evin

**Evin — The Workflow Engine for software repositories.**

Define tasks, dependencies, environments, and Git hooks in `.evin/`; run the
same workflow reliably on developer machines and in CI.

## Why Evin

The work around a codebase should be as clear and repeatable as the code itself.
Evin gives that work one declared path:

- **Repository workflows** — organize checks, builds, tests, and delivery work as
  tasks with explicit dependencies.
- **Git lifecycle automation** — connect declared tasks to native Git hooks and
  keep their generated hook shims in sync.
- **Portable automation** — execute task commands through ESH, Evin's portable
  runtime for project automation.
- **Built-in delivery actions** — keep documentation, versions, changelogs,
  archives, and releases in the same workflow as everyday checks.

## Quick start

Install Evin from this checkout with Cargo:

```sh
git clone https://github.com/dukasn/evin.git
cd evin
cargo install --path crates/evin
```

Initialize a repository, then run one of its declared targets:

```sh
evin init --all
evin run <target>
```

For example, `.evin/workspace.yaml` can describe a small validation workflow:

```yaml
workspace:
  name: my-app
  version: "1.0.0"

stages:
  - Check
  - Test

tasks:
  lint:
    stage: Check
    steps:
      - run: cargo clippy -- -D warnings

  test:
    stage: Test
    needs: [lint]
    on:
      hooks: [pre-commit]
    steps:
      - run: cargo test
```

Run `evin sync` to install or update the Git hooks declared by the workspace.

## What Evin includes

`evin` is the repository workflow CLI: it initializes `.evin/` configuration,
synchronizes Git hooks, and resolves task, stage, and hook targets.

[ESH](crates/esh/README.md) is Evin's portable automation runtime. It is also a
standalone-capable shell for writing explicit project scripts across Linux,
macOS, and Windows.

Built-in actions provide common delivery capabilities without splitting them
from the task graph: documentation sites, version bumps, changelogs, archives,
and Git releases.

## Learn more

- [The Evin Book](docs/book/00.index.md) — learn to define and run repository workflows.
- [ESH documentation](docs/reference/esh/00.index.md) — write portable automation scripts.
- [Built-in actions](docs/reference/actions/00.index.md) — bring delivery work into a workflow.
- [Technical reference](docs/reference/00.index.md) — configuration, runtime, and CLI contracts.
- [Contributing](docs/contributing/00.index.md) — work on Evin and its documentation.

## Status

Evin is in the `v1.0.0-canary.x` phase. Interfaces and behavior may change as
the public release matures.

Evin is free software under the [GNU GPLv3](LICENSE).
