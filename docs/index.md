---
title: Everyinit
description: Orchestrate repository tasks, Git hooks, ESH scripts, and delivery workflows with Evin.
search_prompts:
  - How do I define and run a workflow task?
  - How do I connect Evin tasks to Git hooks?
  - How do I write project automation scripts with ESH?
  - How do I package a release with Evin?
---

# Everyinit

Development, orchestrated.

Evin gives repository commands a shared home. Define tasks and dependencies
once, then run them from a terminal, a Git hook, or CI. Use ESH for project
automation and built-in actions for documentation, archives, and releases.

## Start here

- [Install Evin](/book/installation) and initialize a repository.
- [Run your first workflow](/book/first-workflow) with commands your project
  already uses.
- [Connect workflows to Git hooks](/book/connect-git-hooks) and reuse the same
  checks before commits.
- [Write ESH scripts](/book/esh) when a task needs branching, data handling, or
  cleanup.

## Describe a workflow

Put shared tasks in `.evin/workspace.flow`. Evin resolves the selected task
and its prerequisites before running any steps.

```flow
workspace:
  name: storefront
  execution: linear

stages: [Check, Test]

tasks:
  lint:
    stage: Check
    run: npm run lint

  test:
    stage: Test
    needs: [lint]
    run: npm test
```

Run the checks with one command:

```sh
evin run test
```

## Explore the documentation

- [The Evin Book](/book) teaches workflows from first task through delivery.
- [Technical Reference](/reference) documents configuration, commands, and
  built-in actions.
- [ESH language reference](/reference/esh/language-reference) describes the
  scripting language and its runtime contract.
- [Contributing guide](/contributing) explains development and documentation
  conventions.

## Project links

- [Source code](https://github.com/dukasn/everyinit)
- [Licensing](/contributing/licensing)
- [Acknowledgments](https://github.com/dukasn/everyinit/blob/main/ACKNOWLEDGMENTS.md)
- [Third-party notices](https://github.com/dukasn/everyinit/blob/main/THIRD_PARTY_NOTICES.md)
