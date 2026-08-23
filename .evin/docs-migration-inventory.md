# Documentation migration inventory

| Current source | Markdown destination | Section | Authority | Status |
| --- | --- | --- | --- | --- |
| `docs/index.rst` | `docs/index.md` | Start page | Curated | Migrated |
| `docs/getting-started/*.rst` | `docs/getting-started/NN.*.md` | Getting Started | Curated; filename order metadata | Migrated |
| `docs/guides/*.rst` | `docs/guides/NN.*.md` | Guides | Curated; filename order metadata | Migrated |
| `docs/builtins/*.rst` | `docs/builtins/NN.*.md` | Built-ins | Curated; filename order metadata | Migrated |
| `docs/reference/esh-builtins-catalog-3010.rst` | `docs/esh/100.runtime-reference.3010.md` | ESH | Curated Markdown source; generates `crates/esh/assets/builtins-catalog.json` | Migrated |
| `docs/reference/*.rst` | `docs/reference/NN.*.md` | Reference | Curated; filename order metadata | Migrated |
| `docs/contributing/*.rst` | `docs/contributing/NN.*.md` | Contributing | Curated; filename order metadata | Migrated |
| `docs/guides/visual-test-gallery-*.{md,rst}` | `builtins/evin-action-docs/testdata/visual-gallery.md` | Internal fixture | Curated test fixture | Migrated |

The complete pre-migration source snapshot is stored outside the repository at
`/tmp/evin-docs-before-markdown-20260820T213227Z`.
