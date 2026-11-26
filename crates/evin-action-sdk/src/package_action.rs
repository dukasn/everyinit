// Copyright (C) 2026 Eduardo Marques Santos
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.

//! Shared, format-neutral helpers for native package builtin actions.

#![allow(clippy::result_large_err)]

use crate::{RunnerContext, packaging};
use serde::{Deserialize, de::DeserializeOwned};

/// Package identity and task-filesystem payload mappings shared by native
/// package formats. Format-specific options belong to each action crate.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageIdentity {
    pub name: String,
    pub summary: String,
    pub maintainer: String,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub payload: Vec<packaging::PayloadMapping>,
}

pub fn parse_config<T: DeserializeOwned>(
    with: Option<&evin_flow::SpannedValue>,
) -> Result<T, evin_flow::ConfigError> {
    crate::parse_config(with).map_err(|error| {
        error.map_message(|message| evin_locales::t!("invalid-config", error = message))
    })
}

pub fn diagnostic_to_config_error(
    diagnostic: evin_diagnostic::Diagnostic,
) -> evin_flow::ConfigError {
    evin_flow::ConfigError {
        message: evin_locales::render_issue(&diagnostic.code),
        span: None,
    }
}

pub fn validate_release_reference_config(
    release: &str,
    needs: &[String],
) -> Result<(), evin_flow::ConfigError> {
    validate_release_reference(release, needs).map_err(|issue| evin_flow::ConfigError {
        message: evin_locales::render_issue(&issue),
        span: None,
    })
}

pub fn validate_output_and_release(
    output_dir: &str,
    release: &str,
) -> Result<(), evin_diagnostic::Diagnostic> {
    if output_dir.trim().is_empty() || release.trim().is_empty() {
        return Err(evin_locales::diagnostic(evin_diagnostic::IssueCode::new(
            "package-output-and-release-required",
        )));
    }
    Ok(())
}

pub fn validate_release_reference(
    release: &str,
    needs: &[String],
) -> Result<(), evin_diagnostic::IssueCode> {
    let task = release_reference_task(release).ok_or_else(|| {
        evin_diagnostic::IssueCode::new("package-release-reference-format-invalid")
    })?;
    if !needs.iter().any(|need| need == task) {
        return Err(
            evin_diagnostic::IssueCode::new("package-release-reference-missing-need")
                .arg("task", task),
        );
    }
    Ok(())
}

fn release_reference_task(release: &str) -> Option<&str> {
    release
        .strip_prefix("${")?
        .strip_suffix(".release}")
        .filter(|task| !task.is_empty())
}

pub fn validate_identity(identity: &PackageIdentity) -> Result<(), evin_diagnostic::Diagnostic> {
    for (field, value) in [
        ("name", &identity.name),
        ("summary", &identity.summary),
        ("maintainer", &identity.maintainer),
    ] {
        validate_metadata_string(field, value)?;
    }
    if let Some(homepage) = identity.homepage.as_deref() {
        validate_metadata_string("homepage", homepage)?;
    }
    if identity.payload.is_empty() {
        return Err(evin_locales::diagnostic(evin_diagnostic::IssueCode::new(
            "package-payload-required",
        )));
    }
    if !identity.name.contains("${")
        && !identity.name.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || character == '-'
                || character == '_'
                || character == '.'
        })
    {
        return Err(evin_locales::diagnostic(evin_diagnostic::IssueCode::new(
            "package-name-invalid",
        )));
    }
    Ok(())
}

pub fn validate_metadata_string(
    field: &str,
    value: &str,
) -> Result<(), evin_diagnostic::Diagnostic> {
    if value.trim().is_empty() || value.contains(['\n', '\r', '\0']) {
        return Err(evin_locales::diagnostic(
            evin_diagnostic::IssueCode::new("package-metadata-invalid").arg("field", field),
        ));
    }
    Ok(())
}

pub fn validate_relations(
    field: &str,
    values: &[String],
) -> Result<(), evin_diagnostic::Diagnostic> {
    let mut seen = std::collections::BTreeSet::new();
    for value in values {
        if value.trim().is_empty() || value.contains(['\n', '\r', '\0']) {
            return Err(evin_locales::diagnostic(
                evin_diagnostic::IssueCode::new("package-relation-invalid").arg("field", field),
            ));
        }
        if !seen.insert(value) {
            return Err(evin_locales::diagnostic(
                evin_diagnostic::IssueCode::new("package-relation-duplicate")
                    .arg("field", field)
                    .arg("value", value),
            ));
        }
    }
    Ok(())
}

pub fn validate_script_path(path: &str) -> Result<(), evin_diagnostic::Diagnostic> {
    if path.trim().is_empty() {
        return Err(evin_locales::diagnostic(evin_diagnostic::IssueCode::new(
            "package-script-path-empty",
        )));
    }
    packaging::safe_relative_path(path).map_err(evin_diagnostic::Diagnostic::from)?;
    Ok(())
}

pub fn resolve_payloads(
    ctx: &RunnerContext,
    identity: &PackageIdentity,
) -> Result<Vec<packaging::SourceEntry>, evin_diagnostic::Diagnostic> {
    packaging::resolve_payloads(&ctx.fs, &identity.payload)
        .map_err(evin_diagnostic::Diagnostic::from)
}

pub fn load_scripts(
    ctx: &RunnerContext,
    definitions: &[(&str, &str)],
) -> Result<Vec<(String, Vec<u8>)>, evin_diagnostic::Diagnostic> {
    definitions
        .iter()
        .map(|(name, path)| {
            let relative =
                packaging::safe_relative_path(path).map_err(evin_diagnostic::Diagnostic::from)?;
            let source =
                packaging::resolve_workspace_source(&ctx.fs, &ctx.fs.task_cwd().join(relative))
                    .map_err(evin_diagnostic::Diagnostic::from)?;
            let bytes = std::fs::read(source).map_err(|error| {
                evin_locales::diagnostic(
                    evin_diagnostic::IssueCode::new("package-script-read-failed")
                        .arg("error", error.to_string()),
                )
            })?;
            Ok(((*name).to_string(), bytes))
        })
        .collect()
}

pub fn release_state(
    serialized: &str,
    package_name: &str,
    target: &str,
) -> Result<(String, u32), evin_diagnostic::Diagnostic> {
    let artifact: crate::release::ReleaseArtifact =
        serde_json::from_str(serialized).map_err(|error| {
            evin_locales::diagnostic(
                evin_diagnostic::IssueCode::new("package-release-output-invalid")
                    .arg("error", error.to_string()),
            )
        })?;
    if artifact.schema != 1 {
        return Err(evin_locales::diagnostic(
            evin_diagnostic::IssueCode::new("package-release-schema-unsupported")
                .arg("schema", artifact.schema),
        ));
    }
    let state = artifact
        .packages
        .get(package_name)
        .and_then(|package| package.revisions.get(target))
        .ok_or_else(|| {
            evin_locales::diagnostic(
                evin_diagnostic::IssueCode::new("package-release-revision-missing")
                    .arg("target", target)
                    .arg("package", package_name),
            )
        })?;
    if state.revision == 0 || state.version.trim().is_empty() {
        return Err(evin_locales::diagnostic(evin_diagnostic::IssueCode::new(
            "package-release-version-revision-invalid",
        )));
    }
    Ok((state.version.clone(), state.revision))
}

pub fn output_path(output_dir: &str, filename: &str) -> String {
    format!("{}/{}", output_dir.trim_end_matches('/'), filename)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_reference_requires_the_exact_output_and_declared_dependency() {
        let needs = vec!["release".to_string()];
        assert!(validate_release_reference("${release.release}", &needs).is_ok());
        assert_eq!(
            validate_release_reference("${release.version}", &needs)
                .unwrap_err()
                .key,
            "package-release-reference-format-invalid"
        );
        assert_eq!(
            validate_release_reference("${release.release}", &[])
                .unwrap_err()
                .key,
            "package-release-reference-missing-need"
        );
    }

    #[test]
    fn release_state_selects_the_requested_target_without_a_format_registry() {
        let serialized = serde_json::json!({
            "schema": 1,
            "release": {"version": "1.2.3", "tag": "v1.2.3"},
            "packages": {
                "demo": {"revisions": {
                    "rpm": {"version": "1.2.3", "revision": 2},
                    "deb": {"version": "1.2.3", "revision": 4}
                }}
            }
        })
        .to_string();
        assert_eq!(
            release_state(&serialized, "demo", "rpm").unwrap(),
            ("1.2.3".to_string(), 2)
        );
        assert_eq!(
            release_state(&serialized, "demo", "apk")
                .unwrap_err()
                .code
                .key,
            "package-release-revision-missing"
        );
    }

    #[test]
    fn identity_requires_payload_and_validates_common_metadata() {
        let empty = PackageIdentity {
            name: "demo".into(),
            summary: "Demo".into(),
            maintainer: "Maintainer".into(),
            homepage: None,
            payload: Vec::new(),
        };
        assert_eq!(
            validate_identity(&empty).unwrap_err().code.key,
            "package-payload-required"
        );
        let populated = PackageIdentity {
            payload: vec![packaging::PayloadMapping {
                source: "target/file".into(),
                strip_prefix: "target".into(),
                destination: "/usr/bin".into(),
                mode: None,
            }],
            ..empty
        };
        assert!(validate_identity(&populated).is_ok());
    }
}
