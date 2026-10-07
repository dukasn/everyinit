# Evin LSP completion TODO

## Objective

Make completion intentional at every Flow cursor position. When Evin knows the
valid candidates for a position, return only those candidates. When the
position has no meaningful candidates or is not a supported completion
context, return an explicit empty completion result (`[]`). Never let a
missing Evin context fall through to unrelated schema keys, symbols, or generic
word suggestions from Evin's completion provider.

An empty Evin result controls Evin's items. Some editors merge results from
other providers or show word-based suggestions independently; verify the
supported editor integration too, and document or configure it if it needs a
setting to suppress generic suggestions.

## Current state

- Completion now resolves the narrowest schema/reference context and returns a
  JSON array, including `[]` for comments, commands, dynamic keys, and unknown
  value contexts. It does not fall back to enclosing schema fields.
- Completion covers typed keys/values, action descriptors, stages, presets,
  hooks, environment names, task references, in-scope template values and
  prerequisite outputs, and workspace-relative module/namespace paths.
- Task suggestions honor module and namespace scopes, aliases, local shorthand,
  and workspace qualification for shadowed root tasks. Prefixes such as `rel`
  and `release:` narrow to the applicable scope and tasks.
- Completion edits replace the active token and the LSP adapter converts byte
  ranges to UTF-16. Incomplete parse recovery is stored separately from the
  validated AST, so it can anchor completion without feeding semantic tokens,
  hovers, validation, or cross-file symbols.
- Built-in environment preset names and variables have one shared definition
  used by runtime environment construction, validation, and completion.
- VS Code's generic client can still add word suggestions independently of
  Evin. The LSP guide documents the `editor.wordBasedSuggestions` setting for
  the configured language ID; protocol tests assert that Evin returns `[]` in
  unsupported positions.

## Requirements

### 1. Resolve a completion context before collecting candidates

- [x] Add an explicit completion-context result for every request position.
      It must distinguish a supported context with candidates, a supported
      context with no candidates, and an unsupported or ambiguous context.
- [x] Resolve the smallest applicable Flow schema or reference context at the
      cursor. Do not use an enclosing object schema as a fallback when the
      cursor is inside a value, comment, scalar, or dynamic map key that has a
      different meaning.
- [x] Resolve positions in unsaved buffers and incomplete prefixes without
      writing buffers to disk. Do not offer stale declarations from a previous
      buffer when the current declaration has changed or become invalid.
- [x] Return an explicit empty list for comments, plain command strings,
      arbitrary user-defined map keys, unsupported value positions, and
      contexts where no valid choices exist. Do not return `null`, omit the
      response, or substitute top-level configuration fields in these cases.

### 2. Cover configuration keys and values

- [x] Cover root and nested keys in workspace, module, namespace, environment,
      and GitRules manifests using their owning typed schemas.
- [x] Cover task fields and nested structures, including triggers, watches,
      action steps, and action `with:` fields. Keep action field metadata and
      conditions owned by each action descriptor.
- [x] Offer finite schema values only where a field declares finite choices.
      Include workspace execution modes, booleans where useful, and other
      schema enums; do not invent choices for free-form strings.
- [x] Offer valid stage names in task `stage` values and valid environment
      preset names in task `presets` values. Include built-in and declared
      custom presets, respecting their documented override rules.
- [x] Offer workspace-relative path candidates in module-discovery and
      namespace-include path values only when the cursor is in those fields.
      Do not show file paths in unrelated string values.
- [x] Omit already-present fixed keys and duplicate candidates. Preserve
      schema snippets and documentation where the client supports them.

### 3. Complete references from scope and type

- [x] In task `needs` entries, offer only tasks that can be referenced from
      the owning task's scope, applying the same rules as validation and task
      resolution.
- [x] Support progressively typed qualified references. For example, typing
      `rel` may produce `release:build`; after typing `release:`, narrow to
      tasks in that scope. Support nested module namespaces and include
      aliases where the configuration permits them.
- [x] Offer local shorthand only where it resolves in the current module or
      namespace. Offer workspace-root qualification where needed to
      disambiguate shadowed task names. Never suggest a candidate that the
      checker would reject at that position.
- [x] Offer visible task names in other task-reference contexts, including
      task-output template references, using that syntax's valid forms.
- [x] Offer only in-scope value names and environment names in their
      respective template and environment-selection contexts. Do not confuse
      task-local, module, namespace, and workspace-level declarations.
- [x] Treat modules and namespaces as scopes, not as a global list of
      unrelated words. Suggest a parent scope while the user types its
      qualifier, then narrow to children after the separator where applicable.

### 4. Keep completion edits precise

- [x] Compute the active token and replacement range from the Flow syntax and
      completion context. Replacing a completion must not duplicate a typed
      prefix such as `rel` or disturb neighboring YAML punctuation.
- [x] Keep workspace byte offsets internal and convert ranges to LSP UTF-16
      positions at the protocol boundary, including when non-ASCII text comes
      before the cursor.
- [x] Advertise trigger characters that match supported syntax, including
      scope separators and template delimiters. Do not rely on a trigger
      character as the only way to request completion while typing a prefix.
- [x] Respect client snippet support and return deterministic candidate order,
      labels, filtering text, and replacement behavior.

### 5. Prevent irrelevant editor suggestions

- [x] Make every completion branch return either context-valid candidates or
      an explicit empty list. Remove broad fallback behavior that exposes
      enclosing-schema keys in a different context.
- [x] Verify that a valid empty Evin result stays empty in supported editors.
      If an editor independently adds word-based or third-party suggestions,
      document or configure that editor integration rather than fabricating
      Evin candidates to compete with them.
- [x] Ensure stale or malformed input does not cause generic completion to
      appear. Preserve candidates from unaffected declarations only when the
      current semantic context can still be resolved safely.

## Coverage matrix

For each row, verify both a positive completion case and a negative case that
must return `[]` or exclude out-of-scope candidates.

| Context | Candidates | Empty or exclusion case |
| --- | --- | --- |
| Fixed object keys | Missing keys valid for the current schema object | Cursor in a scalar/comment; keys already present |
| Finite schema values | Declared enum or constrained values | Free-form strings; invalid or unrelated fields |
| Task `stage` | Declared workspace stages | `needs`, `run`, and arbitrary strings |
| Task `needs` | Visible root, module, and namespace tasks | Unknown scope; task outside current visibility |
| Qualified task prefix | Matching parent scopes and tasks such as `release:build` | Unrelated scopes; invalid qualification |
| Task output template | Valid task outputs in supported template syntax | Ordinary command text; unsupported template syntax |
| Environment selection | Declared environment names and valid presets | Other string fields; undefined names |
| Values/templates | Visible workspace, module, namespace, and task values | Names outside the current visibility rules |
| Module/namespace paths | Relevant workspace-relative directories or included files | Other path and string fields |
| Action `with:` | Fields and values from the selected action descriptor | Unknown action or cursor outside `with:` |
| Dynamic declaration keys | No generic suggestions unless the language defines candidates | Task IDs, arbitrary values, package IDs, variable names |
| Comments, commands, unknown contexts | Explicit empty Evin result | Must not fall back to generic Evin schema candidates |

## Implementation direction

- [x] Keep context resolution and editor-neutral candidates in
      `evin-config` semantic analysis. Keep LSP protocol objects, client
      capabilities, and UTF-16 conversion in the LSP adapter.
- [x] Reuse the semantic index, source ranges, typed Flow schemas, scope
      resolution, template parser, and action descriptors. Do not create a
      parallel task or action registry for completion.
- [x] Keep completion advisory. The shared validator remains authoritative
      and must accept the same task/stage/scope references completion offers.

## Acceptance criteria

- A request at every supported schema or reference position returns only
  candidates valid at that position; a request elsewhere returns `[]` from
  Evin.
- Typing `rel` in a task reference can complete to `release:build`, and
  typing a scope prefix narrows candidates without offering unrelated tasks.
- Stage completion, local shorthand, qualified module/namespace references,
  aliases, template names, environment selections, and native action fields
  match the corresponding semantic and runtime rules.
- Completion replacements preserve YAML syntax and correctly replace partial
  tokens with Unicode before the cursor.
- Tests prove both candidates and empty results through the semantic API and
  the actual LSP protocol, including supported editor behavior for generic
  suggestions.
