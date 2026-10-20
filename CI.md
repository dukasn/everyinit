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

- Pull requests targeting `release` run workspace tests, binary E2E, quality
  checks, and `release:distribution-rehearsal`. The rehearsal generates the
  candidate changelog under ignored `target/` output and builds and inspects
  the Evin and ESH tar.gz, DEB, APK, and Arch packages. It does not publish or
  access release-signing secrets.
- Pull requests targeting `main` from the repository's `release` branch use a
  lightweight promotion check that confirms the PR is the current `release`
  tip and that `release` contains `main`. Other `main` PRs run the complete
  validation and distribution rehearsal.
- Pushes to `main` and `release` that correspond to merged PRs do not rerun
  those PR checks. A push not associated with a merged PR runs the full
  validation and rehearsal as a bypass safeguard.
- Configure the `CI gate` job as the required status check on both protected
  branches. Keep direct pushes, force pushes, and deletions blocked by the
  branch rulesets.
- Tagged release builds are verification-only. Publication is a separate
  manual dispatch; it downloads the artifacts from the successful build job.
  Only the protected `release` environment's publish job receives the release
  signing key and `contents: write` permission. Merging `release` into `main`
  does not publish a release. Follow the [canary release procedure](/contributing/development#prepare-a-canary-release)
  to push a version tag, inspect its build, and dispatch publication for that
  same tag.

## Expanding support

Treat broader platform support as a later, explicit project decision. Before
making macOS or Windows supported, define the user-facing support promise and
release expectations, then add native CI coverage for the relevant build,
test, and quality workflows. Until then, prioritize Linux feature development
and defer platform-specific work unless it is required by the architecture.
