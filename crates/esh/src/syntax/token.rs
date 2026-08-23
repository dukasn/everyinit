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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenKind {
    Plain,
    Comment,
    Keyword,
    Command,
    Builtin,
    Flag,
    Variable,
    String,
    Number,
    Operator,
    Punctuation,
}

pub type SyntaxCategory = TokenKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct TokenSpan {
    pub line: usize,
    pub col: usize,
    pub col_utf16: usize,
    pub length_utf16: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxToken<'a> {
    pub kind: TokenKind,
    pub text: &'a str,
    pub line: usize,
    pub col: usize,
    pub col_utf16: usize,
    pub length_utf16: usize,
}

impl<'a> SyntaxToken<'a> {
    pub fn new(kind: TokenKind, text: &'a str, span: TokenSpan) -> Self {
        Self {
            kind,
            text,
            line: span.line,
            col: span.col,
            col_utf16: span.col_utf16,
            length_utf16: span.length_utf16,
        }
    }
}
