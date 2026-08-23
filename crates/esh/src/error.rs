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

use crate::lexer::Span;
use evin_diagnostic::{Diagnostic, IssueCode, Severity};
use std::fmt;
use thiserror::Error;

/// Root error type for esh operations.
#[derive(Debug, Error)]
pub enum EshError {
    #[error("Lexer error: {0}")]
    Lex(#[from] LexError),

    #[error("Parse error: {0}")]
    Parse(#[from] ParseError),

    #[error("Execution error: {0}")]
    Exec(#[from] ExecError),
}

impl From<EshError> for Diagnostic {
    fn from(err: EshError) -> Self {
        match err {
            EshError::Lex(e) => e.into(),
            EshError::Parse(e) => e.into(),
            EshError::Exec(e) => e.into(),
        }
    }
}

/// Errors originating from the zero-copy lexer.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub struct LexError {
    pub message: String,
    pub span: Span,
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} at line {}, column {}",
            self.message, self.span.line, self.span.column
        )
    }
}

impl From<LexError> for Diagnostic {
    fn from(err: LexError) -> Self {
        let diag_span: evin_diagnostic::Span = err.span.into();
        Diagnostic::new(
            "Syntax error during lexing",
            IssueCode::new("esh_syntax_error").arg("message", &err.message),
        )
        .with_severity(Severity::Error)
        .with_file_span(
            "<esh>",
            diag_span.offset,
            diag_span.length,
            Some(err.message),
        )
    }
}

/// Errors originating from the Pratt parser.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub struct ParseError {
    pub message: String,
    pub span: Span,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} at line {}, column {}",
            self.message, self.span.line, self.span.column
        )
    }
}

impl From<ParseError> for Diagnostic {
    fn from(err: ParseError) -> Self {
        let diag_span: evin_diagnostic::Span = err.span.into();
        Diagnostic::new(
            "Syntax error during parsing",
            IssueCode::new("esh_parse_error").arg("message", &err.message),
        )
        .with_severity(Severity::Error)
        .with_file_span(
            "<esh>",
            diag_span.offset,
            diag_span.length,
            Some(err.message),
        )
    }
}

/// Errors occurring during plan lowering or execution runtime.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ExecError {
    #[error("Command not found: {0}")]
    CommandNotFound(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("No such file or directory: {0}")]
    NotFound(String),

    #[error("Invalid argument: {0}")]
    InvalidArgument(String),

    #[error("Scope error: {0}")]
    ScopeUnderflow(String),

    #[error("Join error: {0}")]
    JoinError(String),

    #[error("IO error: {0}")]
    IoError(String),

    #[error("Runtime failure with exit code {0}")]
    ExitCode(i32),

    #[error("Custom error: {0}")]
    Custom(String),
}

impl ExecError {
    pub fn exit_code(&self) -> i32 {
        match self {
            ExecError::ExitCode(code) => *code,
            ExecError::CommandNotFound(_) => 127,
            ExecError::PermissionDenied(_) => 126,
            ExecError::NotFound(_) => 1,
            ExecError::InvalidArgument(_) => 1,
            ExecError::ScopeUnderflow(_) => 1,
            ExecError::JoinError(_) => 1,
            ExecError::IoError(_) => 1,
            ExecError::Custom(_) => 1,
        }
    }
}

impl From<std::io::Error> for ExecError {
    fn from(e: std::io::Error) -> Self {
        ExecError::IoError(e.to_string())
    }
}

impl From<ExecError> for Diagnostic {
    fn from(err: ExecError) -> Self {
        Diagnostic::new(
            "Esh runtime execution failure",
            IssueCode::new("esh_exec_error").arg("message", err.to_string()),
        )
        .with_severity(Severity::Error)
    }
}
