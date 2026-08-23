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

use thiserror::Error;

#[derive(Debug, Error)]
pub enum IoError {
    #[error("I/O error at '{path}': {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("Path traversal blocked: '{path}' escapes workspace root '{root}'")]
    PathTraversal { path: String, root: String },
    #[error("Not valid UTF-8: '{path}'")]
    Encoding { path: String },
    #[error("Invalid glob pattern: {0}")]
    InvalidPattern(String),
}

impl From<std::io::Error> for IoError {
    fn from(source: std::io::Error) -> Self {
        IoError::Io {
            path: "<unknown>".to_string(),
            source,
        }
    }
}
