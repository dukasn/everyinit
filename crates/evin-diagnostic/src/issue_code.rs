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

/// A lightweight, locale-first data carrier for issue codes.
/// Holds an FTL message key and named arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssueCode {
    pub key: &'static str,
    pub args: Vec<(&'static str, String)>,
}

impl IssueCode {
    pub const fn new(key: &'static str) -> Self {
        Self {
            key,
            args: Vec::new(),
        }
    }

    pub fn arg(mut self, name: &'static str, value: impl ToString) -> Self {
        self.args.push((name, value.to_string()));
        self
    }

    /// Escape hatch for untyped runtime messages.
    pub fn custom(message: impl Into<String>) -> Self {
        Self::new("__custom__").arg("message", message.into())
    }

    pub fn is_custom(&self) -> bool {
        self.key == "__custom__"
    }
}

impl fmt::Display for IssueCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.key)
    }
}
