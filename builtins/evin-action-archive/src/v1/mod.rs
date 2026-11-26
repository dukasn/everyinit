// Copyright (C) 2026 Eduardo Marques Santos
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License, version 3.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

#![allow(clippy::result_large_err)]
mod action;
mod archiver;
pub mod config;
mod mapping;
mod packages;
mod reporting;
mod strategies;
pub use action::{ApkPackageAction, ArchPackageAction, ArchiveAction, DebPackageAction};

use archiver::Archiver;
use config::{ApkConfig, ArchConfig, DebConfig, PackageConfig, TarGzConfig};
use packages::{
    PACKAGE_ENCODERS, PackageBuild, PackageMetadata, load_scripts, package_filename,
    resolve_package_payloads, validate_apk, validate_arch, validate_deb, validate_package,
};
use reporting::{archive_input_report, compact_archive_paths, format_byte_size};

use evin_action_sdk::{
    RunnerContext,
    packaging::{self, PackageContext, PackageFormat, SourceEntry},
};
use evin_diagnostic::{Diagnostic, IssueCode};
use mapping::resolve_and_map_files;
use serde::de::DeserializeOwned;
use strategies::targz::TarGzArchiver;

pub fn validate_archive_config(
    with: Option<&evin_flow::SpannedValue>,
) -> Result<(), evin_flow::ConfigError> {
    let config: TarGzConfig = parse_config(with)?;
    config.validate().map_err(to_config_error)
}

pub fn validate_package_config<T: DeserializeOwned>(
    with: Option<&evin_flow::SpannedValue>,
    validate: impl FnOnce(&T) -> Result<(), Diagnostic>,
) -> Result<(), evin_flow::ConfigError> {
    let config: T = parse_config(with)?;
    validate(&config).map_err(to_config_error)?;
    Ok(())
}

pub fn validate_deb_config(
    with: Option<&evin_flow::SpannedValue>,
) -> Result<(), evin_flow::ConfigError> {
    validate_package_config(with, |config: &DebConfig| {
        config::validate_base_package_config(&config.output_dir, &config.release)?;
        validate_deb(config)
    })
}

pub fn validate_apk_config(
    with: Option<&evin_flow::SpannedValue>,
) -> Result<(), evin_flow::ConfigError> {
    validate_package_config(with, |config: &ApkConfig| {
        config::validate_base_package_config(&config.output_dir, &config.release)?;
        validate_apk(config)
    })
}

pub fn validate_arch_config(
    with: Option<&evin_flow::SpannedValue>,
) -> Result<(), evin_flow::ConfigError> {
    validate_package_config(with, |config: &ArchConfig| {
        config::validate_base_package_config(&config.output_dir, &config.release)?;
        validate_arch(config)
    })
}

pub fn validate_archive_task_config(
    _ctx: &evin_action_sdk::ActionValidationContext<'_>,
    with: Option<&evin_flow::SpannedValue>,
) -> Result<(), evin_flow::ConfigError> {
    validate_archive_config(with)
}

pub fn validate_deb_task_config(
    ctx: &evin_action_sdk::ActionValidationContext<'_>,
    with: Option<&evin_flow::SpannedValue>,
) -> Result<(), evin_flow::ConfigError> {
    validate_package_task_config(ctx, with, validate_deb_config)
}

pub fn validate_apk_task_config(
    ctx: &evin_action_sdk::ActionValidationContext<'_>,
    with: Option<&evin_flow::SpannedValue>,
) -> Result<(), evin_flow::ConfigError> {
    validate_package_task_config(ctx, with, validate_apk_config)
}

pub fn validate_arch_task_config(
    ctx: &evin_action_sdk::ActionValidationContext<'_>,
    with: Option<&evin_flow::SpannedValue>,
) -> Result<(), evin_flow::ConfigError> {
    validate_package_task_config(ctx, with, validate_arch_config)
}

pub fn validate_package_task_config(
    ctx: &evin_action_sdk::ActionValidationContext<'_>,
    with: Option<&evin_flow::SpannedValue>,
    validate: impl FnOnce(Option<&evin_flow::SpannedValue>) -> Result<(), evin_flow::ConfigError>,
) -> Result<(), evin_flow::ConfigError> {
    validate(with)?;
    let release = package_release(with)?;
    config::validate_release_reference(release, ctx.needs).map_err(|issue| evin_flow::ConfigError {
        message: evin_locales::render_issue(&issue),
        span: None,
    })
}

fn package_release(with: Option<&evin_flow::SpannedValue>) -> Result<&str, evin_flow::ConfigError> {
    fn find<'a>(
        value: &'a evin_flow::SpannedValue,
        key: &str,
    ) -> Option<&'a evin_flow::SpannedValue> {
        let evin_flow::SpannedValue::Mapping(entries, _) = value else {
            return None;
        };
        entries
            .iter()
            .find_map(|(candidate, value)| match candidate {
                evin_flow::SpannedValue::String(candidate, _) if candidate == key => Some(value),
                _ => None,
            })
    }
    let release = with.and_then(|with| find(with, "release"));
    match release {
        Some(evin_flow::SpannedValue::String(value, _)) => Ok(value),
        _ => Err(evin_flow::ConfigError {
            message: evin_locales::t!("invalid-config", error = "release is required"),
            span: None,
        }),
    }
}

fn parse_config<T: DeserializeOwned>(
    with: Option<&evin_flow::SpannedValue>,
) -> Result<T, evin_flow::ConfigError> {
    evin_action_sdk::parse_config(with).map_err(|error| {
        error.map_message(|message| evin_locales::t!("invalid-config", error = message))
    })
}

fn to_config_error(diagnostic: Diagnostic) -> evin_flow::ConfigError {
    evin_flow::ConfigError {
        message: evin_locales::render_issue(&diagnostic.code),
        span: None,
    }
}

pub async fn execute_archive(
    ctx: &RunnerContext,
    with: Option<evin_flow::SpannedValue>,
) -> Result<(), Diagnostic> {
    let config: TarGzConfig = ctx
        .load_config(with.as_ref())
        .map_err(|error| error.into_diagnostic(evin_locales::t!("invalid-config-title"), None))?;
    config.validate()?;
    let sources = resolve_and_map_files(ctx, &config)?
        .into_iter()
        .map(|entry| SourceEntry {
            source: entry.source,
            destination: entry.destination.to_string_lossy().replace('\\', "/"),
            mode: None,
        })
        .collect::<Vec<_>>();
    execute_artifact(
        ctx,
        ArtifactSpec {
            path: config.path,
            format: "tar.gz".into(),
            unresolved_inputs: config.include,
            sources,
            writer: ArtifactWriter::TarGz {
                compression_level: config.compression_level,
            },
        },
    )
    .await
}

pub async fn execute_deb(
    ctx: &RunnerContext,
    with: Option<evin_flow::SpannedValue>,
) -> Result<(), Diagnostic> {
    let config: DebConfig = load_package_config(ctx, with.as_ref())?;
    config::validate_base_package_config(&config.output_dir, &config.release)?;
    validate_deb(&config)?;
    execute_native_package(
        ctx,
        NativePackageSpec {
            package: config.package,
            output_dir: config.output_dir,
            architecture: config.architecture,
            release: config.release,
            format: PackageFormat::Deb,
            description: config.description,
            license: None,
            depends: config.depends,
            optdepends: Vec::new(),
            provides: Vec::new(),
            conflicts: Vec::new(),
            replaces: Vec::new(),
            backup: Vec::new(),
            scripts: vec![
                ("preinst", config.scripts.preinst),
                ("postinst", config.scripts.postinst),
                ("prerm", config.scripts.prerm),
                ("postrm", config.scripts.postrm),
            ],
        },
    )
    .await
}

pub async fn execute_apk(
    ctx: &RunnerContext,
    with: Option<evin_flow::SpannedValue>,
) -> Result<(), Diagnostic> {
    let config: ApkConfig = load_package_config(ctx, with.as_ref())?;
    config::validate_base_package_config(&config.output_dir, &config.release)?;
    validate_apk(&config)?;
    execute_native_package(
        ctx,
        NativePackageSpec {
            package: config.package,
            output_dir: config.output_dir,
            architecture: config.architecture,
            release: config.release,
            format: PackageFormat::Apk,
            description: None,
            license: Some(config.license),
            depends: config.depends,
            optdepends: Vec::new(),
            provides: Vec::new(),
            conflicts: Vec::new(),
            replaces: Vec::new(),
            backup: Vec::new(),
            scripts: vec![
                (".pre-install", config.scripts.pre_install),
                (".post-install", config.scripts.post_install),
                (".pre-deinstall", config.scripts.pre_deinstall),
                (".post-deinstall", config.scripts.post_deinstall),
            ],
        },
    )
    .await
}

pub async fn execute_arch(
    ctx: &RunnerContext,
    with: Option<evin_flow::SpannedValue>,
) -> Result<(), Diagnostic> {
    let config: ArchConfig = load_package_config(ctx, with.as_ref())?;
    config::validate_base_package_config(&config.output_dir, &config.release)?;
    validate_arch(&config)?;
    execute_native_package(
        ctx,
        NativePackageSpec {
            package: config.package,
            output_dir: config.output_dir,
            architecture: config.architecture,
            release: config.release,
            format: PackageFormat::Arch,
            description: None,
            license: Some(config.license),
            depends: config.depends,
            optdepends: config.optdepends,
            provides: config.provides,
            conflicts: config.conflicts,
            replaces: config.replaces,
            backup: config.backup,
            scripts: vec![(".INSTALL", config.scripts.install)],
        },
    )
    .await
}

fn load_package_config<T: DeserializeOwned>(
    ctx: &RunnerContext,
    with: Option<&evin_flow::SpannedValue>,
) -> Result<T, Diagnostic> {
    ctx.load_config(with).map_err(|error| {
        error.into_diagnostic(evin_locales::t!("package-invalid-config-title"), None)
    })
}

struct NativePackageSpec {
    package: PackageConfig,
    output_dir: String,
    architecture: evin_action_sdk::packaging::LinuxArch,
    release: String,
    format: PackageFormat,
    description: Option<String>,
    license: Option<String>,
    depends: Vec<String>,
    optdepends: Vec<String>,
    provides: Vec<String>,
    conflicts: Vec<String>,
    replaces: Vec<String>,
    backup: Vec<String>,
    scripts: Vec<(&'static str, Option<String>)>,
}

async fn execute_native_package(
    ctx: &RunnerContext,
    spec: NativePackageSpec,
) -> Result<(), Diagnostic> {
    let NativePackageSpec {
        package,
        output_dir,
        architecture,
        release,
        format,
        description,
        license,
        depends,
        optdepends,
        provides,
        conflicts,
        replaces,
        backup,
        scripts: script_definitions,
    } = spec;
    validate_package(&package)?;
    let artifact: evin_action_sdk::release::ReleaseArtifact = serde_json::from_str(&release)
        .map_err(|error| {
            diagnostic(
                IssueCode::new("archive-release-output-invalid").arg("error", error.to_string()),
            )
        })?;
    if artifact.schema != 1 {
        return Err(diagnostic(
            IssueCode::new("archive-release-schema-unsupported").arg("schema", artifact.schema),
        ));
    }
    let target_name = format.as_str();
    let release_state = artifact
        .packages
        .get(&package.name)
        .and_then(|package| package.revisions.get(target_name))
        .ok_or_else(|| {
            diagnostic(
                IssueCode::new("archive-release-revision-missing")
                    .arg("target", target_name)
                    .arg("package", package.name.as_str()),
            )
        })?;
    if release_state.revision == 0 || release_state.version.trim().is_empty() {
        return Err(diagnostic(IssueCode::new(
            "archive-release-version-revision-invalid",
        )));
    }
    let arch = match format {
        PackageFormat::Deb => architecture.deb(),
        PackageFormat::Apk => architecture.apk(),
        PackageFormat::Arch => architecture.arch(),
        PackageFormat::TarGz => unreachable!(),
    }
    .to_string();
    let filename = package_filename(
        format,
        &package.name,
        &release_state.version,
        release_state.revision,
        &arch,
    );
    let output_path = format!("{}/{}", output_dir.trim_end_matches('/'), filename);
    let sources = resolve_package_payloads(ctx, &package)?;
    let metadata = PackageMetadata {
        package,
        format,
        arch,
        revision: release_state.revision,
        version: release_state.version.clone(),
        description,
        license,
        depends,
        optdepends,
        provides,
        conflicts,
        replaces,
        backup,
        scripts: Vec::new(),
    };
    let scripts = script_definitions
        .into_iter()
        .filter_map(|(name, path)| path.map(|path| (name.to_string(), path)))
        .collect();
    execute_artifact(
        ctx,
        ArtifactSpec {
            path: output_path,
            format: format.as_str().into(),
            unresolved_inputs: Vec::new(),
            sources,
            writer: ArtifactWriter::NativePackage {
                metadata: Box::new(metadata),
                script_definitions: scripts,
            },
        },
    )
    .await
}

struct ArtifactSpec {
    path: String,
    format: String,
    unresolved_inputs: Vec<String>,
    sources: Vec<SourceEntry>,
    writer: ArtifactWriter,
}

enum ArtifactWriter {
    TarGz {
        compression_level: Option<u32>,
    },
    NativePackage {
        metadata: Box<PackageMetadata>,
        script_definitions: Vec<(String, String)>,
    },
}

impl ArtifactWriter {
    fn compression_level(&self) -> Option<u32> {
        match self {
            Self::TarGz { compression_level } => *compression_level,
            Self::NativePackage { .. } => None,
        }
    }
}

async fn execute_artifact(ctx: &RunnerContext, spec: ArtifactSpec) -> Result<(), Diagnostic> {
    let ArtifactSpec {
        path: artifact_path,
        format,
        unresolved_inputs,
        sources: source_entries,
        mut writer,
    } = spec;
    let compression_level = writer.compression_level();
    if source_entries.is_empty() {
        if ctx.mode == evin_action_sdk::PipelineMode::DryRun && !ctx.uncertainty().is_empty() {
            ctx.mark_unverified(evin_locales::t!(
                "archive-dry-run-inputs-unverified",
                includes = unresolved_inputs.join(", ")
            ));
            ctx.log
                .plan(evin_action_sdk::activity::ArchivePlanUnresolved {
                    path: artifact_path.clone(),
                });
            ctx.state.set_output("path", &artifact_path);
            ctx.state.set_output("size", "unknown");
            return Ok(());
        }
        return Err(evin_locales::diagnostic(IssueCode::new("no-files-matched")));
    }

    let output_path = ctx
        .fs
        .get_safe_physical_path(&artifact_path)
        .map_err(|error| {
            diagnostic(
                IssueCode::new("issue-archive-safe-path-failed")
                    .arg("context", "artifact path")
                    .arg("error", error.to_string()),
            )
        })?;
    if ctx.mode == evin_action_sdk::PipelineMode::DryRun {
        ctx.log.plan(evin_action_sdk::activity::ArchivePlan {
            files: source_entries.len(),
            path: artifact_path.clone(),
        });
        ctx.state.set_output("path", &artifact_path);
        ctx.state.set_output("size", "0");
        return Ok(());
    }
    if let Some(parent) = output_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        tokio::fs::create_dir_all(parent).await.map_err(|error| {
            diagnostic(IssueCode::new("create-output-dir-failed").arg("error", error.to_string()))
        })?;
    }

    evin_action_sdk::action_event!(
        ctx.log,
        evin_action_sdk::activity::ArchiveCompressing {
            files: source_entries.len(),
            path: artifact_path.to_string(),
        }
    );
    let epoch = resolve_epoch(ctx).await?;
    if let ArtifactWriter::NativePackage {
        metadata,
        script_definitions,
    } = &mut writer
    {
        metadata.scripts = load_scripts(
            ctx,
            script_definitions
                .iter()
                .map(|(name, path)| (name.as_str(), Some(path)))
                .collect(),
        )?;
    }
    let write_path = output_path.clone();
    let report = tokio::task::spawn_blocking(move || {
        let context = PackageContext::prepare(epoch, source_entries).map_err(Diagnostic::from)?;
        let report = archive_input_report(&context);
        packaging::write_atomically(&write_path, |output| match writer {
            ArtifactWriter::NativePackage { metadata, .. } => {
                let build = PackageBuild {
                    context,
                    metadata: *metadata,
                };
                let encoder = PACKAGE_ENCODERS
                    .iter()
                    .find(|encoder| encoder.format == build.metadata.format)
                    .expect("fixed package strategy LUT");
                (encoder.write)(output, &build)
            }
            ArtifactWriter::TarGz { compression_level } => {
                TarGzArchiver { compression_level }.write_archive(output, &context)
            }
        })
        .map_err(Diagnostic::from)?;
        Ok::<_, Diagnostic>(report)
    })
    .await
    .map_err(|error| {
        diagnostic(IssueCode::new("archive-blocking-task-failed").arg("error", error.to_string()))
    })??;

    ctx.state.set_output("path", &artifact_path);
    if let Ok(metadata) = tokio::fs::metadata(&output_path).await {
        let output_bytes = metadata.len();
        ctx.state.set_output("size", &output_bytes.to_string());
        ctx.log.debug_event(evin_action_sdk::debug::ArchiveInputs {
            paths: compact_archive_paths(&report.inputs),
        });
        ctx.log.activity(evin_action_sdk::activity::ArchiveCreated {
            format: format.clone(),
            path: artifact_path.clone(),
            input_files: report.inputs.len(),
            input_size: format_byte_size(report.input_bytes),
            output_size: format_byte_size(output_bytes),
        });
        ctx.log.debug_event(evin_action_sdk::debug::ArchiveOutput {
            format: format.clone(),
            path: artifact_path,
            size_bytes: output_bytes,
            input_bytes: report.input_bytes,
            files: report.inputs.len(),
            compression_level,
        });
    }
    Ok(())
}

fn diagnostic(code: IssueCode) -> Diagnostic {
    evin_locales::diagnostic(code)
}

async fn resolve_epoch(ctx: &RunnerContext) -> Result<u64, Diagnostic> {
    match ctx.env.get("SOURCE_DATE_EPOCH") {
        Some(value) => packaging::parse_source_date_epoch(value).map_err(|error| {
            diagnostic(
                IssueCode::new("archive-source-date-epoch-invalid").arg("error", error.to_string()),
            )
        }),
        None => ctx.git.head_commit_epoch().await.map_err(|error| {
            diagnostic(
                IssueCode::new("archive-source-date-epoch-unavailable")
                    .arg("error", error.to_string()),
            )
        }),
    }
}

#[cfg(test)]
mod tests;
