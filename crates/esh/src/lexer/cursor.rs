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

use crate::error::LexError;
use crate::lexer::lut::{CHAR_WHITESPACE, is_char_kind, is_ident_continue, is_ident_start};
use crate::lexer::token::{Span, Token, TokenKind};

pub struct Lexer<'a> {
    source: &'a str,
    bytes: &'a [u8],
    offset: usize,
    line: u32,
    col: u32,
    lossless: bool,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Self {
        Self::new_for_parser(source)
    }

    pub fn new_for_parser(source: &'a str) -> Self {
        Self {
            source,
            bytes: source.as_bytes(),
            offset: 0,
            line: 1,
            col: 1,
            lossless: false,
        }
    }

    pub fn new_lossless(source: &'a str) -> Self {
        Self {
            source,
            bytes: source.as_bytes(),
            offset: 0,
            line: 1,
            col: 1,
            lossless: true,
        }
    }

    pub fn tokenize_all(mut self) -> Result<Vec<Token<'a>>, LexError> {
        let mut tokens: Vec<Token<'a>> = Vec::new();
        let mut pending_heredocs: Vec<(usize, String, bool, bool)> = Vec::new();
        let mut expects_heredoc_delimiter = false;
        let mut delimiter_builder: Option<(usize, String, bool, u32)> = None;
        loop {
            let token = self.next_token()?;
            let is_eof = token.kind == TokenKind::Eof;
            let mut consume_as_delimiter_fragment = false;

            if let Some((_, raw, _, last_end)) = delimiter_builder.as_mut() {
                if is_heredoc_delimiter_word(token.kind) && token.span.start == *last_end {
                    raw.push_str(token.slice(self.source));
                    *last_end = token.span.end;
                    consume_as_delimiter_fragment = true;
                } else {
                    let (index, raw, strip_tabs, _) = delimiter_builder.take().unwrap();
                    let (delimiter, quoted) = parse_heredoc_delimiter(&raw);
                    tokens[index].kind = TokenKind::HereDocDelimiter {
                        body: "",
                        quoted,
                        strip_tabs,
                    };
                    pending_heredocs.push((index, delimiter, quoted, strip_tabs));
                }
            }

            if consume_as_delimiter_fragment {
                // The fragments together form one shell word. The first token carries
                // the combined delimiter; later fragments must not reach the parser.
            } else {
                tokens.push(token);
            }

            let index = tokens.len() - 1;
            if expects_heredoc_delimiter {
                if !is_heredoc_delimiter_word(token.kind) {
                    return Err(LexError {
                        message: "Expected a here-document delimiter after <<".to_string(),
                        span: token.span,
                    });
                }
                let strip_tabs = index > 0
                    && tokens
                        .get(index - 1)
                        .is_some_and(|previous| previous.kind == TokenKind::RedirectHereDocStrip);
                delimiter_builder = Some((
                    index,
                    token.slice(self.source).to_string(),
                    strip_tabs,
                    token.span.end,
                ));
                expects_heredoc_delimiter = false;
            } else if !consume_as_delimiter_fragment {
                expects_heredoc_delimiter = matches!(
                    token.kind,
                    TokenKind::RedirectHereDoc | TokenKind::RedirectHereDocStrip
                );
            }

            if token.kind == TokenKind::Newline
                && let Some((index, raw, strip_tabs, _)) = delimiter_builder.take()
            {
                let (delimiter, quoted) = parse_heredoc_delimiter(&raw);
                tokens[index].kind = TokenKind::HereDocDelimiter {
                    body: "",
                    quoted,
                    strip_tabs,
                };
                pending_heredocs.push((index, delimiter, quoted, strip_tabs));
            }

            if token.kind == TokenKind::Newline && !pending_heredocs.is_empty() {
                for (index, delimiter, quoted, strip_tabs) in pending_heredocs.drain(..) {
                    let body =
                        self.collect_heredoc_body(&delimiter, quoted, strip_tabs, token.span)?;
                    tokens[index].kind = TokenKind::HereDocDelimiter {
                        body,
                        quoted,
                        strip_tabs,
                    };
                }
            }

            if is_eof {
                if !pending_heredocs.is_empty() || expects_heredoc_delimiter {
                    return Err(LexError {
                        message: "Unterminated here-document".to_string(),
                        span: token.span,
                    });
                }
                break;
            }
        }
        Ok(tokens)
    }

    fn collect_heredoc_body(
        &mut self,
        delimiter: &str,
        quoted: bool,
        strip_tabs: bool,
        span: Span,
    ) -> Result<&'a str, LexError> {
        let body_start = self.offset;
        let mut continued = false;
        loop {
            if self.is_eof() {
                return Err(LexError {
                    message: format!("Unterminated here-document; expected '{}'", delimiter),
                    span,
                });
            }

            let line_start = self.offset;
            while !self.is_eof() && self.peek_byte() != Some(b'\n') {
                self.advance();
            }
            let line_end = self.offset;
            let line = self.source[line_start..line_end].trim_end_matches('\r');
            let candidate = if strip_tabs {
                line.trim_start_matches('\t')
            } else {
                line
            };

            if !continued && candidate == delimiter {
                let body = &self.source[body_start..line_start];
                if self.peek_byte() == Some(b'\n') {
                    self.advance();
                }
                return Ok(body);
            }

            continued = !quoted && line_has_unescaped_trailing_backslash(line);

            if self.peek_byte() == Some(b'\n') {
                self.advance();
            }
        }
    }

    #[inline(always)]
    fn is_eof(&self) -> bool {
        self.offset >= self.bytes.len()
    }

    #[inline(always)]
    fn peek_byte(&self) -> Option<u8> {
        if self.is_eof() {
            None
        } else {
            Some(self.bytes[self.offset])
        }
    }

    #[inline(always)]
    fn peek_byte_at(&self, delta: usize) -> Option<u8> {
        let pos = self.offset + delta;
        if pos < self.bytes.len() {
            Some(self.bytes[pos])
        } else {
            None
        }
    }

    fn advance(&mut self) -> Option<u8> {
        if self.is_eof() {
            return None;
        }
        let b = self.bytes[self.offset];

        self.offset += 1;
        if b == b'\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(b)
    }

    fn current_span_start(&self) -> (usize, u32, u32) {
        (self.offset, self.line, self.col)
    }

    fn make_span(&self, start_offset: usize, start_line: u32, start_col: u32) -> Span {
        Span::new(
            start_offset as u32,
            self.offset as u32,
            start_line,
            start_col,
        )
    }

    #[inline(always)]
    fn skip_whitespace_and_comments(&mut self) {
        while self.offset < self.bytes.len() {
            let b = self.bytes[self.offset];
            if is_char_kind(b, CHAR_WHITESPACE) {
                self.offset += 1;
                self.col += 1;
                continue;
            }
            if b == b'#' {
                while self.offset < self.bytes.len() && self.bytes[self.offset] != b'\n' {
                    self.offset += 1;
                    self.col += 1;
                }
                continue;
            }
            if b == b'\\' {
                if self.peek_byte_at(1) == Some(b'\n') {
                    self.offset += 2;
                    self.line += 1;
                    self.col = 1;
                    continue;
                }
                if self.peek_byte_at(1) == Some(b'\r') && self.peek_byte_at(2) == Some(b'\n') {
                    self.offset += 3;
                    self.line += 1;
                    self.col = 1;
                    continue;
                }
            }
            break;
        }
    }

    pub fn next_token(&mut self) -> Result<Token<'a>, LexError> {
        if self.lossless {
            if self.is_eof() {
                let (start_offset, start_line, start_col) = self.current_span_start();
                return Ok(Token::new(
                    TokenKind::Eof,
                    self.make_span(start_offset, start_line, start_col),
                ));
            }

            let b = self.bytes[self.offset];

            // 1. Line comment: `#`
            if b == b'#' {
                let (start_offset, start_line, start_col) = self.current_span_start();
                while !self.is_eof() && self.peek_byte() != Some(b'\n') {
                    self.advance();
                }
                let text = &self.source[start_offset..self.offset];
                return Ok(Token::new(
                    TokenKind::Comment(text),
                    self.make_span(start_offset, start_line, start_col),
                ));
            }

            // 2. Horizontal whitespace & line continuation
            let is_line_cont = (b == b'\\' && self.peek_byte_at(1) == Some(b'\n'))
                || (b == b'\\'
                    && self.peek_byte_at(1) == Some(b'\r')
                    && self.peek_byte_at(2) == Some(b'\n'));
            if is_char_kind(b, CHAR_WHITESPACE) || is_line_cont {
                let (start_offset, start_line, start_col) = self.current_span_start();
                while !self.is_eof() {
                    let curr = self.bytes[self.offset];
                    if curr == b'\\' && self.peek_byte_at(1) == Some(b'\n') {
                        self.advance();
                        self.advance();
                    } else if curr == b'\\'
                        && self.peek_byte_at(1) == Some(b'\r')
                        && self.peek_byte_at(2) == Some(b'\n')
                    {
                        self.advance();
                        self.advance();
                        self.advance();
                    } else if is_char_kind(curr, CHAR_WHITESPACE) {
                        self.advance();
                    } else {
                        break;
                    }
                }
                let text = &self.source[start_offset..self.offset];
                return Ok(Token::new(
                    TokenKind::Whitespace(text),
                    self.make_span(start_offset, start_line, start_col),
                ));
            }
        } else {
            self.skip_whitespace_and_comments();
        }

        let (start_offset, start_line, start_col) = self.current_span_start();

        if self.is_eof() {
            return Ok(Token::new(
                TokenKind::Eof,
                self.make_span(start_offset, start_line, start_col),
            ));
        }

        let b = self.bytes[self.offset];

        // ESH has a deliberately small standard-stream redirection model.
        // Reject numeric descriptor prefixes that would otherwise tokenize
        // as an ordinary numeric argument followed by a different redirect.
        if b.is_ascii_digit() {
            let mut descriptor_end = self.offset;
            while self
                .bytes
                .get(descriptor_end)
                .is_some_and(u8::is_ascii_digit)
            {
                descriptor_end += 1;
            }
            let descriptor = &self.source[self.offset..descriptor_end];
            let next = self.bytes.get(descriptor_end).copied();
            if matches!(next, Some(b'<' | b'>')) {
                let after_operator = self.bytes.get(descriptor_end + 1).copied();
                let supported_stderr = descriptor == "2"
                    && next == Some(b'>')
                    && match after_operator {
                        Some(b'>') => true,
                        Some(b'&') => self.bytes.get(descriptor_end + 2) == Some(&b'1'),
                        Some(b'|') => false,
                        _ => true,
                    };
                let supported = (descriptor == "0" && next == Some(b'<'))
                    || (descriptor == "1"
                        && next == Some(b'>')
                        && (after_operator != Some(b'&')
                            || self.bytes.get(descriptor_end + 2) == Some(&b'2')))
                    || supported_stderr
                    || (descriptor == "1"
                        && next == Some(b'>')
                        && self.bytes.get(descriptor_end + 1) == Some(&b'&')
                        && self.bytes.get(descriptor_end + 2) == Some(&b'2'));
                if !supported {
                    return Err(LexError {
                        message: "ESH supports standard-stream redirections only; this file descriptor form is not supported".to_string(),
                        span: self.make_span(start_offset, start_line, start_col),
                    });
                }
            }
        }

        // Terminators
        if b == b'\n' {
            self.advance();
            return Ok(Token::new(
                TokenKind::Newline,
                self.make_span(start_offset, start_line, start_col),
            ));
        }
        if b == b';' {
            self.advance();
            if self.peek_byte() == Some(b';') {
                self.advance();
                if self.peek_byte() == Some(b'&') {
                    self.advance();
                    return Ok(Token::new(
                        TokenKind::DoubleSemiAnd,
                        self.make_span(start_offset, start_line, start_col),
                    ));
                }
                return Ok(Token::new(
                    TokenKind::DoubleSemi,
                    self.make_span(start_offset, start_line, start_col),
                ));
            }
            if self.peek_byte() == Some(b'&') {
                self.advance();
                return Ok(Token::new(
                    TokenKind::SemiAnd,
                    self.make_span(start_offset, start_line, start_col),
                ));
            }
            return Ok(Token::new(
                TokenKind::Semicolon,
                self.make_span(start_offset, start_line, start_col),
            ));
        }

        // Logical and Parallel operators
        if b == b'&' {
            self.advance();
            if self.peek_byte() == Some(b'&') {
                self.advance();
                return Ok(Token::new(
                    TokenKind::AndIf,
                    self.make_span(start_offset, start_line, start_col),
                ));
            }
            if self.peek_byte() == Some(b'|') {
                self.advance();
                return Ok(Token::new(
                    TokenKind::ParallelPipe,
                    self.make_span(start_offset, start_line, start_col),
                ));
            }
            if self.peek_byte() == Some(b'>') {
                self.advance();
                if self.peek_byte() == Some(b'>') {
                    self.advance();
                    return Ok(Token::new(
                        TokenKind::RedirectAllAppend,
                        self.make_span(start_offset, start_line, start_col),
                    ));
                }
                return Ok(Token::new(
                    TokenKind::RedirectAll,
                    self.make_span(start_offset, start_line, start_col),
                ));
            }
            return Err(LexError {
                message: "Unexpected single '&' operator".to_string(),
                span: self.make_span(start_offset, start_line, start_col),
            });
        }

        if b == b'|' {
            self.advance();
            if self.peek_byte() == Some(b'|') {
                self.advance();
                return Ok(Token::new(
                    TokenKind::OrIf,
                    self.make_span(start_offset, start_line, start_col),
                ));
            }
            return Ok(Token::new(
                TokenKind::Pipe,
                self.make_span(start_offset, start_line, start_col),
            ));
        }

        // Grouping Delimiters
        if b == b'{' {
            self.advance();
            return Ok(Token::new(
                TokenKind::LBrace,
                self.make_span(start_offset, start_line, start_col),
            ));
        }
        if b == b'}' {
            self.advance();
            return Ok(Token::new(
                TokenKind::RBrace,
                self.make_span(start_offset, start_line, start_col),
            ));
        }
        if b == b'(' {
            self.advance();
            return Ok(Token::new(
                TokenKind::LParen,
                self.make_span(start_offset, start_line, start_col),
            ));
        }
        if b == b')' {
            self.advance();
            return Ok(Token::new(
                TokenKind::RParen,
                self.make_span(start_offset, start_line, start_col),
            ));
        }

        // Redirections
        if b == b'>' {
            if self.peek_byte_at(1) == Some(b'&') && self.peek_byte_at(2) == Some(b'2') {
                self.advance(); // consume '>'
                self.advance(); // consume '&'
                self.advance(); // consume '2'
                return Ok(Token::new(
                    TokenKind::RedirectStdoutToStderr,
                    self.make_span(start_offset, start_line, start_col),
                ));
            }
            self.advance();
            if self.peek_byte() == Some(b'>') {
                self.advance();
                return Ok(Token::new(
                    TokenKind::RedirectAppend,
                    self.make_span(start_offset, start_line, start_col),
                ));
            }
            if self.peek_byte() == Some(b'|') {
                self.advance();
                return Ok(Token::new(
                    TokenKind::RedirectClobber,
                    self.make_span(start_offset, start_line, start_col),
                ));
            }
            return Ok(Token::new(
                TokenKind::RedirectOut,
                self.make_span(start_offset, start_line, start_col),
            ));
        }
        if b == b'<' {
            if self.peek_byte_at(1) == Some(b'>') {
                return Err(LexError {
                    message: "Read/write redirection '<>' is not supported by ESH".to_string(),
                    span: self.make_span(start_offset, start_line, start_col),
                });
            }
            self.advance();
            if self.peek_byte() == Some(b'<') {
                self.advance();
                if self.peek_byte() == Some(b'<') {
                    self.advance();
                    return Ok(Token::new(
                        TokenKind::RedirectHereString,
                        self.make_span(start_offset, start_line, start_col),
                    ));
                }
                if self.peek_byte() == Some(b'-') {
                    self.advance();
                    return Ok(Token::new(
                        TokenKind::RedirectHereDocStrip,
                        self.make_span(start_offset, start_line, start_col),
                    ));
                }
                return Ok(Token::new(
                    TokenKind::RedirectHereDoc,
                    self.make_span(start_offset, start_line, start_col),
                ));
            }
            return Ok(Token::new(
                TokenKind::RedirectIn,
                self.make_span(start_offset, start_line, start_col),
            ));
        }

        // Logical / Pipeline Negation '!'
        if b == b'!'
            && matches!(
                self.peek_byte_at(1),
                Some(b' ' | b'\t' | b'\n' | b';' | b'|' | b'&') | None
            )
        {
            self.advance();
            return Ok(Token::new(
                TokenKind::Bang,
                self.make_span(start_offset, start_line, start_col),
            ));
        }

        // 2> and 2>> and 2>&1 redirection
        if b == b'2' && self.peek_byte_at(1) == Some(b'>') {
            if self.peek_byte_at(2) == Some(b'&') && self.peek_byte_at(3) == Some(b'1') {
                self.advance(); // consume '2'
                self.advance(); // consume '>'
                self.advance(); // consume '&'
                self.advance(); // consume '1'
                return Ok(Token::new(
                    TokenKind::RedirectStderrToStdout,
                    self.make_span(start_offset, start_line, start_col),
                ));
            }
            self.advance(); // consume '2'
            self.advance(); // consume '>'
            if self.peek_byte() == Some(b'>') {
                self.advance();
                return Ok(Token::new(
                    TokenKind::RedirectStderrAppend,
                    self.make_span(start_offset, start_line, start_col),
                ));
            }
            return Ok(Token::new(
                TokenKind::RedirectStderr,
                self.make_span(start_offset, start_line, start_col),
            ));
        }

        // Standard input/output descriptor aliases: 0<, 1>, 1>>, 1>|.
        if b == b'0' && self.peek_byte_at(1) == Some(b'<') {
            self.advance(); // consume '0'
            self.advance(); // consume '<'
            if self.peek_byte() == Some(b'<') {
                self.advance();
                if self.peek_byte() == Some(b'<') {
                    self.advance();
                    return Ok(Token::new(
                        TokenKind::RedirectHereString,
                        self.make_span(start_offset, start_line, start_col),
                    ));
                }
                if self.peek_byte() == Some(b'-') {
                    self.advance();
                    return Ok(Token::new(
                        TokenKind::RedirectHereDocStrip,
                        self.make_span(start_offset, start_line, start_col),
                    ));
                }
                return Ok(Token::new(
                    TokenKind::RedirectHereDoc,
                    self.make_span(start_offset, start_line, start_col),
                ));
            }
            return Ok(Token::new(
                TokenKind::RedirectIn,
                self.make_span(start_offset, start_line, start_col),
            ));
        }

        if b == b'1' && self.peek_byte_at(1) == Some(b'>') {
            if self.peek_byte_at(2) == Some(b'&') && self.peek_byte_at(3) == Some(b'2') {
                self.advance(); // consume '1'
                self.advance(); // consume '>'
                self.advance(); // consume '&'
                self.advance(); // consume '2'
                return Ok(Token::new(
                    TokenKind::RedirectStdoutToStderr,
                    self.make_span(start_offset, start_line, start_col),
                ));
            }
            self.advance(); // consume '1'
            self.advance(); // consume '>'
            if self.peek_byte() == Some(b'>') {
                self.advance();
                return Ok(Token::new(
                    TokenKind::RedirectAppend,
                    self.make_span(start_offset, start_line, start_col),
                ));
            }
            if self.peek_byte() == Some(b'|') {
                self.advance();
                return Ok(Token::new(
                    TokenKind::RedirectClobber,
                    self.make_span(start_offset, start_line, start_col),
                ));
            }
            return Ok(Token::new(
                TokenKind::RedirectOut,
                self.make_span(start_offset, start_line, start_col),
            ));
        }

        // Single Quoted literal
        if b == b'\'' {
            return self.read_single_quoted(start_offset, start_line, start_col);
        }

        // Double Quoted string
        if b == b'"' {
            return self.read_double_quoted(start_offset, start_line, start_col);
        }

        if b == b'`' {
            return self.read_backtick_substitution(start_offset, start_line, start_col);
        }

        // Variable or Command Substitution
        if b == b'$' {
            return self.read_sigil_token(start_offset, start_line, start_col);
        }

        // Regular Bare Word
        self.read_bare_word(start_offset, start_line, start_col)
    }

    fn read_single_quoted(
        &mut self,
        start_offset: usize,
        start_line: u32,
        start_col: u32,
    ) -> Result<Token<'a>, LexError> {
        self.advance(); // consume opening '\''
        let content_start = self.offset;
        while !self.is_eof() && self.peek_byte() != Some(b'\'') {
            self.advance();
        }
        if self.is_eof() {
            return Err(LexError {
                message: "Unterminated single quote string".to_string(),
                span: self.make_span(start_offset, start_line, start_col),
            });
        }
        let content_end = self.offset;
        self.advance(); // consume closing '\''

        let literal = &self.source[content_start..content_end];
        Ok(Token::new(
            TokenKind::SingleQuoted(literal),
            self.make_span(start_offset, start_line, start_col),
        ))
    }

    fn read_double_quoted(
        &mut self,
        start_offset: usize,
        start_line: u32,
        start_col: u32,
    ) -> Result<Token<'a>, LexError> {
        self.advance(); // consume opening '"'
        let content_start = self.offset;
        while !self.is_eof() {
            let b = self.bytes[self.offset];
            if b == b'\\' {
                self.advance();
                if !self.is_eof() {
                    self.advance();
                }
                continue;
            }
            if b == b'$'
                && self.offset + 1 < self.bytes.len()
                && self.bytes[self.offset + 1] == b'('
            {
                self.advance(); // consume '$'
                self.advance(); // consume '('
                let mut depth = 1;
                while !self.is_eof() && depth > 0 {
                    let c = self.bytes[self.offset];
                    if c == b'\\' {
                        self.advance();
                        if !self.is_eof() {
                            self.advance();
                        }
                        continue;
                    }
                    if c == b'\'' {
                        self.advance();
                        while !self.is_eof() && self.bytes[self.offset] != b'\'' {
                            self.advance();
                        }
                        if !self.is_eof() {
                            self.advance();
                        }
                        continue;
                    }
                    if c == b'"' {
                        self.advance();
                        while !self.is_eof() && self.bytes[self.offset] != b'"' {
                            if self.bytes[self.offset] == b'\\' {
                                self.advance();
                            }
                            if !self.is_eof() {
                                self.advance();
                            }
                        }
                        if !self.is_eof() {
                            self.advance();
                        }
                        continue;
                    }
                    if c == b'(' {
                        depth += 1;
                    } else if c == b')' {
                        depth -= 1;
                    }
                    if depth > 0 {
                        self.advance();
                    }
                }
                if !self.is_eof() && self.bytes[self.offset] == b')' {
                    self.advance();
                }
                continue;
            }
            if b == b'$'
                && self.offset + 1 < self.bytes.len()
                && self.bytes[self.offset + 1] == b'{'
            {
                self.advance(); // consume '$'
                self.advance(); // consume '{'
                let mut depth = 1usize;
                let mut quote = None;
                while !self.is_eof() && depth > 0 {
                    let current = self.bytes[self.offset];
                    if let Some(active_quote) = quote {
                        if current == b'\\' && active_quote == b'"' {
                            self.advance();
                            if !self.is_eof() {
                                self.advance();
                            }
                        } else if current == active_quote {
                            quote = None;
                            self.advance();
                        } else if current == b'$'
                            && self.offset + 1 < self.bytes.len()
                            && self.bytes[self.offset + 1] == b'{'
                        {
                            self.advance();
                            self.advance();
                            depth += 1;
                        } else {
                            self.advance();
                        }
                    } else if current == b'\'' || current == b'"' {
                        quote = Some(current);
                        self.advance();
                    } else if current == b'\\' {
                        self.advance();
                        if !self.is_eof() {
                            self.advance();
                        }
                    } else if current == b'$'
                        && self.offset + 1 < self.bytes.len()
                        && self.bytes[self.offset + 1] == b'{'
                    {
                        self.advance();
                        self.advance();
                        depth += 1;
                    } else if current == b'}' {
                        self.advance();
                        depth -= 1;
                    } else {
                        self.advance();
                    }
                }
                if depth > 0 {
                    return Err(LexError {
                        message: "Unterminated variable reference ${...}".to_string(),
                        span: self.make_span(start_offset, start_line, start_col),
                    });
                }
                continue;
            }
            if b == b'"' {
                break;
            }
            self.advance();
        }

        if self.is_eof() {
            return Err(LexError {
                message: "Unterminated double quote string".to_string(),
                span: self.make_span(start_offset, start_line, start_col),
            });
        }

        let content_end = self.offset;
        self.advance(); // consume closing '"'

        let content = &self.source[content_start..content_end];
        Ok(Token::new(
            TokenKind::DoubleQuoted(content),
            self.make_span(start_offset, start_line, start_col),
        ))
    }

    fn read_sigil_token(
        &mut self,
        start_offset: usize,
        start_line: u32,
        start_col: u32,
    ) -> Result<Token<'a>, LexError> {
        self.advance(); // consume '$'

        if self.is_eof() {
            let span = self.make_span(start_offset, start_line, start_col);
            return Ok(Token::new(TokenKind::Word("$"), span));
        }

        let b = self.bytes[self.offset];

        // Command substitution: $(...)
        if b == b'(' {
            self.advance();
            let cmd_start = self.offset;
            let cmd_end =
                self.scan_command_substitution_body(start_offset, start_line, start_col)?;

            let cmd = &self.source[cmd_start..cmd_end];
            return Ok(Token::new(
                TokenKind::CommandSub(cmd),
                self.make_span(start_offset, start_line, start_col),
            ));
        }

        // Braced variable: ${VAR}
        if b == b'{' {
            self.advance();
            let var_start = self.offset;
            let mut depth = 1usize;
            let mut quote = None;
            while !self.is_eof() && depth > 0 {
                let current = self.peek_byte().expect("not at EOF");
                if let Some(active_quote) = quote {
                    if current == b'\\' && active_quote == b'"' {
                        self.advance();
                        if !self.is_eof() {
                            self.advance();
                        }
                    } else if current == active_quote {
                        quote = None;
                        self.advance();
                    } else if current == b'$' && self.peek_byte_at(1) == Some(b'{') {
                        self.advance();
                        self.advance();
                        depth += 1;
                    } else {
                        self.advance();
                    }
                } else if current == b'\'' || current == b'"' {
                    quote = Some(current);
                    self.advance();
                } else if current == b'\\' {
                    self.advance();
                    if !self.is_eof() {
                        self.advance();
                    }
                } else if current == b'$'
                    && self.offset + 1 < self.bytes.len()
                    && self.bytes[self.offset + 1] == b'{'
                {
                    self.advance();
                    self.advance();
                    depth += 1;
                } else if current == b'}' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    self.advance();
                } else {
                    self.advance();
                }
            }
            if depth > 0 {
                return Err(LexError {
                    message: "Unterminated variable reference ${...}".to_string(),
                    span: self.make_span(start_offset, start_line, start_col),
                });
            }
            let var_end = self.offset;
            self.advance(); // consume '}'
            let var_name = &self.source[var_start..var_end];
            return Ok(Token::new(
                TokenKind::Variable(var_name),
                self.make_span(start_offset, start_line, start_col),
            ));
        }

        // Special variables: $?, $@, $*, $#, $-, $0..$9
        if b == b'?' || b == b'@' || b == b'*' || b == b'#' || b == b'-' || b.is_ascii_digit() {
            let var_start = self.offset;
            self.advance();
            let var_name = &self.source[var_start..self.offset];
            return Ok(Token::new(
                TokenKind::Variable(var_name),
                self.make_span(start_offset, start_line, start_col),
            ));
        }

        // Standard Identifier variable: $VAR
        if is_ident_start(b) {
            let var_start = self.offset;
            while !self.is_eof() {
                let next_b = self.bytes[self.offset];
                if is_ident_continue(next_b) {
                    self.advance();
                } else {
                    break;
                }
            }
            let var_name = &self.source[var_start..self.offset];
            return Ok(Token::new(
                TokenKind::Variable(var_name),
                self.make_span(start_offset, start_line, start_col),
            ));
        }

        // Fallback: literal '$'
        let span = self.make_span(start_offset, start_line, start_col);
        Ok(Token::new(TokenKind::Word("$"), span))
    }

    fn scan_command_substitution_body(
        &mut self,
        start_offset: usize,
        start_line: u32,
        start_col: u32,
    ) -> Result<usize, LexError> {
        let mut depth = 1usize;
        let mut quote = None;
        let mut pending_heredocs: Vec<(String, bool, bool)> = Vec::new();

        while !self.is_eof() {
            let current = self.bytes[self.offset];
            if quote == Some(b'\'') {
                if current == b'\'' {
                    quote = None;
                }
                self.advance();
                continue;
            }

            if quote == Some(b'"') {
                match current {
                    b'"' => {
                        quote = None;
                        self.advance();
                    }
                    b'\\' => {
                        self.advance();
                        if !self.is_eof() {
                            self.advance();
                        }
                    }
                    b'`' => self.skip_backtick_in_command_substitution(
                        start_offset,
                        start_line,
                        start_col,
                    )?,
                    b'$' if self.bytes.get(self.offset + 1) == Some(&b'(') => {
                        let arithmetic = self.bytes.get(self.offset + 2) == Some(&b'(');
                        self.advance(); // $
                        self.advance(); // (
                        if arithmetic {
                            self.advance(); // second ( of $((
                            self.skip_arithmetic_expansion(start_offset, start_line, start_col)?;
                        } else {
                            self.scan_command_substitution_body(
                                start_offset,
                                start_line,
                                start_col,
                            )?;
                        }
                    }
                    _ => {
                        self.advance();
                    }
                }
                continue;
            }

            match current {
                b'$' if self.bytes.get(self.offset + 1) == Some(&b'(')
                    && self.bytes.get(self.offset + 2) == Some(&b'(') =>
                {
                    self.advance();
                    self.advance();
                    self.advance();
                    self.skip_arithmetic_expansion(start_offset, start_line, start_col)?;
                }
                b'\\' => {
                    self.advance();
                    if !self.is_eof() {
                        self.advance();
                    }
                }
                b'\'' | b'"' => {
                    quote = Some(current);
                    self.advance();
                }
                b'`' => {
                    self.skip_backtick_in_command_substitution(start_offset, start_line, start_col)?
                }
                b'#' if self.is_shell_comment_start() => {
                    while !self.is_eof() && self.bytes[self.offset] != b'\n' {
                        self.advance();
                    }
                }
                b'<' if self.bytes.get(self.offset + 1) == Some(&b'<')
                    && self.bytes.get(self.offset + 2) != Some(&b'<') =>
                {
                    self.advance();
                    self.advance();
                    let strip_tabs = self.peek_byte() == Some(b'-');
                    if strip_tabs {
                        self.advance();
                    }
                    if let Some((delimiter, quoted)) =
                        self.scan_heredoc_delimiter_word(start_offset, start_line, start_col)?
                    {
                        pending_heredocs.push((delimiter, quoted, strip_tabs));
                    }
                }
                b'\n' if !pending_heredocs.is_empty() => {
                    self.advance();
                    for (delimiter, quoted, strip_tabs) in pending_heredocs.drain(..) {
                        self.skip_heredoc_body(
                            &delimiter,
                            quoted,
                            strip_tabs,
                            start_offset,
                            start_line,
                            start_col,
                        )?;
                    }
                }
                b'(' => {
                    depth += 1;
                    self.advance();
                }
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        let end = self.offset;
                        self.advance();
                        return Ok(end);
                    }
                    self.advance();
                }
                _ => {
                    self.advance();
                }
            }
        }

        Err(LexError {
            message: "Unterminated command substitution $(...)".to_string(),
            span: self.make_span(start_offset, start_line, start_col),
        })
    }

    fn scan_heredoc_delimiter_word(
        &mut self,
        start_offset: usize,
        start_line: u32,
        start_col: u32,
    ) -> Result<Option<(String, bool)>, LexError> {
        while matches!(self.peek_byte(), Some(b' ' | b'\t')) {
            self.advance();
        }
        if self.is_eof() || self.peek_byte() == Some(b'\n') {
            return Ok(None);
        }

        let word_start = self.offset;
        let mut quote = None;
        while !self.is_eof() {
            let current = self.bytes[self.offset];
            if quote == Some(b'\'') {
                if current == b'\'' {
                    quote = None;
                }
                self.advance();
                continue;
            }
            if quote == Some(b'"') {
                match current {
                    b'"' => {
                        quote = None;
                        self.advance();
                    }
                    b'\\' => {
                        self.advance();
                        if !self.is_eof() {
                            self.advance();
                        }
                    }
                    b'$' if self.bytes.get(self.offset + 1) == Some(&b'(') => {
                        let arithmetic = self.bytes.get(self.offset + 2) == Some(&b'(');
                        self.advance();
                        self.advance();
                        if arithmetic {
                            self.advance();
                            self.skip_arithmetic_expansion(start_offset, start_line, start_col)?;
                        } else {
                            self.scan_command_substitution_body(
                                start_offset,
                                start_line,
                                start_col,
                            )?;
                        }
                    }
                    _ => {
                        self.advance();
                    }
                }
                continue;
            }

            match current {
                b' ' | b'\t' | b'\n' | b';' | b'|' | b'&' | b'<' | b'>' | b'(' | b')' | b'{'
                | b'}' => break,
                b'\'' | b'"' => {
                    quote = Some(current);
                    self.advance();
                }
                b'\\' => {
                    self.advance();
                    if !self.is_eof() {
                        self.advance();
                    }
                }
                b'$' if self.bytes.get(self.offset + 1) == Some(&b'(')
                    && self.bytes.get(self.offset + 2) == Some(&b'(') =>
                {
                    self.advance();
                    self.advance();
                    self.advance();
                    self.skip_arithmetic_expansion(start_offset, start_line, start_col)?;
                }
                b'`' => {
                    self.skip_backtick_in_command_substitution(start_offset, start_line, start_col)?
                }
                b'$' if self.bytes.get(self.offset + 1) == Some(&b'(') => {
                    self.advance();
                    self.advance();
                    self.scan_command_substitution_body(start_offset, start_line, start_col)?;
                }
                _ => {
                    self.advance();
                }
            }
        }

        if self.offset == word_start {
            return Ok(None);
        }
        let raw = &self.source[word_start..self.offset];
        let (delimiter, quoted) = parse_heredoc_delimiter(raw);
        Ok(Some((delimiter, quoted)))
    }

    fn skip_heredoc_body(
        &mut self,
        delimiter: &str,
        quoted: bool,
        strip_tabs: bool,
        start_offset: usize,
        start_line: u32,
        start_col: u32,
    ) -> Result<(), LexError> {
        let mut continued = false;
        loop {
            if self.is_eof() {
                return Err(LexError {
                    message: format!("Unterminated here-document; expected '{delimiter}'"),
                    span: self.make_span(start_offset, start_line, start_col),
                });
            }

            let line_start = self.offset;
            while !self.is_eof() && self.peek_byte() != Some(b'\n') {
                self.advance();
            }
            let line_end = self.offset;
            let line = self.source[line_start..line_end].trim_end_matches('\r');
            let candidate = if strip_tabs {
                line.trim_start_matches('\t')
            } else {
                line
            };
            if !continued && candidate == delimiter {
                if self.peek_byte() == Some(b'\n') {
                    self.advance();
                }
                return Ok(());
            }

            continued = !quoted && line_has_unescaped_trailing_backslash(line);
            if self.peek_byte() == Some(b'\n') {
                self.advance();
            }
        }
    }

    fn skip_backtick_in_command_substitution(
        &mut self,
        start_offset: usize,
        start_line: u32,
        start_col: u32,
    ) -> Result<(), LexError> {
        self.advance(); // opening backtick
        while !self.is_eof() {
            match self.bytes[self.offset] {
                b'\\' => {
                    self.advance();
                    if !self.is_eof() {
                        self.advance();
                    }
                }
                b'`' => {
                    self.advance();
                    return Ok(());
                }
                _ => {
                    self.advance();
                }
            }
        }
        Err(LexError {
            message: "Unterminated command substitution using backticks".to_string(),
            span: self.make_span(start_offset, start_line, start_col),
        })
    }

    fn skip_arithmetic_expansion(
        &mut self,
        start_offset: usize,
        start_line: u32,
        start_col: u32,
    ) -> Result<(), LexError> {
        let mut depth = 0usize;
        let mut quote = None;
        while !self.is_eof() {
            let current = self.bytes[self.offset];
            if let Some(active_quote) = quote {
                if current == b'\\' {
                    self.advance();
                    if !self.is_eof() {
                        self.advance();
                    }
                } else {
                    if current == active_quote {
                        quote = None;
                    }
                    self.advance();
                }
                continue;
            }

            match current {
                b'\'' | b'"' => {
                    quote = Some(current);
                    self.advance();
                }
                b'(' => {
                    depth += 1;
                    self.advance();
                }
                b')' if depth > 0 => {
                    depth -= 1;
                    self.advance();
                }
                b')' if self.bytes.get(self.offset + 1) == Some(&b')') => {
                    self.advance();
                    self.advance();
                    return Ok(());
                }
                b'\\' => {
                    self.advance();
                    if !self.is_eof() {
                        self.advance();
                    }
                }
                _ => {
                    self.advance();
                }
            }
        }
        Err(LexError {
            message: "Unterminated arithmetic expansion $((...))".to_string(),
            span: self.make_span(start_offset, start_line, start_col),
        })
    }

    fn is_shell_comment_start(&self) -> bool {
        self.offset == 0
            || self.offset.checked_sub(1).is_some_and(|previous| {
                matches!(
                    self.bytes[previous],
                    b' ' | b'\t' | b'\n' | b';' | b'|' | b'&' | b'(' | b')' | b'<' | b'>'
                )
            })
    }

    fn read_backtick_substitution(
        &mut self,
        start_offset: usize,
        start_line: u32,
        start_col: u32,
    ) -> Result<Token<'a>, LexError> {
        self.advance(); // consume opening backtick
        let body_start = self.offset;
        while !self.is_eof() {
            match self.peek_byte() {
                Some(b'\\') => {
                    self.advance();
                    if !self.is_eof() {
                        self.advance();
                    }
                }
                Some(b'`') => {
                    let body_end = self.offset;
                    self.advance();
                    return Ok(Token::new(
                        TokenKind::CommandSub(&self.source[body_start..body_end]),
                        self.make_span(start_offset, start_line, start_col),
                    ));
                }
                _ => {
                    self.advance();
                }
            }
        }

        Err(LexError {
            message: "Unterminated command substitution using backticks".to_string(),
            span: self.make_span(start_offset, start_line, start_col),
        })
    }

    fn read_bare_word(
        &mut self,
        start_offset: usize,
        start_line: u32,
        start_col: u32,
    ) -> Result<Token<'a>, LexError> {
        while !self.is_eof() {
            let b = self.bytes[self.offset];
            // Stop at delimiters, whitespace, operators, quotes, or terminators
            if is_char_kind(b, CHAR_WHITESPACE)
                || b == b'\n'
                || b == b';'
                || b == b'|'
                || b == b'&'
                || b == b'>'
                || b == b'<'
                || b == b'('
                || b == b')'
                || b == b'{'
                || b == b'}'
                || b == b'"'
                || b == b'\''
                || b == b'$'
                || b == b'`'
            {
                break;
            }

            // Escaped character
            if b == b'\\' {
                self.advance();
                if !self.is_eof() {
                    self.advance();
                }
                continue;
            }

            self.advance();
        }

        let word = &self.source[start_offset..self.offset];
        if word.is_empty() {
            return Err(LexError {
                message: format!(
                    "Unexpected character: {:?}",
                    self.peek_byte().map(|b| b as char)
                ),
                span: self.make_span(start_offset, start_line, start_col),
            });
        }

        let kind = match word {
            "if" => TokenKind::If,
            "then" => TokenKind::Then,
            "elif" => TokenKind::Elif,
            "else" => TokenKind::Else,
            "fi" => TokenKind::Fi,
            "for" => TokenKind::For,
            "in" => TokenKind::In,
            "while" => TokenKind::While,
            "until" => TokenKind::Until,
            "do" => TokenKind::Do,
            "done" => TokenKind::Done,
            "case" => TokenKind::Case,
            "esac" => TokenKind::Esac,
            "select" => TokenKind::Select,
            _ => TokenKind::Word(word),
        };

        Ok(Token::new(
            kind,
            self.make_span(start_offset, start_line, start_col),
        ))
    }
}

fn line_has_unescaped_trailing_backslash(line: &str) -> bool {
    line.chars().rev().take_while(|ch| *ch == '\\').count() % 2 == 1
}

fn is_heredoc_delimiter_word(kind: TokenKind<'_>) -> bool {
    matches!(
        kind,
        TokenKind::Word(_)
            | TokenKind::SingleQuoted(_)
            | TokenKind::DoubleQuoted(_)
            | TokenKind::Variable(_)
            | TokenKind::CommandSub(_)
            | TokenKind::If
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
            | TokenKind::Select
    )
}

fn parse_heredoc_delimiter(raw: &str) -> (String, bool) {
    let mut result = String::new();
    let mut chars = raw.chars().peekable();
    let mut quote = None;
    let mut quoted = false;

    while let Some(ch) = chars.next() {
        match (quote, ch) {
            (None, '\'') => {
                quote = Some('\'');
                quoted = true;
            }
            (Some('\''), '\'') => quote = None,
            (None, '"') => {
                quote = Some('"');
                quoted = true;
            }
            (Some('"'), '"') => quote = None,
            (None, '\\') => {
                quoted = true;
                if let Some(next) = chars.next()
                    && next != '\n'
                {
                    result.push(next);
                }
            }
            (Some('"'), '\\') if matches!(chars.peek(), Some('$' | '`' | '"' | '\\')) => {
                quoted = true;
                if let Some(next) = chars.next() {
                    result.push(next);
                }
            }
            _ => result.push(ch),
        }
    }

    (result, quoted)
}
