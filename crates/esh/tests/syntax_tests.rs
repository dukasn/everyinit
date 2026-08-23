// Copyright (C) 2026 Eduardo Marques Santos
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

use esh::syntax::{highlight_ansi, highlight_html, highlight_html_classed, scan_tokens};

#[test]
fn test_syntax_token_scanner() {
    let line = "if read -r -p 'Name: ' USERNAME; then # Prompt user";
    let tokens = scan_tokens(line, 0);

    assert!(
        tokens
            .iter()
            .any(|t| t.text == "if" && t.kind == esh::syntax::TokenKind::Keyword)
    );
    assert!(
        tokens
            .iter()
            .any(|t| t.text == "read" && t.kind == esh::syntax::TokenKind::Builtin)
    );
    assert!(
        tokens
            .iter()
            .any(|t| t.text == "'Name: '" && t.kind == esh::syntax::TokenKind::String)
    );
    assert!(
        tokens
            .iter()
            .any(|t| t.text == "USERNAME" && t.kind == esh::syntax::TokenKind::Plain)
    );
    assert!(
        tokens
            .iter()
            .any(|t| t.text == "# Prompt user" && t.kind == esh::syntax::TokenKind::Comment)
    );
}

#[test]
fn test_highlight_html_classed_with_syn_prefix() {
    let script = r#"
# Deploy script
TARGET="prod"
if read -c -p "Deploy to $TARGET?"; then
    echo "Deploying..."
fi
"#;

    let html = highlight_html_classed(script, "syn-");

    assert!(html.contains("<span class=\"syn-comment\"># Deploy script</span>"));
    assert!(html.contains("<span class=\"syn-keyword\">if</span>"));
    assert!(html.contains("<span class=\"syn-keyword\">then</span>"));
    assert!(html.contains("<span class=\"syn-keyword\">fi</span>"));
    assert!(html.contains("<span class=\"syn-entity syn-function\">read</span>"));
    assert!(html.contains("<span class=\"syn-string\">prod</span>"));
    assert!(html.contains("<span class=\"syn-string\">&quot;</span>"));
}

#[test]
fn test_highlight_html_container() {
    let script = "read -r LINE";
    let html = highlight_html(script);

    assert!(html.starts_with("<pre class=\"esh-highlight\"><code>"));
    assert!(html.ends_with("</code></pre>"));
    assert!(html.contains("<span class=\"esh-entity esh-function\">read</span>"));
}

#[test]
fn test_highlight_ansi_formatting() {
    let script = "if [ 1 -eq 1 ]; then echo 'yes'; fi";
    let ansi = highlight_ansi(script, true);

    assert!(ansi.contains("1 │"));
    assert!(ansi.contains("\x1b[1;35mif\x1b[0m"));
}

#[test]
fn test_parallel_pipeline_and_flag_highlighting() {
    let script = "cargo test --lib &| cargo test --tests &| cargo fmt --check";
    let html = highlight_html_classed(script, "syn-");

    // All `cargo` command occurrences should be styled as entity/function commands
    assert!(html.contains("<span class=\"syn-entity syn-function\">cargo</span> test <span class=\"syn-parameter syn-attribute\">--lib</span>"));
    assert!(html.contains("<span class=\"syn-operator\">&amp;|</span> <span class=\"syn-entity syn-function\">cargo</span> test <span class=\"syn-parameter syn-attribute\">--tests</span>"));
    assert!(html.contains("<span class=\"syn-operator\">&amp;|</span> <span class=\"syn-entity syn-function\">cargo</span> fmt <span class=\"syn-parameter syn-attribute\">--check</span>"));
}

#[test]
fn test_math_arithmetic_expansion_highlighting() {
    let script = r#"
# Basic math and parentheses
TOTAL=$(( (COUNT * 10 + 5) / 2 ))

# Radix number literals (hex, binary, octal)
FLAGS=$(( 0xFF & 0b11110000 ))

# In-place variable updates
NEXT=$(( ++INDEX ))
$(( TOTAL += 100 ))

# Ternary conditionals
STATUS=$(( CODE == 200 ? 1 : 0 ))
"#;

    let html = highlight_html_classed(script, "syn-");

    // Check numbers are styled as constants/numbers
    assert!(html.contains("<span class=\"syn-constant syn-number\">10</span>"));
    assert!(html.contains("<span class=\"syn-constant syn-number\">0xFF</span>"));
    assert!(html.contains("<span class=\"syn-constant syn-number\">0b11110000</span>"));
    assert!(html.contains("<span class=\"syn-constant syn-number\">200</span>"));

    // Check variables are styled as variables
    assert!(html.contains("<span class=\"syn-variable\">COUNT</span>"));
    assert!(html.contains("<span class=\"syn-variable\">INDEX</span>"));
    assert!(html.contains("<span class=\"syn-variable\">CODE</span>"));
    assert!(html.contains("<span class=\"syn-variable\">TOTAL</span>"));

    // Check operators are styled as operators
    assert!(html.contains("<span class=\"syn-operator\">$((</span>"));
    assert!(html.contains("<span class=\"syn-operator\">))</span>"));
    assert!(html.contains("<span class=\"syn-operator\">++</span>"));
    assert!(html.contains("<span class=\"syn-operator\">+=</span>"));
    assert!(html.contains("<span class=\"syn-operator\">==</span>"));
    assert!(html.contains("<span class=\"syn-operator\">?</span>"));
    assert!(html.contains("<span class=\"syn-operator\">:</span>"));
}

#[test]
fn test_posix_test_builtin_and_loop_highlighting() {
    let script = r#"
COUNT=0
while [ "$COUNT" -lt 5 ]; do
    COUNT=$(( COUNT + 1 ))
done
"#;

    let html = highlight_html_classed(script, "syn-");

    // Check assignment value 0 is styled as number
    assert!(html.contains("<span class=\"syn-variable\">COUNT</span><span class=\"syn-operator\">=</span><span class=\"syn-constant syn-number\">0</span>"));

    // Check `[` is recognized as builtin command
    assert!(html.contains("<span class=\"syn-entity syn-function\">[</span>"));

    // Check double quoted string with interpolated variable
    assert!(html.contains("<span class=\"syn-string\">&quot;</span><span class=\"syn-variable\">$COUNT</span><span class=\"syn-string\">&quot;</span>"));

    // Check `-lt` test operator
    assert!(html.contains("<span class=\"syn-operator\">-lt</span>"));

    // Check `]` punctuation
    assert!(html.contains("<span class=\"syn-punctuation\">]</span>"));
}

#[test]
fn test_highlight_declaration_builtins() {
    let inputs = [
        "declare FOO=bar",
        "declare -r -x TARGET=\"prod\"",
        "export API_KEY=secret123",
        "local COUNT=42",
        "readonly MAX_RETRIES=5",
        "typeset -i INDEX=0",
    ];

    for src in inputs {
        let html = highlight_html_classed(src, "syn-");
        assert!(
            html.contains("<span class=\"syn-operator\">=</span>"),
            "Failed to highlight '=' operator in: {src}\nGenerated HTML:\n{html}"
        );
        assert!(
            html.contains("syn-variable"),
            "Failed to highlight variable name in: {src}\nGenerated HTML:\n{html}"
        );
    }
}

#[test]
fn test_highlight_flag_assignments_and_command_prefixes() {
    // 1. Flag assignments
    let flag_html = highlight_html_classed("cargo build --target=x86_64 --jobs=4", "syn-");
    assert!(flag_html.contains("<span class=\"syn-parameter syn-attribute\">--target</span><span class=\"syn-operator\">=</span>"));
    assert!(flag_html.contains("<span class=\"syn-parameter syn-attribute\">--jobs</span><span class=\"syn-operator\">=</span><span class=\"syn-constant syn-number\">4</span>"));

    // 2. Leading command-prefix assignments
    let prefix_html = highlight_html_classed("RUST_LOG=debug FOO=1 cargo test", "syn-");
    assert!(prefix_html.contains(
        "<span class=\"syn-variable\">RUST_LOG</span><span class=\"syn-operator\">=</span>"
    ));
    assert!(prefix_html.contains("<span class=\"syn-variable\">FOO</span><span class=\"syn-operator\">=</span><span class=\"syn-constant syn-number\">1</span>"));
    // 'cargo' must still be classified as command/entity function despite leading assignments
    assert!(prefix_html.contains("<span class=\"syn-entity syn-function\">cargo</span>"));
}
