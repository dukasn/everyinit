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

use super::token::{SyntaxToken, TokenKind, TokenSpan};

/// Generic trait for streaming syntax token consumption with zero intermediate allocations.
pub trait SyntaxEmitter<'a> {
    fn emit(&mut self, kind: TokenKind, text: &'a str, span: TokenSpan);
    fn emit_newline(&mut self) {}
}

/// Emits HTML spans with customizable CSS class prefix (e.g. "syn-" or "esh-").
pub struct HtmlClassedEmitter<'a> {
    output: &'a mut String,
    prefix: String,
}

impl<'a> HtmlClassedEmitter<'a> {
    pub fn new(output: &'a mut String, prefix: &str) -> Self {
        let safe_prefix = if !prefix.is_empty()
            && prefix
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            prefix.to_string()
        } else {
            "syn-".to_string()
        };
        Self {
            output,
            prefix: safe_prefix,
        }
    }
}

impl<'a> SyntaxEmitter<'a> for HtmlClassedEmitter<'_> {
    fn emit(&mut self, kind: TokenKind, text: &'a str, _span: TokenSpan) {
        let class_suffix = match kind {
            TokenKind::Plain => None,
            TokenKind::Comment => Some("comment"),
            TokenKind::Keyword => Some("keyword"),
            TokenKind::Command | TokenKind::Builtin => Some("entity function"),
            TokenKind::Flag => Some("parameter attribute"),
            TokenKind::Variable => Some("variable"),
            TokenKind::String => Some("string"),
            TokenKind::Number => Some("constant number"),
            TokenKind::Operator => Some("operator"),
            TokenKind::Punctuation => Some("punctuation"),
        };

        if let Some(suffix) = class_suffix {
            self.output.push_str("<span class=\"");
            for part in suffix.split_whitespace() {
                self.output.push_str(&self.prefix);
                self.output.push_str(part);
                self.output.push(' ');
            }
            if self.output.ends_with(' ') {
                self.output.pop();
            }
            self.output.push_str("\">");
            escape_html_into(self.output, text);
            self.output.push_str("</span>");
        } else {
            escape_html_into(self.output, text);
        }
    }

    fn emit_newline(&mut self) {
        self.output.push('\n');
    }
}

/// Emits ANSI color codes for terminal display.
pub struct AnsiColorEmitter<'a> {
    output: &'a mut String,
    line_numbers: bool,
    current_line: usize,
    total_lines: usize,
}

impl<'a> AnsiColorEmitter<'a> {
    pub fn new(output: &'a mut String, line_numbers: bool, total_lines: usize) -> Self {
        Self {
            output,
            line_numbers,
            current_line: 0,
            total_lines,
        }
    }

    fn start_line_if_needed(&mut self) {
        if self.line_numbers && (self.output.is_empty() || self.output.ends_with('\n')) {
            let width = self.total_lines.to_string().len();
            self.current_line += 1;
            self.output.push_str(&format!(
                "\x1b[90m{:>width$} │ \x1b[0m",
                self.current_line,
                width = width
            ));
        }
    }
}

impl<'a> SyntaxEmitter<'a> for AnsiColorEmitter<'_> {
    fn emit(&mut self, kind: TokenKind, text: &'a str, _span: TokenSpan) {
        self.start_line_if_needed();

        let color = match kind {
            TokenKind::Plain => None,
            TokenKind::Comment => Some("\x1b[90m"),
            TokenKind::Keyword => Some("\x1b[1;35m"),
            TokenKind::Command | TokenKind::Builtin => Some("\x1b[1;34m"),
            TokenKind::Flag => Some("\x1b[33m"),
            TokenKind::Variable => Some("\x1b[36m"),
            TokenKind::String => Some("\x1b[32m"),
            TokenKind::Number => Some("\x1b[33m"),
            TokenKind::Operator => Some("\x1b[35m"),
            TokenKind::Punctuation => Some("\x1b[37m"),
        };

        if let Some(c) = color {
            self.output.push_str(c);
            self.output.push_str(text);
            self.output.push_str("\x1b[0m");
        } else {
            self.output.push_str(text);
        }
    }

    fn emit_newline(&mut self) {
        self.output.push('\n');
    }
}

/// Emits delta-encoded LSP 5-tuples directly into a `Vec<u32>`.
pub struct LspSemanticTokensEmitter<'a> {
    data: &'a mut Vec<u32>,
    prev_line: usize,
    prev_col_utf16: usize,
}

impl<'a> LspSemanticTokensEmitter<'a> {
    pub fn new(data: &'a mut Vec<u32>) -> Self {
        Self {
            data,
            prev_line: 0,
            prev_col_utf16: 0,
        }
    }
}

impl<'a> SyntaxEmitter<'a> for LspSemanticTokensEmitter<'_> {
    fn emit(&mut self, kind: TokenKind, _text: &'a str, span: TokenSpan) {
        let token_type = match kind {
            TokenKind::Keyword => 0,
            TokenKind::Command | TokenKind::Builtin => 1,
            TokenKind::Variable => 2,
            TokenKind::String => 3,
            TokenKind::Number => 4,
            TokenKind::Operator => 5,
            TokenKind::Comment => 6,
            TokenKind::Flag => 7,
            TokenKind::Plain | TokenKind::Punctuation => return,
        };

        let delta_line = span.line - self.prev_line;
        let delta_col = if delta_line == 0 {
            span.col_utf16.saturating_sub(self.prev_col_utf16)
        } else {
            span.col_utf16
        };

        let length = span.length_utf16;
        let token_modifiers = 0u32;

        self.data.push(delta_line as u32);
        self.data.push(delta_col as u32);
        self.data.push(length as u32);
        self.data.push(token_type);
        self.data.push(token_modifiers);

        self.prev_line = span.line;
        self.prev_col_utf16 = span.col_utf16;
    }
}

/// Collects `SyntaxToken` structs into a `Vec`.
pub struct TokenCollector<'a> {
    pub tokens: Vec<SyntaxToken<'a>>,
}

impl<'a> Default for TokenCollector<'a> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> TokenCollector<'a> {
    pub fn new() -> Self {
        Self { tokens: Vec::new() }
    }
}

impl<'a> SyntaxEmitter<'a> for TokenCollector<'a> {
    fn emit(&mut self, kind: TokenKind, text: &'a str, span: TokenSpan) {
        self.tokens.push(SyntaxToken::new(kind, text, span));
    }
}

fn escape_html_into(output: &mut String, text: &str) {
    for c in text.chars() {
        match c {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&#39;"),
            _ => output.push(c),
        }
    }
}
