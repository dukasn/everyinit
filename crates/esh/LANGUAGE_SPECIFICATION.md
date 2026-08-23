# `esh` (Evin Shell) Language Specification

**Version:** 1.0.0-draft

**Status:** Canonical design specification

**Target module:** `crates/esh` (library; standalone CLI planned)

**Host ecosystem:** Evin

## 1. Overview

`esh` is a deterministic, embeddable, cross-platform scripting language and
execution runtime. Its syntax intentionally resembles the portable parts of
POSIX shell, but an `esh` program is parsed and executed by Rust code. It never
delegates script interpretation to a host shell.

The language has four central properties:

1. **No host shell dependency.** Scripts do not require `sh`, `bash`,
   `cmd.exe`, PowerShell, MSYS2, or WSL.
2. **Portable semantics.** Filesystem behavior, paths, environment inheritance,
   line endings, streams, and cancellation are defined by `esh`, rather than by
   the host's shell implementation.
3. **Safe expansion.** Variable expansion is one argument by default. It does
   not silently split on whitespace or expand globs.
4. **Validated execution.** A script is lexed, parsed, and lowered to an
   immutable execution plan before execution begins. Syntax, built-in
   resolution, and declarative argument contracts are checked before an
   operation changes the filesystem.

`esh` is not a byte-for-byte POSIX shell implementation. Features not stated
in this specification are unsupported until specified. In particular, shell
`eval`, implicit expansion splitting, host-specific syntax, and automatic
shell-function import are out of scope.

## 2. Source text and lexical rules

### 2.1 Encoding and spans

Source text is UTF-8. Syntax operators and identifiers use ASCII recognition;
string content may contain Unicode. Every token and syntax node records an
`evin_diagnostic::Span` with its one-based line and column plus byte offset and
length. This lets callers associate each failure with the original source.

### 2.2 Whitespace and comments

- Space, tab, and carriage return separate tokens where separation is needed.
- A newline or semicolon terminates a statement, except inside quotes,
  substitutions, or an unfinished grouped expression.
- `#` starts a line comment when it appears where a word could start; the
  comment ends at the next newline.
- Backslash-newline joins physical lines. Backslash otherwise escapes the next
  character in an unquoted or double-quoted word.

### 2.3 Tokens

| Category | Syntax | Meaning |
| --- | --- | --- |
| Control | `&&`, `||`, `&|`, `|`, `;`, newline | logical, parallel, pipe, and statement flow |
| Grouping | `{`, `}`, `(`, `)` | block and subshell boundaries |
| Redirection | `>`, `>>`, `<`, `2>`, `2>>`, `&>`, `&>>` | file-descriptor routing |
| Expansion | `$name`, `${name}`, `$?`, `$0`–`$9`, `$@`, `$(...)` | variable, status, positional, and command expansion |
| Words | bare words, `"..."`, `'...'` | command names and arguments |

Single-quoted words are literal. Double-quoted and unquoted words may contain
variable and command expansions. An unquoted `*`, `?`, or `[` is an ordinary
character; it does not invoke pathname expansion.

## 3. Syntax

The following EBNF defines the initial language surface. `Separator` is one or
more statement terminators.

```ebnf
Script              ::= StatementList EOF
StatementList       ::= (Statement (Separator Statement)*)? Separator?
Separator           ::= ("\n" | ";")+

Statement           ::= LogicalChain
LogicalChain        ::= ParallelChain (("&&" | "||") ParallelChain)*
ParallelChain       ::= Pipeline ("&|" Pipeline)*
Pipeline            ::= Command ("|" Command)*

Command             ::= SimpleCommand | FunctionDeclaration | Block | Subshell
SimpleCommand       ::= Assignment* Word (Word | Redirection)*
Assignment          ::= Identifier "=" Word
FunctionDeclaration ::= Identifier "(" ")" Separator? Block
Block               ::= "{" StatementList "}"
Subshell            ::= "(" StatementList ")"
Redirection         ::= RedirectOperator Word
RedirectOperator    ::= ">" | ">>" | "<" | "2>" | "2>>" | "&>" | "&>>"
Word                ::= WordPart+
WordPart            ::= Bare | SingleQuoted | DoubleQuoted | VariableRef | CommandSub
VariableRef         ::= "$" Identifier | "${" Identifier "}" | "$?" | "$@" | "$" Digit
CommandSub          ::= "$(" StatementList ")"
Identifier          ::= IdentifierStart IdentifierContinue*
IdentifierStart     ::= "_" | "A"…"Z" | "a"…"z"
IdentifierContinue  ::= IdentifierStart | Digit
```

Operator precedence, from lowest to highest, is: statement terminator;
`&&`/`||`; `&|`; `|`; redirection; and word expansion. Operators at the same
level associate left-to-right. Redirections attach to the command that contains
them rather than forming a standalone expression.

## 4. Execution model

`esh` has three stages:

1. **Parse.** The lexer and parser produce a spanned AST without executing any
   command.
2. **Plan.** The compiler lowers the AST to an immutable `ExecutionPlan`. This
   resolves built-ins, prepares external command invocations, and validates
   declarative `getopts` contracts.
3. **Run.** The async runtime evaluates the plan using an `ExecutionContext`.
   It owns the virtual working directory, environment, positional arguments,
   functions, traps, stream handles, cancellation state, and exit status.

An assignment before a simple command applies only to that command unless it
appears as a standalone assignment statement. Blocks share their parent's
context. A subshell receives a child context, so its directory, variables,
functions, and traps do not mutate the parent.

External commands are launched directly as programs with an argument vector;
they are never passed through a shell. The command lookup policy is: a built-in
first, then an executable found in the context's sanitized `PATH`. A missing
program exits with `127`.

### 4.1 Status and logical flow

- `a && b` runs `b` only when `a` exits `0`.
- `a || b` runs `b` only when `a` exits non-zero.
- `a | b` connects `a`'s stdout to `b`'s stdin. The pipeline status is the
  rightmost command's status.
- `$?` contains the immediately preceding command, pipeline, or chain status.
- `$(...)` executes in a child context and substitutes its stdout with trailing
  line terminators removed. A non-zero substitution status fails the containing
  command unless a surrounding `&&` or `||` handles it.

### 4.2 Parallel joins: `&|`

`&|` runs each adjacent pipeline concurrently and creates a synchronization
barrier. All branches must complete before the following statement starts.

```esh
cargo build --bin api &| cargo build --bin worker &| npm run build --prefix ./web
```

The result is `0` only if every branch succeeds; otherwise it is the first
non-zero status observed in source order. Branch stdout and stderr are
line-buffered before forwarding to a terminal or Evin log sink so messages do
not corrupt each other. Branches get isolated mutable execution contexts.

### 4.3 Fail-fast blocks

A block that is the left operand of a block fallback is fail-fast:

```esh
{
    mkdir -p ./dist
    cargo build --release
    cp ./target/release/app ./dist/
} || {
    echo "Pipeline failed at $ESH_FAILED_CMD (line $ESH_FAILED_LINE)"
    rm -rf ./dist
    exit 1
}
```

When a statement in the first block fails, remaining statements in that block
are skipped and the fallback block runs. Within the fallback scope,
`ESH_FAILED_CMD`, `ESH_FAILED_LINE`, and `ESH_FAILED_MSG` describe the failed
statement. A block without this form follows normal logical-chain behavior.

## 5. Variables, functions, and traps

Variables are strings. `$0` is the script name and `$1` through `$9` are
positional arguments. `$@` expands to the positional argument list, preserving
argument boundaries. Function parameters inherit the caller's positional
arguments unless the invocation provides new ones.

Expanding a scalar variable produces exactly one argument even when unquoted:

```esh
DIR="target build"
rm -rf $DIR     # one path, equivalent to rm -rf "target build"
```

Splitting must be explicit through an array-aware built-in or iteration feature
once that feature is specified. There is no implicit glob expansion.

`trap { ... } ERR` registers an error handler for the current lexical scope.
A subshell inherits a trap and may replace it; the replacement disappears when
the subshell exits. The handler can read the `ESH_FAILED_*` variables. Trap
execution and cancellation details are implementation work, but must preserve
this lexical-scope rule.

## 6. Built-ins

Built-ins execute in-process against the `ExecutionContext`. They are resolved
while building the execution plan, not by spawning a host utility. The initial
set is intentionally small and portable:

| Built-in | Required behavior |
| --- | --- |
| `cd [path]` | Change the context's virtual working directory. |
| `mkdir [-p] paths…` | Create directories, including parents with `-p`. |
| `rm [-r] [-f] paths…` | Remove files or directories according to the flags. |
| `cp [-r] source destination` | Copy files or directories. |
| `mv source destination` | Move or rename a path. |
| `touch paths…` | Create files or update their timestamps. |
| `exit [status]` | Terminate the current script or function context. |
| `return [status]` | Terminate the current function context. |
| `getopts` | Parse either a POSIX-style option string or an `esh` contract. |

The planned `path`, `str`, `json`, `yaml`, `toml`, `http`, `prompt`, and
`mktemp` utilities are language built-ins only after their flags, input/output,
security, and dry-run behavior are specified. Their examples are not a promise
of current availability.

### 6.1 Declarative `getopts`

The form below selects contract mode by using `$@` as the first operand. It
validates options before the function body continues, binds values as local
variables, and reserves `-h`/`--help` for generated help output.

```esh
deploy() {
    getopts "$@" \
        --about "Deploy build artifacts" \
        -e, --env <target> "Target environment" default="staging" \
        -v, --verbose "Enable debug output" \
        <artifact_path> "Path to the archive" \
        || return $?
}
```

Traditional `while getopts "e:v" opt; do ... done` remains supported. Invalid
contracts, missing required positional arguments, and unknown options are
diagnostic errors rather than silent parser failures.

## 7. Diagnostics, output, and exit statuses

`esh` uses Evin's native diagnostic model. Core APIs return
`evin_diagnostic::Diagnostic`, with an `IssueCode`, severity, hint, and an
`evin_diagnostic::Span`-derived source location. It does **not** expose or
depend on `miette`.

The standalone CLI renders diagnostics through `evin-ui`. An Evin integration
should forward diagnostics and raw program streams through `evin-action-sdk`'s
existing message protocol, preserving the distinction between diagnostics,
activity, structural debug records, and command output. User-facing issue text
is rendered by Evin's locale system; the core uses stable issue-code keys.

Initial exit statuses follow shell convention:

| Code | Meaning |
| --- | --- |
| `0` | success |
| `1` | general runtime or validation failure |
| `2` | parse or syntax failure |
| `126` | command found but cannot execute |
| `127` | command not found |
| `130` | interrupted by the user |

## 8. Evin integration

`esh` is a library first. A host constructs an engine and context, evaluates a
string or source file, and receives an exit status or structured diagnostic.
The final public Rust API is intentionally deferred until the parser and plan
types stabilise; the following is illustrative rather than a committed
signature:

```rust,ignore
let engine = esh::Engine::new();
let mut context = esh::ExecutionContext::new();
let status = engine.eval_str(task_script, &mut context).await?;
```

For Evin, the integration point is `evin-pipeline`'s shell runner, which today
selects `sh -c` on Unix and `cmd.exe /c` on Windows. An `esh` runner should
replace that host-dependent strategy while retaining the runner's cancellation,
working-directory, environment, and action-log contracts. There is no
`evin-core` crate in this workspace.

The planned CLI surface is:

```console
esh -c 'mkdir -p ./dist &| cargo build'
esh ./scripts/deploy.esh --env production ./dist/app.tar.gz
esh --dry-run ./scripts/deploy.esh
```

`--dry-run` must create and display the plan without performing filesystem,
network, prompt, or subprocess effects.

## 9. Compatibility and evolution

This document is the normative source for language behavior. Any behavior
marked planned or illustrative is not part of the implementation contract yet.
Changes to grammar, status semantics, built-ins, or the embedding API require
an update to this document and tests that exercise the changed behavior.
