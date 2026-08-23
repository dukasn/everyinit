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

pub mod classifier;
pub mod emitter;
pub mod lut;
pub mod position;
pub mod token;

pub use classifier::SemanticClassifier;
pub use emitter::{
    AnsiColorEmitter, HtmlClassedEmitter, LspSemanticTokensEmitter, SyntaxEmitter, TokenCollector,
};
pub use position::SourcePositionIndex;
pub use token::{SyntaxCategory, SyntaxToken, TokenKind, TokenSpan};

/// Tokenize a line of source code into syntax tokens with precise UTF-8 and UTF-16 positions.
pub fn scan_tokens(line: &str, line_idx: usize) -> Vec<SyntaxToken<'_>> {
    let mut collector = TokenCollector::new();
    let mut classifier = SemanticClassifier::new_for_line(line, line_idx);
    classifier.classify_into(&mut collector);
    collector.tokens
}

/// Highlight source code to static HTML with customized CSS class prefix (e.g. "syn-").
/// Emits directly with zero intermediate allocations.
pub fn highlight_html_classed(source: &str, prefix: &str) -> String {
    let mut output = String::with_capacity(source.len() * 2);
    let mut emitter = HtmlClassedEmitter::new(&mut output, prefix);
    let mut classifier = SemanticClassifier::new(source);
    classifier.classify_into(&mut emitter);
    output
}

/// Highlight source code to a standalone `<pre class="esh-highlight"><code>...</code></pre>` block.
pub fn highlight_html(source: &str) -> String {
    let mut output = String::with_capacity(source.len() * 2 + 64);
    output.push_str("<pre class=\"esh-highlight\"><code>");
    let mut emitter = HtmlClassedEmitter::new(&mut output, "esh-");
    let mut classifier = SemanticClassifier::new(source);
    classifier.classify_into(&mut emitter);
    output.push_str("</code></pre>");
    output
}

/// Highlight source code with ANSI escape colors for terminal display.
pub fn highlight_ansi(source: &str, line_numbers: bool) -> String {
    let lines_count = source.lines().count().max(1);
    let mut output = String::with_capacity(source.len() * 2);
    let mut emitter = AnsiColorEmitter::new(&mut output, line_numbers, lines_count);
    let mut classifier = SemanticClassifier::new(source);
    classifier.classify_into(&mut emitter);
    output
}
