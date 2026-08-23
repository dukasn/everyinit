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

#![allow(clippy::result_large_err)]
use crate::models::GitRulesRoot;
use serde_json::Error as JsonError;
use std::fs;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParserError {
    #[error("I/O Error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON Parsing Error: {0}")]
    Json(#[from] JsonError),
    #[error("Unsupported file format. Must be .yaml, .yml, or .json")]
    UnsupportedFormat,
    #[error(transparent)]
    Diagnostic(#[from] evin_diagnostic::Diagnostic),
}

pub fn parse_file<P: AsRef<Path>>(path: P) -> Result<GitRulesRoot, ParserError> {
    let path = path.as_ref();
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

    match ext {
        "yaml" | "yml" => {
            let root = <GitRulesRoot as evin_yaml::YamlDocument>::from_file(path)?;
            Ok(root)
        }
        "json" => {
            let content = fs::read_to_string(path)?;
            let root: GitRulesRoot = serde_json::from_str(&content)?;
            Ok(root)
        }
        _ => Err(ParserError::UnsupportedFormat),
    }
}
