# Changelog
All notable changes to this project will be documented in this file.

## [v0.2.2]

### Features

- **vscode:** render concurrency slots and step details in trace viewer (a65fbab)
- **pipeline:** record scoped execution traces with step telemetry (ce8e985)
- **release:** migrate release lock format to json (d66dd7f)

### Refactoring

- **flow:** rename evin-yaml crate to evin-flow (1504767)

## [v0.2.1]

### Bug Fixes

- **pipeline:** record accurate task status in execution trace events (a06918f)
- **lsp:** serialize document and workspace symbols with camelCase fields (143cc6b)
- **vscode:** expand workspace variables and add local target binary fallback (4c019e4)
- **ci:** bootstrap vscode extension dependencies and fix import (fa10d93)

### Documentation

- update reuse annotations and licensing for editor assets (ab195df)

### Features

- **vscode:** add sidebar, flow codelens, trace timeline, and icon font (6613f75)
- **lsp:** expose workspace catalog and file watch sync for editors (feb5471)
- **vscode:** enhance flow syntax highlighting and semantic scopes (2c3fac9)
- **release:** integrate vscode extension into release flow (3119355)
- **vscode:** add unified extension for flow and esh (e174f36)

## [v0.2.0]

### Bug Fixes

- **ci:** reduce runner disk usage (92e962a)
- **release:** publish reviewed changelog notes (e6ad1dc)

### Features

- **versioning:** move strategy into git rules (7e6a332)

## [v0.1.0]

### Bug Fixes

- **config:** diagnose task outputs missing direct dependencies (cb19d9f)
- **release:** preserve successful checks for non-candidate previews (678be1e)
- **release:** declare version output dependency for preparation (9c3d10c)
- **release:** forward publishing context to isolated tasks (5e4516e)
- **release:** resolve workspace version for changelog verification (c25e96a)
- **docs:** escape terminal controls in tape diagnostics (b256804)
- **infra:** match shell imports with Wrangler rule (217ddbc)
- **cli:** render config resolution in command pipelines (4471eec)
- **discovery:** preserve file URIs in wrapped output (5268843)
- **discovery:** preserve file URIs in narrow terminals (3e455c7)
- **io:** embed locales from generated build output (451e6d4)
- **docs:** fix mobile sidebar backdrop and tap behavior (05c9373)
- support Windows and Unix paths in Evin and ESH (ac8de5c)
- **docs:** improve scrolling for diagrams and terminal recordings (eb10823)
- **docs:** keep validation non-mutating (38c6e56)
- **process:** find executables through PATH on Windows (6d7b617)
- **tape:** stabilize terminal recording playback (2475549)
- **docs:** preserve inline theme colors (40ea6d8)
- **ci:** install Evin for Cloudflare tape checks (da363d2)
- **process:** supervise child processes across platforms (95675ae)

### Documentation

- **readme:** add optimized GPLv3 license badge and terms (6149517)
- **brand:** refresh the project landing page (1f0c959)
- **book:** rebuild the Evin workflow learning path (c9f3d05)
- **esh:** reorganize learning and reference guides (42552df)
- **cli:** document task working directory diagnostics (f52d9b7)
- **site:** refresh the Evin landing page (9aa7f2f)
- **archive:** explain Debian and Alpine package configuration (5352090)
- **config:** document inline package settings (756352b)
- explain Evin's native Git integration (1a48527)
- **actions:** update built-in action references (1a831a4)
- expand workflow guides and technical reference (fb98303)
- describe Evin as a workflow engine (7b8ac5d)

### Features

- **release:** automate Cargo-style GitHub releases (601dc75)
- **release:** maintain a release pull request from main (d06e05d)
- **docs:** fold editor reference cards and extend LSP catalog (c300b60)
- **review-request:** add forge-agnostic review request action (eac39b6)
- **git-release:** push release branches and prune stale tags (0516df1)
- **changelog:** write pending release notes to notes_path (fb85369)
- **version-bumper:** report whether a new version was calculated (0ea7590)
- **sdk:** add git push, tag pruning and typed process output (d1e0679)
- **release:** add signed candidate previews and canary promotion (08acc3e)
- **release:** publish validated candidates to signed mirror channels - #3 (c45e133)
- **release:** publish validated candidates to signed mirror channels (1488ebe)
- **release:** add distribution rehearsal and promotion flow (73acab2)
- **release:** add distribution rehearsal and promotion flow (9d76d78)
- **release:** publish scoped artifacts through Evin CI (704745f)
- **release:** package and distribute Evin and ESH (751264e)
- **archive:** report input and output details (ddfdc02)
- **tape:** add setup commands and aliases (7b2e2c5)
- **ui:** show verbose output for task steps (eeaf87b)
- **gitrules:** require .flow for YAML rule files (67bb3c9)
- **lsp:** add typed Flow completion and shared catalogs (e64b717)
- **config:** make Flow the canonical typed manifest format (2c5dff4)
- **lsp:** add language-server support for workspaces (dadd1b7)
- **config:** identify Evin files from document headers (bf0ed26)
- **actions:** provide editor metadata for built-in actions (571b4cc)
- **archive:** configure Debian and Alpine packages inline (90c0b36)
- **docs:** show server startup status (7ccdf88)
- **actions:** pass invocation flags to documentation builds (8900605)
- **config:** unify workspace, module, and namespace variables (6ec0ffd)
- **docs:** generate page metadata and search previews (f3907e8)
- **docs:** publish pages for declared project metrics (a3c9243)
- **archive:** build Debian and Alpine packages (dea817b)
- **release:** track package revisions (64d26e0)
- **cli:** validate action settings before execution (7f04917)
- **cli:** guide users to available workspace targets (6ceb7e3)
- **config:** support included task namespaces (c4c083b)
- **ci:** reconcile releases offline (df1d3a3)
- **tape:** adapt terminal recordings to page layout (15a1cb4)
- **docs:** render Mermaid diagrams offline (ebf0730)
- **docs:** add titled code blocks and Mermaid diagrams (cdc18c6)
- **cli:** run Evin from a source checkout (bdb1f24)
- **workspace:** add workspaces, built-in actions, and documentation engine (d39e49b)

### Maintenance

- **legal:** adopt REUSE specification and standard SPDX licensing (95ca160)
- **brand:** adopt Everyinit public identity (3f60289)
- **lsp:** remove completed completion planning artifacts (c62d7ed)
- **esh:** update the built-in action catalog (5e15c27)
- update Rust tooling and project notices (59b6b46)
- **git:** ignore generated and local-only files (653c8fd)

### Other Changes

- cache musl release build outputs (5861cdd)
- **release:** format metadata script (b3510f1)
- share Rust build outputs across validation (f53a0b8)
- cache Rust builds and refresh Worker types (1c9addb)
- **platform:** focus verification on Linux (c4cee32)
- **docs:** refine the Matter theme (5a0f44c)
- **docs:** check documentation pages in a browser (525dbd2)
- **docs:** improve mobile and desktop layouts (20c895f)
- build Linux artifacts for musl systems (7af97c6)

### Refactoring

- **installer:** serve generic package selection script (bd70afa)
- **sync:** reuse resolved config for release locks (6fe72c4)
- **watch:** split watcher implementation into modules (493d8ab)
- **test:** keep helpers within their owning crates (e4b8a00)
- **lsp:** split the workspace server into modules (cd5f8c8)
- **docs:** split site rendering into focused modules (6f96003)
- **config:** extract static workspace analysis (47a9fe4)
- **workspace:** centralize project orchestration (0a02247)
- **esh:** align maintenance scripts with shared tooling (e409e6f)

### Tests

- **cli:** account for wrapped discovery output (c0a9b7f)
