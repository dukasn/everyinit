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

## Branch checks and release rehearsal

- Pull requests and pushes targeting `main` or `release`, and manual CI
  dispatches, run workspace tests, binary E2E, quality checks, and
  `release:distribution-rehearsal`. The rehearsal generates the
  candidate changelog under ignored `target/` output and builds and inspects
  the Evin and ESH tar.gz, DEB, APK, and Arch packages. It does not publish or
  access release-signing secrets.
- Scheduled CI runs only the Linux process-supervision soak suite. A manual
  dispatch can include it alongside full validation with `run_soak`.
- The existing `Workspace tests`, `Binary E2E (Linux)`, `Formatting, Clippy,
  and browser checks`, and `Distribution rehearsal (Evin and ESH)` check names
  report the shared validation result.
- Configure the `CI gate` job as the required status check on both protected
  branches. Keep direct pushes, force pushes, and deletions blocked by the
  branch rulesets.
- After successful main push CI, the Release workflow maintains a single
  release PR. Merging it (merge commit, squash, or rebase) starts a release only after
  exact-SHA main push CI succeeds. Evin builds and verifies the Linux musl
  archives, then the workflow tags the merged commit and publishes them.
  Ordinary pushes and tag pushes never publish. See the
  [release procedure](/contributing/development#prepare-a-github-release).

## Expanding support

Treat broader platform support as a later, explicit project decision. Before
making macOS or Windows supported, define the user-facing support promise and
release expectations, then add native CI coverage for the relevant build,
test, and quality workflows. Until then, prioritize Linux feature development
and defer platform-specific work unless it is required by the architecture.
