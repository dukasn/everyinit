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

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Span {
    pub start: u32,
    pub end: u32,
    pub line: u32,
    pub column: u32,
}

impl Span {
    pub const fn new(start: u32, end: u32, line: u32, column: u32) -> Self {
        Self {
            start,
            end,
            line,
            column,
        }
    }

    pub fn merge(self, other: Span) -> Self {
        Self {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
            line: self.line.min(other.line),
            column: if self.start <= other.start {
                self.column
            } else {
                other.column
            },
        }
    }
}

impl From<Span> for evin_diagnostic::Span {
    fn from(s: Span) -> Self {
        evin_diagnostic::Span {
            line: s.line as usize,
            column: s.column as usize,
            offset: s.start as usize,
            length: (s.end.saturating_sub(s.start)) as usize,
        }
    }
}

impl From<evin_diagnostic::Span> for Span {
    fn from(s: evin_diagnostic::Span) -> Self {
        Span {
            start: s.offset as u32,
            end: (s.offset + s.length) as u32,
            line: s.line as u32,
            column: s.column as u32,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind<'a> {
    Whitespace(&'a str),
    Comment(&'a str),
    Word(&'a str),
    HereDocDelimiter {
        body: &'a str,
        quoted: bool,
        strip_tabs: bool,
    },
    SingleQuoted(&'a str),
    DoubleQuoted(&'a str),
    Variable(&'a str),
    CommandSub(&'a str),
    Pipe,                   // |
    ParallelPipe,           // &|
    AndIf,                  // &&
    OrIf,                   // ||
    Semicolon,              // ;
    Newline,                // \n
    LBrace,                 // {
    RBrace,                 // }
    LParen,                 // (
    RParen,                 // )
    RedirectOut,            // >
    RedirectClobber,        // >|
    RedirectAppend,         // >>
    RedirectIn,             // <
    RedirectHereDoc,        // <<
    RedirectHereDocStrip,   // <<-
    RedirectHereString,     // <<<
    RedirectStderr,         // 2>
    RedirectStderrAppend,   // 2>>
    RedirectStderrToStdout, // 2>&1
    RedirectStdoutToStderr, // 1>&2
    RedirectAll,            // &>
    RedirectAllAppend,      // &>>
    Bang,                   // !
    If,                     // if
    Then,                   // then
    Elif,                   // elif
    Else,                   // else
    Fi,                     // fi
    For,                    // for
    In,                     // in
    While,                  // while
    Until,                  // until
    Do,                     // do
    Done,                   // done
    Case,                   // case
    Esac,                   // esac
    DoubleSemi,             // ;;
    SemiAnd,                // ;&
    DoubleSemiAnd,          // ;;&
    Eof,
}

impl<'a> TokenKind<'a> {
    #[inline(always)]
    pub fn is_trivia(&self) -> bool {
        matches!(self, TokenKind::Whitespace(_) | TokenKind::Comment(_))
    }
}

impl<'a> fmt::Display for TokenKind<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenKind::Whitespace(s) => write!(f, "{}", s),
            TokenKind::Comment(s) => write!(f, "{}", s),
            TokenKind::Word(s) => write!(f, "{}", s),
            TokenKind::HereDocDelimiter { .. } => write!(f, "<here-document>"),
            TokenKind::SingleQuoted(s) => write!(f, "'{}'", s),
            TokenKind::DoubleQuoted(s) => write!(f, "\"{}\"", s),
            TokenKind::Variable(s) => write!(f, "${}", s),
            TokenKind::CommandSub(s) => write!(f, "$({})", s),
            TokenKind::Pipe => write!(f, "|"),
            TokenKind::ParallelPipe => write!(f, "&|"),
            TokenKind::AndIf => write!(f, "&&"),
            TokenKind::OrIf => write!(f, "||"),
            TokenKind::Semicolon => write!(f, ";"),
            TokenKind::Newline => write!(f, "\\n"),
            TokenKind::LBrace => write!(f, "{{"),
            TokenKind::RBrace => write!(f, "}}"),
            TokenKind::LParen => write!(f, "("),
            TokenKind::RParen => write!(f, ")"),
            TokenKind::RedirectOut => write!(f, ">"),
            TokenKind::RedirectClobber => write!(f, ">|"),
            TokenKind::RedirectAppend => write!(f, ">>"),
            TokenKind::RedirectIn => write!(f, "<"),
            TokenKind::RedirectHereDoc => write!(f, "<<"),
            TokenKind::RedirectHereDocStrip => write!(f, "<<-"),
            TokenKind::RedirectHereString => write!(f, "<<<"),
            TokenKind::RedirectStderr => write!(f, "2>"),
            TokenKind::RedirectStderrAppend => write!(f, "2>>"),
            TokenKind::RedirectStderrToStdout => write!(f, "2>&1"),
            TokenKind::RedirectStdoutToStderr => write!(f, "1>&2"),
            TokenKind::RedirectAll => write!(f, "&>"),
            TokenKind::RedirectAllAppend => write!(f, "&>>"),
            TokenKind::Bang => write!(f, "!"),
            TokenKind::If => write!(f, "if"),
            TokenKind::Then => write!(f, "then"),
            TokenKind::Elif => write!(f, "elif"),
            TokenKind::Else => write!(f, "else"),
            TokenKind::Fi => write!(f, "fi"),
            TokenKind::For => write!(f, "for"),
            TokenKind::In => write!(f, "in"),
            TokenKind::While => write!(f, "while"),
            TokenKind::Until => write!(f, "until"),
            TokenKind::Do => write!(f, "do"),
            TokenKind::Done => write!(f, "done"),
            TokenKind::Case => write!(f, "case"),
            TokenKind::Esac => write!(f, "esac"),
            TokenKind::DoubleSemi => write!(f, ";;"),
            TokenKind::SemiAnd => write!(f, ";&"),
            TokenKind::DoubleSemiAnd => write!(f, ";;&"),
            TokenKind::Eof => write!(f, "<EOF>"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token<'a> {
    pub kind: TokenKind<'a>,
    pub span: Span,
}

impl<'a> Token<'a> {
    pub const fn new(kind: TokenKind<'a>, span: Span) -> Self {
        Self { kind, span }
    }

    #[inline(always)]
    pub fn slice<'s>(&self, source: &'s str) -> &'s str {
        &source[self.span.start as usize..self.span.end as usize]
    }
}
