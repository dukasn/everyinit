# CI and platform support

Evin's current tested and supported target is Linux x86_64. This is a deliberate
scope choice for an early project maintained by one developer: it keeps the
initial architecture and feature work focused while the core workflows mature.
It is not a claim that other platforms are unimportant or should never be
supported.

## Current policy

- Linux x86_64 is the supported target for contributor workflows, CI, and
  official Evin and ESH release artifacts.
- Linux x86_64 behavior is the primary compatibility target. Local setup and
  validation should be documented and kept reliable on that target.
- GitHub Actions runs on Linux x86_64 only for now, including end-to-end and
  process-supervision checks. Other Linux architectures, macOS, and Windows
  are not part of the current CI runner set or official CLI builds.
- The canonical task definitions live in Evin `.flow` files. CI should invoke
  those tasks through `cargo evin-dev` and keep workflow YAML focused on runner
  setup, credentials, and GitHub-specific orchestration.

## Branch checks and release rehearsal

- Pull requests and pushes targeting `main` or `release`, and manual CI
  dispatches, run workspace tests, binary E2E, quality checks, and
  `release:rehearse`.
- CI builds and verifies every release format—Evin and ESH tarballs, Debian
  packages, Arch packages, and the Everyinit VS Code extension package (`.vsix`).
  All packages are cryptographically signed with SHA-256 checksum manifests and
  Ed25519 signatures (`SHA256SUMS.sig`), then uploaded as workflow artifacts
  for immediate testing. Native package verification accepts a single positive
  revision for each package and format, and rejects ambiguous matches.
  Staging clears the shared archive directory before any release package is
  written, including the VS Code extension.
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
  exact-SHA main push CI succeeds. Evin builds, verifies, and cryptographically
  signs all package formats (tarballs, `.deb`, Arch packages, and `.vsix`),
  then the workflow tags the merged commit and publishes them to GitHub Releases.
  Ordinary pushes and tag pushes never publish. See the
  [release procedure](/contributing/development#prepare-a-github-release).

## Verifying release signatures

Release checksum manifests are signed with an Ed25519 key pinned at
`infra/get/release-key.pub`. To verify artifact authenticity and integrity:

```bash
# 1. Verify the manifest signature:
openssl pkeyutl -verify -pubin -inkey release-key.pub -rawin -in SHA256SUMS -sigfile SHA256SUMS.sig

# 2. Verify artifact checksums:
sha256sum -c SHA256SUMS
```

## Expanding support

Treat broader platform support as a later, explicit project decision. Before
making other Linux architectures, macOS, or Windows supported, define the
user-facing support promise and release expectations, then add native CI
coverage for the relevant build, test, and quality workflows. Until then,
prioritize Linux x86_64 feature development and defer platform-specific work
unless it is required by the architecture.
