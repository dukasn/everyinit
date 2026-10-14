# CI and platform support

Evin is developed and supported locally on Linux for now. This is a deliberate
scope choice for an early project maintained by one developer: it keeps the
initial architecture and feature work focused while the core workflows mature.
It is not a claim that other platforms are unimportant or should never be
supported.

## Current policy

- Linux is the supported platform for local development, contributor workflows,
  and release artifacts.
- Linux behavior is the primary compatibility target. Local setup and
  validation should be documented and kept reliable on Linux.
- GitHub Actions runs on Linux only for now, including end-to-end and
  process-supervision checks. macOS and Windows are not part of the current CI
  runner set.
- The canonical task definitions live in Evin `.flow` files. CI should invoke
  those tasks through `cargo evin-dev` and keep workflow YAML focused on runner
  setup, credentials, and GitHub-specific orchestration.

## Expanding support

Treat broader platform support as a later, explicit project decision. Before
making macOS or Windows supported, define the user-facing support promise and
release expectations, then add native CI coverage for the relevant build,
test, and quality workflows. Until then, prioritize Linux feature development
and defer platform-specific work unless it is required by the architecture.
