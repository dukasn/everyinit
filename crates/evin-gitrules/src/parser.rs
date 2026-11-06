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
    #[error("Unsupported file format. Must be .flow or .json")]
    UnsupportedFormat,
    #[error(transparent)]
    Diagnostic(#[from] evin_diagnostic::Diagnostic),
}

pub fn parse_file<P: AsRef<Path>>(path: P) -> Result<GitRulesRoot, ParserError> {
    let path = path.as_ref();
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

    let content = fs::read_to_string(path)?;
    parse_str(&content, ext, Some(path))
}

/// Parse GitRules from caller-provided text, including editor document overlays.
pub fn parse_str(
    content: &str,
    extension: &str,
    path: Option<&Path>,
) -> Result<GitRulesRoot, ParserError> {
    match extension {
        "flow" => Ok(<GitRulesRoot as evin_flow::YamlDocument>::from_str(
            content, path,
        )?),
        "json" => Ok(serde_json::from_str(content)?),
        _ => Err(ParserError::UnsupportedFormat),
    }
}

#[cfg(test)]
mod tests {
    use super::{ParserError, parse_str};
    use crate::VersioningStrategy;
    use std::path::Path;

    #[test]
    fn flow_gitrules_accepts_v1_headers_and_rejects_other_evin_headers() {
        assert!(parse_str("#!evin/v1\nversion: 1\n", "flow", None).is_ok());
        assert!(matches!(
            parse_str("#!evin/v1\nversion: 1\n", "yaml", None),
            Err(ParserError::UnsupportedFormat)
        ));
        let error = parse_str(
            "#!evin/v2\nversion: 1\n",
            "flow",
            Some(Path::new("rules.flow")),
        )
        .unwrap_err();
        assert!(
            matches!(error, ParserError::Diagnostic(ref diagnostic) if diagnostic.code.key == "issue-evin-header-invalid")
        );
    }

    #[test]
    fn versioning_strategy_is_typed_and_defaults_to_semver() {
        for (flow, expected) in [
            ("releases: {}\n", VersioningStrategy::Semver),
            ("releases:\n  versioning: {}\n", VersioningStrategy::Semver),
            (
                "releases:\n  versioning:\n    strategy: cargo\n",
                VersioningStrategy::Cargo,
            ),
            (
                "releases:\n  versioning:\n    strategy: semver\n",
                VersioningStrategy::Semver,
            ),
        ] {
            let rules = parse_str(flow, "flow", None).unwrap();
            assert_eq!(rules.releases.unwrap().versioning.strategy, expected);
        }
        for flow in [
            "releases:\n  versioning:\n    strategy: unknown\n",
            "releases:\n  versioning:\n    strategy: null\n",
            "releases:\n  versioning: null\n",
            "releases:\n  versioning:\n    other: cargo\n",
        ] {
            assert!(parse_str(flow, "flow", None).is_err(), "{flow}");
        }
    }
}
