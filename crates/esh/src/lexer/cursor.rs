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
        let mut tokens = Vec::new();
        loop {
            let token = self.next_token()?;
            let is_eof = token.kind == TokenKind::Eof;
            tokens.push(token);
            if is_eof {
                break;
            }
        }
        Ok(tokens)
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
                    TokenKind::RedirectStderr,
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
            return Ok(Token::new(
                TokenKind::RedirectOut,
                self.make_span(start_offset, start_line, start_col),
            ));
        }
        if b == b'<' {
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
                    TokenKind::RedirectAll,
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

        // 1>&2 redirection
        if b == b'1'
            && self.peek_byte_at(1) == Some(b'>')
            && self.peek_byte_at(2) == Some(b'&')
            && self.peek_byte_at(3) == Some(b'2')
        {
            self.advance(); // consume '1'
            self.advance(); // consume '>'
            self.advance(); // consume '&'
            self.advance(); // consume '2'
            return Ok(Token::new(
                TokenKind::RedirectStderr,
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
            let mut depth = 1;
            while !self.is_eof() && depth > 0 {
                let curr = self.bytes[self.offset];
                if curr == b'(' {
                    depth += 1;
                } else if curr == b')' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                self.advance();
            }

            if depth > 0 || self.is_eof() {
                return Err(LexError {
                    message: "Unterminated command substitution $(...)".to_string(),
                    span: self.make_span(start_offset, start_line, start_col),
                });
            }

            let cmd_end = self.offset;
            self.advance(); // consume closing ')'

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
            while !self.is_eof() && self.peek_byte() != Some(b'}') {
                self.advance();
            }
            if self.is_eof() {
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

        // Special variables: $?, $@, $*, $#, $0..$9
        if b == b'?' || b == b'@' || b == b'*' || b == b'#' || b.is_ascii_digit() {
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
            _ => TokenKind::Word(word),
        };

        Ok(Token::new(
            kind,
            self.make_span(start_offset, start_line, start_col),
        ))
    }
}
