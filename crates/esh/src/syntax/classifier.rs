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

use crate::catalog::{find_builtin, find_keyword};
use crate::lexer::cursor::Lexer;
use crate::lexer::lut::{is_digit, is_hex_digit, is_ident_continue, is_whitespace};
use crate::lexer::token::{Span, TokenKind};
use crate::syntax::emitter::SyntaxEmitter;
use crate::syntax::lut::TEST_OPERATORS;
use crate::syntax::token::{SyntaxCategory, TokenSpan};

pub struct SemanticClassifier<'a> {
    source: &'a str,
    base_line: usize,
}

impl<'a> SemanticClassifier<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            base_line: 0,
        }
    }

    pub fn new_for_line(line: &'a str, line_idx: usize) -> Self {
        Self {
            source: line,
            base_line: line_idx,
        }
    }

    pub fn classify_into<E: SyntaxEmitter<'a>>(&mut self, emitter: &mut E) {
        let mut line_starts = vec![0usize];
        for (i, b) in self.source.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(i + 1);
            }
        }

        let mut lexer = Lexer::new_lossless(self.source);
        let mut expecting_command = true;
        let mut inside_test = false;

        while let Ok(tok) = lexer.next_token() {
            if tok.kind == TokenKind::Eof {
                break;
            }

            let span = self.compute_span(&tok.span, &line_starts);

            match tok.kind {
                TokenKind::Whitespace(text) => {
                    emitter.emit(SyntaxCategory::Plain, text, span);
                }
                TokenKind::Comment(text) => {
                    emitter.emit(SyntaxCategory::Comment, text, span);
                }
                TokenKind::Newline => {
                    expecting_command = true;
                    inside_test = false;
                    emitter.emit_newline();
                }
                TokenKind::Semicolon
                | TokenKind::DoubleSemi
                | TokenKind::SemiAnd
                | TokenKind::DoubleSemiAnd => {
                    expecting_command = true;
                    let text = tok.slice(self.source);
                    emitter.emit(SyntaxCategory::Operator, text, span);
                }
                TokenKind::Pipe
                | TokenKind::ParallelPipe
                | TokenKind::AndIf
                | TokenKind::OrIf
                | TokenKind::Bang => {
                    expecting_command = true;
                    let text = tok.slice(self.source);
                    emitter.emit(SyntaxCategory::Operator, text, span);
                }
                TokenKind::RedirectOut
                | TokenKind::RedirectClobber
                | TokenKind::RedirectAppend
                | TokenKind::RedirectIn
                | TokenKind::RedirectHereDoc
                | TokenKind::RedirectHereDocStrip
                | TokenKind::RedirectHereString
                | TokenKind::RedirectStderr
                | TokenKind::RedirectStderrAppend
                | TokenKind::RedirectStderrToStdout
                | TokenKind::RedirectStdoutToStderr
                | TokenKind::RedirectAll
                | TokenKind::RedirectAllAppend => {
                    let text = tok.slice(self.source);
                    emitter.emit(SyntaxCategory::Operator, text, span);
                }
                TokenKind::If
                | TokenKind::Then
                | TokenKind::Elif
                | TokenKind::Else
                | TokenKind::Fi
                | TokenKind::For
                | TokenKind::In
                | TokenKind::While
                | TokenKind::Until
                | TokenKind::Do
                | TokenKind::Done
                | TokenKind::Case
                | TokenKind::Esac
                | TokenKind::Select => {
                    expecting_command = true;
                    let text = tok.slice(self.source);
                    emitter.emit(SyntaxCategory::Keyword, text, span);
                }
                TokenKind::LBrace | TokenKind::LParen => {
                    expecting_command = true;
                    let text = tok.slice(self.source);
                    emitter.emit(SyntaxCategory::Punctuation, text, span);
                }
                TokenKind::RBrace | TokenKind::RParen => {
                    expecting_command = false;
                    let text = tok.slice(self.source);
                    emitter.emit(SyntaxCategory::Punctuation, text, span);
                }
                TokenKind::SingleQuoted(_) => {
                    expecting_command = false;
                    let text = tok.slice(self.source);
                    emitter.emit(SyntaxCategory::String, text, span);
                }
                TokenKind::DoubleQuoted(content) => {
                    expecting_command = false;
                    self.classify_double_quoted(content, &tok.span, &line_starts, emitter);
                }
                TokenKind::Variable(_) | TokenKind::CommandSub(_) => {
                    expecting_command = false;
                    let text = tok.slice(self.source);
                    if text.starts_with("$((") {
                        self.classify_arithmetic(text, &tok.span, &line_starts, emitter);
                    } else {
                        emitter.emit(SyntaxCategory::Variable, text, span);
                    }
                }
                TokenKind::Word(word) => {
                    self.classify_word(
                        word,
                        &tok.span,
                        &line_starts,
                        &mut expecting_command,
                        &mut inside_test,
                        emitter,
                    );
                }
                TokenKind::HereDocDelimiter { .. } => {
                    expecting_command = false;
                    emitter.emit(SyntaxCategory::String, tok.slice(self.source), span);
                }
                TokenKind::Eof => unreachable!(),
            }
        }
    }

    fn compute_span(&self, span: &Span, line_starts: &[usize]) -> TokenSpan {
        let offset = span.start as usize;
        let line_idx = match line_starts.binary_search(&offset) {
            Ok(idx) => idx,
            Err(idx) => idx.saturating_sub(1),
        };
        let line_start = line_starts[line_idx];
        let col = offset - line_start;
        let line_prefix = &self.source[line_start..offset];
        let col_utf16 = line_prefix.encode_utf16().count();
        let tok_text = &self.source[offset..(span.end as usize).min(self.source.len())];
        let length_utf16 = tok_text.encode_utf16().count();

        TokenSpan {
            line: line_idx + self.base_line,
            col,
            col_utf16,
            length_utf16,
        }
    }

    fn classify_word<E: SyntaxEmitter<'a>>(
        &self,
        word: &'a str,
        span: &Span,
        line_starts: &[usize],
        expecting_command: &mut bool,
        inside_test: &mut bool,
        emitter: &mut E,
    ) {
        // 1. Arithmetic Expansion: `$(( ... ))`
        if word.starts_with("$((") {
            self.classify_arithmetic(word, span, line_starts, emitter);
            *expecting_command = false;
            return;
        }

        // 2. Parameter Expansion: `${ ... }`
        if word.starts_with("${") && word.ends_with('}') {
            let tok_span = self.compute_span(span, line_starts);
            emitter.emit(SyntaxCategory::Variable, word, tok_span);
            *expecting_command = false;
            return;
        }

        // 3. POSIX Test condition brackets: `[` vs `]`
        if word == "[" {
            let tok_span = self.compute_span(span, line_starts);
            if *expecting_command {
                *expecting_command = false;
                *inside_test = true;
                emitter.emit(SyntaxCategory::Builtin, word, tok_span);
            } else {
                emitter.emit(SyntaxCategory::Punctuation, word, tok_span);
            }
            return;
        }

        if word == "]" {
            let tok_span = self.compute_span(span, line_starts);
            *inside_test = false;
            emitter.emit(SyntaxCategory::Punctuation, word, tok_span);
            return;
        }

        // 4. Test operators inside test brackets: `-eq`, `-lt`, `-f`, etc.
        if *inside_test && TEST_OPERATORS.contains(word) {
            let tok_span = self.compute_span(span, line_starts);
            emitter.emit(SyntaxCategory::Operator, word, tok_span);
            return;
        }

        // 5. Assignment & Key-Value Classification (e.g. FOO=bar, COUNT=0, --target=release)
        if let Some(eq_idx) = word.find('=')
            && eq_idx > 0
        {
            let lhs = &word[..eq_idx];
            let eq = &word[eq_idx..eq_idx + 1];
            let rhs = &word[eq_idx + 1..];

            let lhs_span = Span::new(
                span.start,
                span.start + eq_idx as u32,
                span.line,
                span.column,
            );
            let eq_span = Span::new(
                span.start + eq_idx as u32,
                span.start + eq_idx as u32 + 1,
                span.line,
                span.column + eq_idx as u32,
            );
            let rhs_span = Span::new(
                span.start + eq_idx as u32 + 1,
                span.end,
                span.line,
                span.column + eq_idx as u32 + 1,
            );

            // Case 1: Variable Assignment (e.g. FOO=bar, COUNT=0, declare FOO=bar)
            if is_valid_ident(lhs) {
                emitter.emit(
                    SyntaxCategory::Variable,
                    lhs,
                    self.compute_span(&lhs_span, line_starts),
                );
                emitter.emit(
                    SyntaxCategory::Operator,
                    eq,
                    self.compute_span(&eq_span, line_starts),
                );

                if !rhs.is_empty() {
                    let rhs_tok_span = self.compute_span(&rhs_span, line_starts);
                    if is_number_literal(rhs) {
                        emitter.emit(SyntaxCategory::Number, rhs, rhs_tok_span);
                    } else {
                        emitter.emit(SyntaxCategory::Plain, rhs, rhs_tok_span);
                    }
                }

                // CRITICAL: If this assignment was in command position (leading env var prefix),
                // we KEEP expecting_command = true so the next word is treated as the executable command!
                return;
            }

            // Case 2: Flag Key-Value Argument (e.g. --target=release, -o=output.bin)
            if is_flag_name(lhs) {
                emitter.emit(
                    SyntaxCategory::Flag,
                    lhs,
                    self.compute_span(&lhs_span, line_starts),
                );
                emitter.emit(
                    SyntaxCategory::Operator,
                    eq,
                    self.compute_span(&eq_span, line_starts),
                );

                if !rhs.is_empty() {
                    let rhs_tok_span = self.compute_span(&rhs_span, line_starts);
                    if is_number_literal(rhs) {
                        emitter.emit(SyntaxCategory::Number, rhs, rhs_tok_span);
                    } else {
                        emitter.emit(SyntaxCategory::Plain, rhs, rhs_tok_span);
                    }
                }

                *expecting_command = false;
                return;
            }
        }

        // 6. CLI Flags / Options: `-r`, `--lib`, etc.
        if word.starts_with('-') && word.len() > 1 {
            let tok_span = self.compute_span(span, line_starts);
            *expecting_command = false;
            emitter.emit(SyntaxCategory::Flag, word, tok_span);
            return;
        }

        // 7. Command Position Classification
        let tok_span = self.compute_span(span, line_starts);
        if *expecting_command {
            *expecting_command = false;
            if word == "function" {
                *expecting_command = true;
                emitter.emit(SyntaxCategory::Keyword, word, tok_span);
            } else if find_keyword(word).is_some() {
                emitter.emit(SyntaxCategory::Keyword, word, tok_span);
            } else if find_builtin(word).is_some() {
                emitter.emit(SyntaxCategory::Builtin, word, tok_span);
            } else {
                emitter.emit(SyntaxCategory::Command, word, tok_span);
            }
            return;
        }

        // 8. Argument Position Classification
        if is_number_literal(word) {
            emitter.emit(SyntaxCategory::Number, word, tok_span);
        } else {
            emitter.emit(SyntaxCategory::Plain, word, tok_span);
        }
    }

    fn classify_double_quoted<E: SyntaxEmitter<'a>>(
        &self,
        _content: &'a str,
        span: &Span,
        line_starts: &[usize],
        emitter: &mut E,
    ) {
        let full_text =
            &self.source[span.start as usize..(span.end as usize).min(self.source.len())];
        if full_text.is_empty() {
            return;
        }

        let bytes = full_text.as_bytes();
        let mut i = 0;

        while i < bytes.len() {
            let start = i;
            let b = bytes[i];

            if b == b'"' {
                let sub_span = Span::new(
                    span.start + i as u32,
                    span.start + i as u32 + 1,
                    span.line,
                    span.column + i as u32,
                );
                emitter.emit(
                    SyntaxCategory::String,
                    "\"",
                    self.compute_span(&sub_span, line_starts),
                );
                i += 1;
                continue;
            }

            if b == b'$' {
                // Arithmetic in double quotes: `$(( ... ))`
                if i + 2 < bytes.len() && bytes[i + 1] == b'(' && bytes[i + 2] == b'(' {
                    let arith_start = i;
                    i += 3;
                    let mut depth = 1;
                    while i < bytes.len() && depth > 0 {
                        if bytes[i] == b'(' && i + 1 < bytes.len() && bytes[i + 1] == b'(' {
                            depth += 1;
                            i += 2;
                        } else if bytes[i] == b')' && i + 1 < bytes.len() && bytes[i + 1] == b')' {
                            depth -= 1;
                            i += 2;
                        } else {
                            i += 1;
                        }
                    }
                    let arith_str = &full_text[arith_start..i];
                    let arith_span = Span::new(
                        span.start + arith_start as u32,
                        span.start + i as u32,
                        span.line,
                        span.column + arith_start as u32,
                    );
                    self.classify_arithmetic(arith_str, &arith_span, line_starts, emitter);
                    continue;
                }

                // Variable in double quotes: `$VAR` or `${VAR}`
                let var_start = i;
                i += 1;
                if i < bytes.len() && bytes[i] == b'{' {
                    while i < bytes.len() && bytes[i] != b'}' {
                        i += 1;
                    }
                    if i < bytes.len() {
                        i += 1;
                    }
                } else {
                    while i < bytes.len() && is_ident_continue(bytes[i]) {
                        i += 1;
                    }
                }
                let var_str = &full_text[var_start..i];
                let var_span = Span::new(
                    span.start + var_start as u32,
                    span.start + i as u32,
                    span.line,
                    span.column + var_start as u32,
                );
                emitter.emit(
                    SyntaxCategory::Variable,
                    var_str,
                    self.compute_span(&var_span, line_starts),
                );
                continue;
            }

            // Normal string characters up to `$` or `"`
            while i < bytes.len() && bytes[i] != b'"' && bytes[i] != b'$' {
                if bytes[i] == b'\\' && i + 1 < bytes.len() {
                    i += 2;
                } else {
                    i += 1;
                }
            }
            if i > start {
                let str_chunk = &full_text[start..i];
                let chunk_span = Span::new(
                    span.start + start as u32,
                    span.start + i as u32,
                    span.line,
                    span.column + start as u32,
                );
                emitter.emit(
                    SyntaxCategory::String,
                    str_chunk,
                    self.compute_span(&chunk_span, line_starts),
                );
            }
        }
    }

    fn classify_arithmetic<E: SyntaxEmitter<'a>>(
        &self,
        text: &'a str,
        span: &Span,
        line_starts: &[usize],
        emitter: &mut E,
    ) {
        let bytes = text.as_bytes();
        let mut i = 0;

        if text.starts_with("$((") {
            let open_span = Span::new(span.start, span.start + 3, span.line, span.column);
            emitter.emit(
                SyntaxCategory::Operator,
                "$((",
                self.compute_span(&open_span, line_starts),
            );
            i += 3;
        }

        let end_limit = if text.ends_with("))") {
            bytes.len().saturating_sub(2)
        } else {
            bytes.len()
        };

        while i < end_limit {
            let b = bytes[i];
            let start = i;

            if is_whitespace(b) {
                while i < end_limit && is_whitespace(bytes[i]) {
                    i += 1;
                }
                let ws = &text[start..i];
                let ws_span = Span::new(
                    span.start + start as u32,
                    span.start + i as u32,
                    span.line,
                    span.column + start as u32,
                );
                emitter.emit(
                    SyntaxCategory::Plain,
                    ws,
                    self.compute_span(&ws_span, line_starts),
                );
                continue;
            }

            // Numbers: decimal, hex (`0x`), binary (`0b`), octal (`0o`)
            if is_digit(b) {
                if b == b'0'
                    && i + 1 < end_limit
                    && matches!(bytes[i + 1], b'x' | b'X' | b'b' | b'B' | b'o' | b'O')
                {
                    i += 2;
                    while i < end_limit && (is_hex_digit(bytes[i]) || bytes[i] == b'_') {
                        i += 1;
                    }
                } else {
                    while i < end_limit && (is_digit(bytes[i]) || bytes[i] == b'_') {
                        i += 1;
                    }
                }
                let num_str = &text[start..i];
                let num_span = Span::new(
                    span.start + start as u32,
                    span.start + i as u32,
                    span.line,
                    span.column + start as u32,
                );
                emitter.emit(
                    SyntaxCategory::Number,
                    num_str,
                    self.compute_span(&num_span, line_starts),
                );
                continue;
            }

            // Identifiers / Variables
            if (b as char).is_ascii_alphabetic() || b == b'_' || b == b'$' {
                if b == b'$' && i + 1 < end_limit {
                    i += 1;
                }
                while i < end_limit && is_ident_continue(bytes[i]) {
                    i += 1;
                }
                let var_str = &text[start..i];
                let var_span = Span::new(
                    span.start + start as u32,
                    span.start + i as u32,
                    span.line,
                    span.column + start as u32,
                );
                emitter.emit(
                    SyntaxCategory::Variable,
                    var_str,
                    self.compute_span(&var_span, line_starts),
                );
                continue;
            }

            // Operators: `+`, `-`, `*`, `/`, `%`, `++`, `--`, `+=`, `-=`, `==`, `!=`, `<`, `>`, `<=`, `>=`, `?`, `:`, `&`, `|`, `^`, `~`
            if matches!(
                b,
                b'+' | b'-'
                    | b'*'
                    | b'/'
                    | b'%'
                    | b'='
                    | b'!'
                    | b'<'
                    | b'>'
                    | b'?'
                    | b':'
                    | b'&'
                    | b'|'
                    | b'^'
                    | b'~'
            ) {
                if i + 1 < end_limit
                    && matches!(
                        &bytes[start..start + 2],
                        b"++"
                            | b"--"
                            | b"+="
                            | b"-="
                            | b"*="
                            | b"/="
                            | b"%="
                            | b"=="
                            | b"!="
                            | b"<="
                            | b">="
                            | b"&&"
                            | b"||"
                            | b"<<"
                            | b">>"
                    )
                {
                    i += 2;
                } else {
                    i += 1;
                }
                let op_str = &text[start..i];
                let op_span = Span::new(
                    span.start + start as u32,
                    span.start + i as u32,
                    span.line,
                    span.column + start as u32,
                );
                emitter.emit(
                    SyntaxCategory::Operator,
                    op_str,
                    self.compute_span(&op_span, line_starts),
                );
                continue;
            }

            // Punctuation: `(`, `)`
            if matches!(b, b'(' | b')') {
                i += 1;
                let punc_str = &text[start..i];
                let punc_span = Span::new(
                    span.start + start as u32,
                    span.start + i as u32,
                    span.line,
                    span.column + start as u32,
                );
                emitter.emit(
                    SyntaxCategory::Punctuation,
                    punc_str,
                    self.compute_span(&punc_span, line_starts),
                );
                continue;
            }

            i += 1;
            let other_str = &text[start..i];
            let other_span = Span::new(
                span.start + start as u32,
                span.start + i as u32,
                span.line,
                span.column + start as u32,
            );
            emitter.emit(
                SyntaxCategory::Plain,
                other_str,
                self.compute_span(&other_span, line_starts),
            );
        }

        if text.ends_with("))") {
            let close_span = Span::new(
                span.start + end_limit as u32,
                span.end,
                span.line,
                span.column + end_limit as u32,
            );
            emitter.emit(
                SyntaxCategory::Operator,
                "))",
                self.compute_span(&close_span, line_starts),
            );
        }
    }
}

fn is_number_literal(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    if bytes.starts_with(b"0x") || bytes.starts_with(b"0X") {
        bytes.len() > 2 && bytes[2..].iter().all(|b| is_hex_digit(*b) || *b == b'_')
    } else if bytes.starts_with(b"0b") || bytes.starts_with(b"0B") {
        bytes.len() > 2 && bytes[2..].iter().all(|b| matches!(b, b'0' | b'1' | b'_'))
    } else if bytes.starts_with(b"0o") || bytes.starts_with(b"0O") {
        bytes.len() > 2 && bytes[2..].iter().all(|b| matches!(b, b'0'..=b'7' | b'_'))
    } else {
        bytes.iter().all(|b| is_digit(*b) || *b == b'_')
    }
}

#[inline(always)]
fn is_valid_ident(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[inline(always)]
fn is_flag_name(s: &str) -> bool {
    s.starts_with('-') && s.len() > 1 && !s.chars().skip(1).all(|c| c == '-')
}
