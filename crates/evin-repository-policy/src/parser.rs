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
use crate::models::RepositoryPolicy;
use serde_json::{Error as JsonError, Value};
use std::fs;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParserError {
    #[error("I/O Error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON Parsing Error: {0}")]
    Json(#[from] JsonError),
    #[error("JSON Schema validation failed: {0}")]
    Schema(String),
    #[error("Unsupported file format. Repository policy must be .json")]
    UnsupportedFormat,
    #[error(transparent)]
    Diagnostic(#[from] evin_diagnostic::Diagnostic),
}

pub fn parse_file<P: AsRef<Path>>(path: P) -> Result<RepositoryPolicy, ParserError> {
    let path = path.as_ref();
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

    let content = fs::read_to_string(path)?;
    parse_str(&content, ext, Some(path))
}

/// Parse RepositoryPolicy from a JSON manifest. The schema is bundled so
/// configuration loading never depends on network availability.
pub fn parse_str(
    content: &str,
    extension: &str,
    _path: Option<&Path>,
) -> Result<RepositoryPolicy, ParserError> {
    if extension != "json" {
        return Err(ParserError::UnsupportedFormat);
    }
    let value: Value = serde_json::from_str(content)?;
    let schema: Value =
        serde_json::from_str(include_str!("../../../schemas/v1/policy.schema.json"))?;
    let validator = jsonschema::validator_for(&schema)
        .map_err(|error| ParserError::Schema(error.to_string()))?;
    let errors = validator
        .iter_errors(&value)
        .map(|error| format!("{} {error}", error.instance_path()))
        .collect::<Vec<_>>();
    if !errors.is_empty() {
        return Err(ParserError::Schema(errors.join("; ")));
    }
    Ok(serde_json::from_value(value)?)
}

#[cfg(test)]
mod tests {
    use super::{ParserError, parse_str};
    use crate::VersioningStrategy;
    use std::path::Path;

    #[test]
    fn policy_requires_json_schema_and_rejects_flow() {
        assert!(matches!(
            parse_str("{}", "flow", None),
            Err(ParserError::UnsupportedFormat)
        ));
        let error = parse_str("{}", "json", Some(Path::new("policy.json"))).unwrap_err();
        assert!(matches!(error, ParserError::Schema(_)));
        let error = parse_str(
            "#!evin/v1\nreleases: {}\n",
            "flow",
            Some(Path::new("policy.flow")),
        )
        .unwrap_err();
        assert!(matches!(error, ParserError::UnsupportedFormat));
    }

    #[test]
    fn versioning_strategy_is_typed_and_defaults_to_semver() {
        for (releases, expected) in [
            ("{}", VersioningStrategy::Semver),
            (r#"{"versioning":{}}"#, VersioningStrategy::Semver),
            (
                r#"{"versioning":{"strategy":"cargo"}}"#,
                VersioningStrategy::Cargo,
            ),
            (
                r#"{"versioning":{"strategy":"semver"}}"#,
                VersioningStrategy::Semver,
            ),
        ] {
            let content = format!(
                r#"{{"$schema":"https://dl.everyinit.dev/schema/v1/policy","releases":{releases}}}"#
            );
            let rules = parse_str(&content, "json", None).unwrap();
            assert_eq!(rules.releases.unwrap().versioning.strategy, expected);
        }
        for releases in [
            r#"{"versioning":{"strategy":"unknown"}}"#,
            r#"{"versioning":{"strategy":null}}"#,
            r#"{"versioning":null}"#,
            r#"{"versioning":{"other":"cargo"}}"#,
        ] {
            let content = format!(
                r#"{{"$schema":"https://dl.everyinit.dev/schema/v1/policy","releases":{releases}}}"#
            );
            assert!(parse_str(&content, "json", None).is_err(), "{content}");
        }
    }
}
