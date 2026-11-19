# Release preview comment examples

These drafts show possible comments on the automated release pull request. The
run IDs, versions, counts, and commit hashes are illustrative placeholders; they
do not describe current CI or audit results. GitHub renders the alert callouts
shown below. Keep the comment current for the audited commit and link details to
the relevant workflow runs.

## Ready to release

<!-- everyinit-site-preview -->
### 🚀 Release Preview · `vX.Y.Z` (`minor`)

**[Open live preview ↗](https://pr-42.everyinit-site.pages.dev)** · [Compare changes](https://github.com/dukasn/everyinit/compare/vX.Y.Z-1...a1b2c3d) · Commit [`a1b2c3d`](https://github.com/dukasn/everyinit/commit/a1b2c3d)

> [!TIP]
> **Ready for release.** Required checks passed; the dependency audit found no known vulnerabilities or unmaintained crates.

| Check | Status | Details |
| :--- | :--- | :--- |
| **Required CI** | ✅ Passed | [Workflow run](https://github.com/dukasn/everyinit/actions/runs/123456789) |
| **Vulnerabilities** | ✅ 0 found | [Audit report](https://github.com/dukasn/everyinit/actions/runs/123456789) |
| **Maintenance** | ✅ 0 notices | [Audit report](https://github.com/dukasn/everyinit/actions/runs/123456789) |

<sub>Last checked `YYYY-MM-DD HH:MM UTC` on commit `a1b2c3d`</sub>

## Informational maintenance notices

<!-- everyinit-site-preview -->
### 🚀 Release Preview · `vX.Y.Z` (`minor`)

**[Open live preview ↗](https://pr-42.everyinit-site.pages.dev)** · [Compare changes](https://github.com/dukasn/everyinit/compare/vX.Y.Z-1...a1b2c3d) · Commit [`a1b2c3d`](https://github.com/dukasn/everyinit/commit/a1b2c3d)

> [!WARNING]
> **Review informational notices.** The audit found 4 unmaintained crates and no known vulnerabilities. See the [audit report](https://github.com/dukasn/everyinit/actions/runs/123456789) for affected crates and dependency paths.

| Check | Status | Details |
| :--- | :--- | :--- |
| **Required CI** | ✅ Passed | [Workflow run](https://github.com/dukasn/everyinit/actions/runs/123456789) |
| **Vulnerabilities** | ✅ 0 found | [Audit report](https://github.com/dukasn/everyinit/actions/runs/123456789) |
| **Maintenance** | ⚠️ 4 unmaintained notices | Informational · [Audit report](https://github.com/dukasn/everyinit/actions/runs/123456789) |
| **Windows probe** | ✅ Passed | Informational · [Workflow run](https://github.com/dukasn/everyinit/actions/runs/123456790) |

<sub>Last checked `YYYY-MM-DD HH:MM UTC` on commit `a1b2c3d`</sub>

## Release blocked by a vulnerability

<!-- everyinit-site-preview -->
### 🚀 Release Preview · `vX.Y.Z` (`patch`)

**[Open live preview ↗](https://pr-42.everyinit-site.pages.dev)** · [Compare changes](https://github.com/dukasn/everyinit/compare/vX.Y.Z-1...a1b2c3d) · Commit [`a1b2c3d`](https://github.com/dukasn/everyinit/commit/a1b2c3d)

> [!CAUTION]
> **Release blocked.** The audit found 1 known vulnerability and 4 unmaintained crates. Review the [audit report](https://github.com/dukasn/everyinit/actions/runs/123456789) for the advisory and dependency path. The preview remains available for inspection.

| Check | Status | Details |
| :--- | :--- | :--- |
| **Required CI** | ❌ Blocked | Dependency audit · [Workflow run](https://github.com/dukasn/everyinit/actions/runs/123456789) |
| **Vulnerabilities** | ❌ 1 found | [Audit report](https://github.com/dukasn/everyinit/actions/runs/123456789) |
| **Maintenance** | ⚠️ 4 unmaintained notices | Informational · [Audit report](https://github.com/dukasn/everyinit/actions/runs/123456789) |

<sub>Last checked `YYYY-MM-DD HH:MM UTC` on commit `a1b2c3d`</sub>

## Retired preview

<!-- everyinit-site-preview -->
### 🚀 Release Preview · Retired

The preview deployment for commit [`a1b2c3d`](https://github.com/dukasn/everyinit/commit/a1b2c3d) was retired when this pull request closed.

<sub>Retired `YYYY-MM-DD HH:MM UTC`</sub>

## Posting rules

- Use the same `everyinit-site-preview` marker so the workflow updates one comment.
- Put the proposed version and bump type in the heading; link the preview, compare, and exact commit in the action line.
- Use the `TIP` callout when there are no vulnerabilities or unmaintained notices, `WARNING` when only informational maintenance notices remain, and `CAUTION` when a known vulnerability blocks release.
- Keep vulnerability and maintenance results in separate rows. Link each result to its audit run.
- Include platform probes only when they ran, and label them informational. Omit rows for probes that did not run.
- Show the audit timestamp and the commit it evaluated. A scheduled audit refresh should update both.
- On PR close, replace the report with the retired-preview notice.
