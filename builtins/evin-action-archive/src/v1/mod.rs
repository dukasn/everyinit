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
mod action;
mod archiver;
pub mod config;
mod mapping;
mod packages;
mod reporting;
mod strategies;
pub use action::ArchiveAction;

use archiver::Archiver;
use config::ArchiveConfig;
use strategies::targz::TarGzArchiver;

use evin_action_sdk::{
    RunnerContext,
    packaging::{self, PackageContext, PackageFormat, SourceEntry},
};
use mapping::resolve_and_map_files;
use packages::{
    PACKAGE_ENCODERS, PackageBuild, load_scripts, package_filename, resolve_package_payloads,
    validate_package,
};
use reporting::{archive_input_report, compact_archive_paths, format_byte_size};

pub fn validate_config(
    with_payload: Option<&evin_flow::SpannedValue>,
) -> Result<(), evin_flow::ConfigError> {
    let payload = with_payload
        .cloned()
        .unwrap_or(evin_flow::SpannedValue::Null(Default::default()));
    let config: ArchiveConfig = evin_action_sdk::parse_config(Some(&payload)).map_err(|error| {
        error.map_message(|message| evin_locales::t!("invalid-config", error = message))
    })?;
    config
        .validate()
        .and_then(|_| {
            validate_format_keys(
                &payload,
                PackageFormat::parse(&config.format).expect("validated format"),
            )
        })
        .map_err(|d| evin_flow::ConfigError {
            message: evin_locales::render_issue(&d.code),
            span: None,
        })
}

pub fn validate_task_config(
    ctx: &evin_action_sdk::ActionValidationContext<'_>,
    with_payload: Option<&evin_flow::SpannedValue>,
) -> Result<(), evin_flow::ConfigError> {
    validate_config(with_payload)?;
    let config: ArchiveConfig = evin_action_sdk::parse_config(with_payload)?;
    let Some(release) = config.release else {
        return Ok(());
    };
    let task = release_reference_task(&release).ok_or_else(|| evin_flow::ConfigError {
        message: evin_locales::t!("archive-release-reference-format-invalid"),
        span: None,
    })?;
    if !ctx.needs.iter().any(|need| need == task) {
        return Err(evin_flow::ConfigError {
            message: evin_locales::t!("archive-release-reference-missing-need", task = task),
            span: None,
        });
    }
    Ok(())
}

fn release_reference_task(value: &str) -> Option<&str> {
    value
        .strip_prefix("${")?
        .strip_suffix(".release}")
        .filter(|task| !task.is_empty())
}

pub async fn execute(
    ctx: &RunnerContext,
    with_payload: Option<evin_flow::SpannedValue>,
) -> Result<(), evin_diagnostic::Diagnostic> {
    let mut config: ArchiveConfig = ctx.load_config(with_payload.as_ref()).map_err(|e| {
        evin_locales::diagnostic(
            evin_diagnostic::IssueCode::new("invalid-config").arg("error", e.to_string()),
        )
    })?;
    config.validate()?;
    validate_format_keys(
        with_payload
            .as_ref()
            .unwrap_or(&evin_flow::SpannedValue::Null(Default::default())),
        config.package_format()?,
    )?;

    let format = config.package_format()?;
    let mut package_build = None;
    let source_entries = if format == PackageFormat::TarGz {
        resolve_and_map_files(ctx, &config)?
            .into_iter()
            .map(|entry| SourceEntry {
                source: entry.source,
                destination: entry.destination.to_string_lossy().replace('\\', "/"),
                mode: None,
            })
            .collect::<Vec<_>>()
    } else {
        let package = config.package.clone().expect("validated package");
        validate_package(&package, format)?;
        let architecture = config.architecture.expect("validated architecture");
        match format {
            PackageFormat::Deb if package.targets.deb.is_none() => {
                return Err(diagnostic(evin_locales::t!(
                    "archive-package-target-missing",
                    format = "deb"
                )));
            }
            PackageFormat::Apk if package.targets.apk.is_none() => {
                return Err(diagnostic(evin_locales::t!(
                    "archive-package-target-missing",
                    format = "apk"
                )));
            }
            PackageFormat::Arch if package.targets.arch.is_none() => {
                return Err(diagnostic(evin_locales::t!(
                    "archive-package-target-missing",
                    format = "arch"
                )));
            }
            _ => {}
        }
        let artifact: evin_action_sdk::release::ReleaseArtifact = serde_json::from_str(
            config.release.as_deref().expect("validated release"),
        )
        .map_err(|error| {
            diagnostic(evin_locales::t!(
                "archive-release-output-invalid",
                error = error.to_string()
            ))
        })?;
        if artifact.schema != 1 {
            return Err(diagnostic(evin_locales::t!(
                "archive-release-schema-unsupported",
                schema = artifact.schema
            )));
        }
        let target_name = format.as_str();
        let release_state = artifact
            .packages
            .get(&package.name)
            .and_then(|package| package.revisions.get(target_name))
            .ok_or_else(|| {
                diagnostic(evin_locales::t!(
                    "archive-release-revision-missing",
                    target = target_name,
                    package = package.name.as_str()
                ))
            })?;
        if release_state.revision == 0 || release_state.version.trim().is_empty() {
            return Err(diagnostic(evin_locales::t!(
                "archive-release-version-revision-invalid"
            )));
        }
        let revision = release_state.revision;
        let arch = match format {
            PackageFormat::Deb => architecture.deb().to_string(),
            PackageFormat::Apk => architecture.apk().to_string(),
            PackageFormat::Arch => architecture.arch().to_string(),
            PackageFormat::TarGz => unreachable!(),
        };
        let filename = package_filename(
            format,
            &package.name,
            &release_state.version,
            revision,
            &arch,
        );
        config.path = Some(format!(
            "{}/{}",
            config
                .output_dir
                .as_deref()
                .expect("validated output_dir")
                .trim_end_matches('/'),
            filename
        ));
        let sources = resolve_package_payloads(ctx, &package)?;
        package_build = Some((package, arch, revision, release_state.version.clone()));
        sources
    };
    let archive_path = config.path.as_deref().expect("validated archive path");
    if source_entries.is_empty() {
        if ctx.mode == evin_action_sdk::PipelineMode::DryRun && !ctx.uncertainty().is_empty() {
            ctx.mark_unverified(evin_locales::t!(
                "archive-dry-run-inputs-unverified",
                includes = config.include.as_deref().unwrap_or_default().join(", ")
            ));
            ctx.log
                .plan(evin_action_sdk::activity::ArchivePlanUnresolved {
                    path: archive_path.to_string(),
                });
            ctx.state.set_output("path", archive_path);
            ctx.state.set_output("size", "unknown");
            return Ok(());
        }
        return Err(evin_locales::diagnostic(evin_diagnostic::IssueCode::new(
            "no-files-matched",
        )));
    }

    let output_path = ctx.fs.get_safe_physical_path(archive_path).map_err(|e| {
        evin_locales::diagnostic(
            evin_diagnostic::IssueCode::new("issue-archive-safe-path-failed")
                .arg("context", "archive path")
                .arg("error", e.to_string()),
        )
    })?;

    if ctx.mode == evin_action_sdk::PipelineMode::DryRun {
        ctx.log.plan(evin_action_sdk::activity::ArchivePlan {
            files: source_entries.len(),
            path: archive_path.to_string(),
        });
        ctx.state.set_output("path", archive_path);
        ctx.state.set_output("size", "0");
        return Ok(());
    }

    // Ensure the parent directory of the output file exists
    if let Some(parent) = output_path.parent()
        && !parent.as_os_str().is_empty()
    {
        tokio::fs::create_dir_all(parent).await.map_err(|e| {
            evin_locales::diagnostic(
                evin_diagnostic::IssueCode::new("create-output-dir-failed")
                    .arg("error", e.to_string()),
            )
        })?;
    }

    evin_action_sdk::action_event!(
        ctx.log,
        evin_action_sdk::activity::ArchiveCompressing {
            files: source_entries.len(),
            path: archive_path.to_string(),
        }
    );
    let epoch = resolve_epoch(ctx).await?;
    let build = if let Some((package, arch, revision, version)) = package_build {
        let context = PackageContext::prepare(epoch, source_entries.clone())
            .map_err(|error| diagnostic(error.to_string()))?;
        let scripts = load_scripts(ctx, &package, format)?;
        Some(PackageBuild {
            context,
            package,
            format,
            arch,
            revision,
            version,
            scripts,
        })
    } else {
        None
    };
    let format_name = config.format.clone();
    let compression_level = config.compression_level;
    let write_path = output_path.clone();
    let report = tokio::task::spawn_blocking(move || {
        let mut report = None;
        packaging::write_atomically(&write_path, |output| {
            if let Some(build) = build {
                report = Some(archive_input_report(&build.context));
                let encoder = PACKAGE_ENCODERS
                    .iter()
                    .find(|encoder| encoder.format == build.format)
                    .expect("fixed package strategy LUT");
                (encoder.write)(output, &build)
            } else {
                let context = PackageContext::prepare(epoch, source_entries)?;
                report = Some(archive_input_report(&context));
                TarGzArchiver { compression_level }.write_archive(output, &context)
            }
        })
        .map_err(|error| diagnostic(error.to_string()))?;
        Ok::<_, evin_diagnostic::Diagnostic>(
            report.expect("archive report is prepared before writing"),
        )
    })
    .await
    .map_err(|e| {
        evin_locales::diagnostic(evin_diagnostic::IssueCode::custom(evin_locales::t!(
            "archive-blocking-task-failed",
            error = e.to_string()
        )))
    })??;

    ctx.state.set_output("path", archive_path);
    if let Ok(metadata) = tokio::fs::metadata(&output_path).await {
        let output_bytes = metadata.len();
        ctx.state.set_output("size", &output_bytes.to_string());
        ctx.log.debug_event(evin_action_sdk::debug::ArchiveInputs {
            paths: compact_archive_paths(&report.inputs),
        });
        ctx.log.activity(evin_action_sdk::activity::ArchiveCreated {
            format: format_name.clone(),
            path: archive_path.to_string(),
            input_files: report.inputs.len(),
            input_size: format_byte_size(report.input_bytes),
            output_size: format_byte_size(output_bytes),
        });
        ctx.log.debug_event(evin_action_sdk::debug::ArchiveOutput {
            format: format_name,
            path: archive_path.to_string(),
            size_bytes: output_bytes,
            input_bytes: report.input_bytes,
            files: report.inputs.len(),
            compression_level,
        });
    }

    Ok(())
}

fn validate_format_keys(
    payload: &evin_flow::SpannedValue,
    format: PackageFormat,
) -> Result<(), evin_diagnostic::Diagnostic> {
    let evin_flow::SpannedValue::Mapping(entries, _) = payload else {
        return Ok(());
    };
    let keys = entries
        .iter()
        .filter_map(|(key, _)| match key {
            evin_flow::SpannedValue::String(value, _) => Some(value.as_str()),
            _ => None,
        })
        .collect::<std::collections::BTreeSet<_>>();
    let forbidden: &[&str] = match format {
        PackageFormat::TarGz => &[
            "architecture",
            "package",
            "release",
            "output_dir",
            "revision",
        ],
        PackageFormat::Deb | PackageFormat::Apk | PackageFormat::Arch => &[
            "path",
            "include",
            "exclude",
            "prefix",
            "layout",
            "compression_level",
            "revision",
        ],
    };
    if let Some(key) = forbidden.iter().find(|key| keys.contains(**key)) {
        return Err(diagnostic(evin_locales::t!(
            "archive-field-invalid-for-format",
            field = *key,
            format = format.as_str()
        )));
    }
    Ok(())
}

fn diagnostic(message: impl Into<String>) -> evin_diagnostic::Diagnostic {
    evin_locales::diagnostic(evin_diagnostic::IssueCode::custom(message.into()))
}

async fn resolve_epoch(ctx: &RunnerContext) -> Result<u64, evin_diagnostic::Diagnostic> {
    match ctx.env.get("SOURCE_DATE_EPOCH") {
        Some(value) => packaging::parse_source_date_epoch(value).map_err(|error| {
            diagnostic(evin_locales::t!(
                "archive-source-date-epoch-invalid",
                error = error.to_string()
            ))
        }),
        None => ctx.git.head_commit_epoch().await.map_err(|error| {
            diagnostic(evin_locales::t!(
                "archive-source-date-epoch-unavailable",
                error = error.to_string()
            ))
        }),
    }
}

#[cfg(test)]
mod tests;
