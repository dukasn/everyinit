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

pub mod span;
pub use span::{Span, Spanned};

pub mod issue_code;
pub use issue_code::IssueCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Severity {
    #[default]
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileLocation {
    Path {
        path: String,
        line: Option<usize>,
        column: Option<usize>,
    },
    ByteSpan {
        path: String,
        offset: usize,
        length: usize,
        label: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub title: String,
    pub code: IssueCode,
    pub hint: Option<String>,
    pub location: Option<FileLocation>,
    pub severity: Severity,
}

impl Diagnostic {
    pub fn new(title: impl Into<String>, code: IssueCode) -> Self {
        Self {
            title: title.into(),
            code,
            hint: None,
            location: None,
            severity: Severity::default(),
        }
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub fn with_severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }

    pub fn with_file_location(
        mut self,
        path: impl Into<String>,
        line: Option<usize>,
        column: Option<usize>,
    ) -> Self {
        self.location = Some(FileLocation::Path {
            path: path.into(),
            line,
            column,
        });
        self
    }

    pub fn with_file_span(
        mut self,
        path: impl Into<String>,
        offset: usize,
        length: usize,
        label: Option<String>,
    ) -> Self {
        self.location = Some(FileLocation::ByteSpan {
            path: path.into(),
            offset,
            length,
            label,
        });
        self
    }
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {:?}", self.title, self.code)
    }
}

impl std::error::Error for Diagnostic {}

impl From<String> for Diagnostic {
    fn from(s: String) -> Self {
        Self::new("Action Failed", IssueCode::custom(s))
    }
}
