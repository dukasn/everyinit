# Changelog
All notable changes to this project will be documented in this file.

Everyinit is canary-era software. CLI and ESH downloads use the canary mirror channel; npm and Marketplace packages use regular workspace versions. The extension is not published as a VS Code pre-release.

## [v0.3.1]

### Bug Fixes

- **esh:** create parent directories portably (a290696)
- **platform:** correct Windows path and touch tests (f80ba19)
- **platform:** preserve Windows copy metadata (7cbfefe)
- **platform:** handle native test differences (e651297)
- **esh:** gate Unix-only test helper import (7ba705e)
- **platform:** run native probes directly (9b9a654)
- **platform:** read Windows shell variables case-insensitively (5724167)
- **platform:** preserve Windows shell environment (077e412)
- **platform:** make native probes portable (33442c4)
- **ci:** render preview audit alerts (3612e14)
- **vscode:** align engine with terminal APIs (8294fac)
- **platform:** compile probes on macOS and Windows (6087edd)

### Documentation

- **contributing:** reorganize contributor guidance (e8e6f01)

### Features

- **esh:** add portable true and false builtins (2e6b4b0)
- **ci:** audit release proposal dependencies (725179f)

### Maintenance

- **infra:** refresh generated worker types (958ea78)
- **deps:** upgrade Rust and Node dependencies (426cc87)

### Tests

- **esh:** remove remaining Unix trap commands (0cc7b0b)
- **esh:** make trap tests platform independent (3c0c6d1)
- **platform:** normalize macOS temporary paths (5d702b7)
- **esh:** account for Apple grep quiet mode (c00d405)
- **esh:** account for Apple BSD grep output (91616f6)
- **esh:** make environment tests portable (4d341c6)
- **esh:** account for Apple Bash parameter expansion (724600f)
- **platform:** cover Windows paths and Apple Bash (9e93012)
- **esh:** skip legacy Bash NaN comparison (e6249a1)
- **evin:** gate POSIX release workflow suite (18d3b64)
- **esh:** account for Apple Bash 3.2 differences (935e885)
- **platform:** improve probe diagnostics (2098e4d)
- **site:** assert rendered Mermaid labels (bb0ff5b)

## [v0.3.0]

### Bug Fixes

- **ci:** use pinned npm for release reconciliation (3845dc8)

### Documentation

- add security reporting policy (78b988b)
- **book:** make installation CLI-first (32c621f)

### Features

- **site:** refresh home and docs experience (7cce4c5)
- **docs:** render diagrams inline in generated sites (1246400)
- **highlighting:** add standard JSON grammar support (afe215b)
- move policy and environment manifests to JSON (63fb939)

### Maintenance

- **ci:** configure platform probe tool installs (7b39b32)
- **ci:** add optional platform probe (5506ab5)

## [v0.2.10]

### Bug Fixes

- **vscode:** improve task execution feedback (9163381)
- **vscode:** open repository workspace on F5 (fd52eb9)

### Documentation

- **site:** move documentation to the main domain (77b39ca)

### Features

- **config:** allow nested namespace task files (eca033d)
- **lsp:** relay task events and defer workspace analysis (c49cfee)

### Refactoring

- **tasks:** streamline workspace task names (cdfe608)

## [v0.2.9]

### Bug Fixes

- **esh:** honor mktemp template paths (7b1747c)
- **esh:** honor mktemp template directories (b80688f)
- **vscode:** build F5 with the pinned Node toolchain (9242808)
- **test:** parse release script arguments (c69f09b)
- **release:** stage GitHub package copy correctly (af3b3e6)
- **release:** avoid shell umask in npm publisher (8f4921e)
- **release:** scope bootstrap token to npmjs (5407d17)
- **release:** load current publisher configuration (0180056)
- **release:** bootstrap npmjs publishing with token (8065daf)
- **release:** publish npm package from local path (84b749d)

### Documentation

- **install:** clarify supported platforms and routes (0268f4e)

### Features

- **release:** preview release PRs and remove unstable channel (e18d6eb)
- **site:** localize pages and move docs host (7632bbd)
- **site:** add current install and platform links (cb6da08)
- **release:** publish VS Code extension with OIDC (8d0d6c2)

### Maintenance

- **licenses:** remove unused Tabler notice (f02721c)
- **github:** add GitHub Sponsors username in FUNDING.yml (b52abe3)

### Refactoring

- **i18n:** map builtin diagnostics through Fluent (bcc7f9e)
- **i18n:** preserve localized builtin issue codes (34b68fd)
- **i18n:** localize builtin diagnostics (4e3e333)
- **i18n:** route builtin messages through Fluent (c7a6ba2)
- **builtins:** align action module versioning (737dd23)
- **release-sync:** decouple provider workflow (6b45d48)

### Tests

- **site:** keep host routing checks local (aa37af8)
- **builtins:** assert fluent diagnostics across actions (bb756e3)

## [v0.2.8]

### Bug Fixes

- **release:** use current mirror publisher for backfills (4fe635e)
- **mirror:** publish objects to remote R2 (1f120d0)
- **release:** publish artifacts to the R2 mirror (fa75eec)

### Other Changes

- **release:** publish npm package to both registries (89a162a)

## [v0.2.7]

### Bug Fixes

- **deploy:** separate Cloudflare deployment tokens (6e2069b)

### Features

- **deploy:** automate Cloudflare site releases (c5cf494)

## [v0.2.6]

### Bug Fixes

- **docs:** preserve docs action in generated catalog (2196479)
- **release:** keep docs version metadata in sync (2a0b36c)
- **release:** align web versions with workspace tag (253f827)
- **site:** prevent mobile hero overflow (751da5e)
- **build:** lock npm checksum for mise CI (607be3b)
- **docs:** align shell routes and layout with reference (1e570ff)

### Documentation

- **ide:** document editor-neutral language support (3413a1a)
- **site:** align content and layout with GitHub Markdown (71132b5)

### Features

- **site:** refine responsive homepage navigation (db2587d)
- **web:** add unified npm integrations and Astro docs site (4cc9e9c)
- **docs:** emit headless search-ready documentation data (555e3fe)
- **docs:** add versioned localized builds (ac04384)
- **docs:** separate editor catalog and simplify page rendering (dd0b0ce)
- **esh:** publish task state and redact runtime secrets (2e02bab)
- **esh:** expand POSIX compatibility (755c40e)

### Maintenance

- **package:** format Everyinit manifest (8cde5ba)

### Other Changes

- **workspace:** align mise and web release automation (3523fb7)

### Refactoring

- **docs:** make docs action emit headless data (713470f)
- **docs:** make layout defaults project agnostic (c7c2060)

### Tests

- **esh:** pin locale in sort differential case (67ddcb8)

## [v0.2.5]

### Bug Fixes

- **process:** retry Unix process spawn on transient ETXTBSY (2ee2179)
- **esh:** support stderr redirection and positional arg expansion (9062602)

### Features

- **evin:** enable debug and verbose output when RUNNER_DEBUG is set (9e817d2)
- **release:** add GitHub release orchestration ESH tasks (a789779)

### Other Changes

- **release:** run release and distribution tasks with --verbose (e16deec)

### Refactoring

- **release:** delegate GitHub release workflow steps to Evin (07af68d)

## [v0.2.4]

### Bug Fixes

- **esh:** extract tar streams from stdin (f8ee15b)
- **release:** inspect complete APK payloads (ec154dc)
- **esh:** verify distribution archives (635e8d0)
- **release:** update package script paths (1fc4627)

### Features

- **esh:** add Ed25519 signing support (d4a4a4b)
- **esh:** migrate release verification to native scripts (c63dce6)
- **esh:** verify Ed25519 signatures (dc8881e)
- **esh:** add checksum and text utilities (56f3e99)

### Refactoring

- **release:** organize ESH automation (6d1536d)
- **release:** simplify ESH control flow (bbcb8f6)
- **release:** simplify package manifest checks (1f2127f)

## [v0.2.3]

### Bug Fixes

- **ci:** stage release archives before packaging extension (6bab83a)
- **ci:** verify native packages with resolved revisions (f4f5f5f)
- **flow:** preserve quoted scalar strings and bind package-state release (ebcd2d5)

### Features

- **ci:** build and sign deb, arch, and vscode extension release packages (1916c7a)

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
