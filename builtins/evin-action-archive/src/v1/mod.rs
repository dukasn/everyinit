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
pub mod archiver;
pub mod config;
pub mod strategies;

use archiver::{ArchiveEntry, Archiver};
use config::{ArchiveConfig, LayoutMode};
use strategies::{apk, deb, targz::TarGzArchiver};

use evin_action_sdk::{
    RunnerContext,
    packaging::{self, PackageContext, PackageFormat, PackageTokens, SourceEntry},
};
use evin_yaml::template::pipeline::resolve_templates;
use glob::Pattern;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

struct PackageBuild {
    context: PackageContext,
    manifest: evin_action_sdk::packaging::PackageManifest,
    format: PackageFormat,
    arch: String,
    revision: u32,
    version: String,
    scripts: Vec<(String, Vec<u8>)>,
}

struct PackageEncoder {
    format: PackageFormat,
    write: fn(&Path, &PackageBuild) -> Result<(), packaging::PackagingError>,
}

const PACKAGE_ENCODERS: [PackageEncoder; 2] = [
    PackageEncoder {
        format: PackageFormat::Deb,
        write: write_deb,
    },
    PackageEncoder {
        format: PackageFormat::Apk,
        write: write_apk,
    },
];

pub fn validate_config(
    with_payload: Option<&evin_yaml::SpannedValue>,
) -> Result<(), evin_yaml::ConfigError> {
    let payload = with_payload
        .cloned()
        .unwrap_or(evin_yaml::SpannedValue::Null(Default::default()));
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
        .map_err(|d| evin_yaml::ConfigError {
            message: evin_locales::render_issue(&d.code),
            span: None,
        })
}

pub fn validate_task_config(
    ctx: &evin_action_sdk::ActionValidationContext<'_>,
    with_payload: Option<&evin_yaml::SpannedValue>,
) -> Result<(), evin_yaml::ConfigError> {
    validate_config(with_payload)?;
    let config: ArchiveConfig = evin_action_sdk::parse_config(with_payload)?;
    let Some(release) = config.release else {
        return Ok(());
    };
    let task = release_reference_task(&release).ok_or_else(|| evin_yaml::ConfigError {
        message: "release must be exactly '${task.release}'".into(),
        span: None,
    })?;
    if !ctx.needs.iter().any(|need| need == task) {
        return Err(evin_yaml::ConfigError {
            message: format!("release references '{task}', but that task is not declared in needs"),
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
    with_payload: Option<evin_yaml::SpannedValue>,
) -> Result<(), evin_diagnostic::Diagnostic> {
    let mut config: ArchiveConfig =
        evin_action_sdk::parse_config(with_payload.as_ref()).map_err(|e| {
            evin_locales::diagnostic(
                evin_diagnostic::IssueCode::new("invalid-config").arg("error", e.to_string()),
            )
        })?;
    config.format = resolve_action_templates(&config.format, ctx)?;
    config.path = resolve_action_templates(&config.path, ctx)
        .map_err(|e| evin_locales::diagnostic(evin_diagnostic::IssueCode::custom(e.to_string())))?;
    if let Some(prefix) = &config.prefix {
        config.prefix = Some(resolve_action_templates(prefix, ctx).map_err(|e| {
            evin_locales::diagnostic(evin_diagnostic::IssueCode::custom(e.to_string()))
        })?);
    }
    config.include = config
        .include
        .as_ref()
        .map(|include| {
            include
                .iter()
                .map(|pattern| {
                    resolve_action_templates(pattern, ctx).map_err(|e| {
                        evin_locales::diagnostic(evin_diagnostic::IssueCode::custom(e.to_string()))
                    })
                })
                .collect::<Result<_, _>>()
        })
        .transpose()?;
    config.exclude = config
        .exclude
        .iter()
        .map(|pattern| {
            resolve_action_templates(pattern, ctx).map_err(|e| {
                evin_locales::diagnostic(evin_diagnostic::IssueCode::custom(e.to_string()))
            })
        })
        .collect::<Result<_, _>>()?;
    config.validate()?;
    validate_format_keys(
        with_payload
            .as_ref()
            .unwrap_or(&evin_yaml::SpannedValue::Null(Default::default())),
        PackageFormat::parse(&config.format).expect("validated format"),
    )?;

    let format = PackageFormat::parse(&config.format).expect("validated format");
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
        let selected = ctx
            .package_manifests
            .select(ctx.fs.task_cwd())
            .ok_or_else(|| diagnostic("no .evin/package.yaml applies to this packaging task"))?;
        let mut manifest = selected.manifest.clone();
        let architecture = config.architecture.expect("validated architecture");
        match format {
            PackageFormat::Deb if manifest.targets.deb.is_none() => {
                return Err(diagnostic("package manifest has no deb target"));
            }
            PackageFormat::Apk if manifest.targets.apk.is_none() => {
                return Err(diagnostic("package manifest has no apk target"));
            }
            _ => {}
        }
        let package_id = config.package.as_deref().expect("validated package");
        if manifest.identity.name != package_id {
            return Err(diagnostic(format!(
                "release package '{package_id}' does not match package manifest identity '{}'",
                manifest.identity.name
            )));
        }
        let reference = config.release.as_deref().expect("validated release");
        let upstream = release_reference_task(reference)
            .ok_or_else(|| diagnostic("release must be exactly '${task.release}'"))?;
        let payload = ctx.state.get_input(upstream, "release").ok_or_else(|| {
            diagnostic(format!(
                "prerequisite '{upstream}' did not publish release output"
            ))
        })?;
        let artifact: evin_action_sdk::release::ReleaseArtifact = serde_json::from_str(&payload)
            .map_err(|error| diagnostic(format!("invalid release output: {error}")))?;
        if artifact.schema != 1 {
            return Err(diagnostic(format!(
                "unsupported release schema {}",
                artifact.schema
            )));
        }
        let target_name = format.as_str();
        let release_state = artifact
            .packages
            .get(package_id)
            .and_then(|package| package.revisions.get(target_name))
            .ok_or_else(|| {
                diagnostic(format!(
                    "release has no resolved '{target_name}' revision for package '{package_id}'"
                ))
            })?;
        if release_state.revision == 0 || release_state.version.trim().is_empty() {
            return Err(diagnostic(
                "release contains an invalid version or revision",
            ));
        }
        let revision = release_state.revision;
        let arch = match format {
            PackageFormat::Deb => architecture.deb().to_string(),
            PackageFormat::Apk => architecture.apk().to_string(),
            PackageFormat::TarGz => unreachable!(),
        };
        let mut tokens = PackageTokens {
            name: manifest.identity.name.clone(),
            version: release_state.version.clone(),
            arch: arch.clone(),
            revision,
            maintainer: manifest.identity.maintainer.clone(),
            summary: manifest.identity.summary.clone(),
        };
        render_manifest_metadata(&mut manifest, &mut tokens)?;
        config.path = packaging::render_intrinsic_tokens(&config.path, &tokens)
            .map_err(|error| diagnostic(error.to_string()))?;
        let sources = resolve_package_payloads(ctx, selected.root.as_path(), &manifest, &tokens)?;
        package_build = Some((manifest, tokens, arch, revision, selected.root.clone()));
        sources
    };
    if source_entries.is_empty() {
        if ctx.mode == evin_action_sdk::PipelineMode::DryRun && !ctx.uncertainty().is_empty() {
            ctx.mark_unverified(format!(
                "Archive inputs could not be verified: {}",
                config.include.as_deref().unwrap_or_default().join(", ")
            ));
            ctx.log
                .plan(evin_action_sdk::activity::ArchivePlanUnresolved {
                    path: config.path.clone(),
                });
            ctx.state.set_output("path", &config.path);
            ctx.state.set_output("size", "unknown");
            return Ok(());
        }
        return Err(evin_locales::diagnostic(evin_diagnostic::IssueCode::new(
            "no-files-matched",
        )));
    }

    let output_path = ctx.fs.get_safe_physical_path(&config.path).map_err(|e| {
        evin_locales::diagnostic(
            evin_diagnostic::IssueCode::new("issue-archive-safe-path-failed")
                .arg("context", "archive path")
                .arg("error", e.to_string()),
        )
    })?;

    if ctx.mode == evin_action_sdk::PipelineMode::DryRun {
        ctx.log.plan(evin_action_sdk::activity::ArchivePlan {
            files: source_entries.len(),
            path: config.path.clone(),
        });
        ctx.state.set_output("path", &config.path);
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
            path: config.path.clone(),
        }
    );
    let epoch = resolve_epoch(ctx).await?;
    let files_len = source_entries.len();
    let build = if let Some((manifest, tokens, arch, revision, root)) = package_build {
        let context = PackageContext::prepare(epoch, source_entries.clone())
            .map_err(|error| diagnostic(error.to_string()))?;
        let scripts = load_scripts(&manifest, &tokens, format, &root)?;
        Some(PackageBuild {
            context,
            manifest,
            format,
            arch,
            revision,
            version: tokens.version.clone(),
            scripts,
        })
    } else {
        None
    };
    tokio::task::spawn_blocking(move || {
        if let Some(build) = build {
            let encoder = PACKAGE_ENCODERS
                .iter()
                .find(|encoder| encoder.format == build.format)
                .expect("fixed package strategy LUT");
            (encoder.write)(&output_path, &build).map_err(|error| diagnostic(error.to_string()))
        } else {
            let context = PackageContext::prepare(epoch, source_entries)
                .map_err(|error| diagnostic(error.to_string()))?;
            TarGzArchiver {
                compression_level: config.compression_level,
            }
            .create_archive(&output_path, &context)
        }
    })
    .await
    .map_err(|e| {
        evin_locales::diagnostic(evin_diagnostic::IssueCode::custom(format!(
            "Blocking task failed: {}",
            e
        )))
    })??;

    ctx.state.set_output("path", &config.path);
    if let Ok(metadata) =
        tokio::fs::metadata(ctx.fs.get_safe_physical_path(&config.path).unwrap()).await
    {
        ctx.state.set_output("size", &metadata.len().to_string());
        ctx.log.debug_event(evin_action_sdk::debug::ArchiveOutput {
            format: config.format.clone(),
            size_bytes: metadata.len(),
            files: files_len,
        });
    }

    Ok(())
}

fn validate_format_keys(
    payload: &evin_yaml::SpannedValue,
    format: PackageFormat,
) -> Result<(), evin_diagnostic::Diagnostic> {
    let evin_yaml::SpannedValue::Mapping(entries, _) = payload else {
        return Ok(());
    };
    let keys = entries
        .iter()
        .filter_map(|(key, _)| match key {
            evin_yaml::SpannedValue::String(value, _) => Some(value.as_str()),
            _ => None,
        })
        .collect::<std::collections::BTreeSet<_>>();
    let forbidden: &[&str] = match format {
        PackageFormat::TarGz => &["architecture", "package", "release", "revision"],
        PackageFormat::Deb | PackageFormat::Apk => &[
            "include",
            "exclude",
            "prefix",
            "layout",
            "compression_level",
            "revision",
        ],
    };
    if let Some(key) = forbidden.iter().find(|key| keys.contains(**key)) {
        return Err(diagnostic(format!(
            "'{key}' is not valid when format is '{}'",
            format.as_str()
        )));
    }
    Ok(())
}

fn resolve_action_templates(
    value: &str,
    ctx: &RunnerContext,
) -> Result<String, evin_diagnostic::Diagnostic> {
    const OPEN: &str = "__EVIN_INTRINSIC_OPEN__";
    const CLOSE: &str = "__EVIN_INTRINSIC_CLOSE__";
    let protected = value.replace("{{", OPEN).replace("}}", CLOSE);
    resolve_templates(&protected, ctx)
        .map(|rendered| rendered.replace(OPEN, "{{").replace(CLOSE, "}}"))
        .map_err(|e| diagnostic(e.to_string()))
}

fn diagnostic(message: impl Into<String>) -> evin_diagnostic::Diagnostic {
    evin_locales::diagnostic(evin_diagnostic::IssueCode::custom(message.into()))
}

async fn resolve_epoch(ctx: &RunnerContext) -> Result<u64, evin_diagnostic::Diagnostic> {
    match ctx.env.get("SOURCE_DATE_EPOCH") {
        Some(value) => {
            packaging::parse_source_date_epoch(value).map_err(|error| diagnostic(error.to_string()))
        }
        None => ctx.git.head_commit_epoch().await.map_err(|error| {
            diagnostic(format!(
                "SOURCE_DATE_EPOCH is absent and Git HEAD timestamp is unavailable: {error}"
            ))
        }),
    }
}

fn resolve_package_payloads(
    ctx: &RunnerContext,
    root: &Path,
    manifest: &evin_action_sdk::packaging::PackageManifest,
    tokens: &PackageTokens,
) -> Result<Vec<SourceEntry>, evin_diagnostic::Diagnostic> {
    let mut entries = BTreeMap::<String, SourceEntry>::new();
    for mapping in &manifest.payload {
        let source = packaging::render_intrinsic_tokens(&mapping.source, tokens)
            .map_err(|error| diagnostic(error.to_string()))?;
        let strip_prefix = packaging::render_intrinsic_tokens(&mapping.strip_prefix, tokens)
            .map_err(|error| diagnostic(error.to_string()))?;
        let destination = packaging::render_intrinsic_tokens(&mapping.destination, tokens)
            .map_err(|error| diagnostic(error.to_string()))?;
        let strip = packaging::safe_relative_path(&strip_prefix)
            .map_err(|error| diagnostic(error.to_string()))?;
        let destination = packaging::normalize_install_dir(&destination)
            .map_err(|error| diagnostic(error.to_string()))?;
        let strip = root.join(strip);
        for source_path in ctx
            .fs
            .matching_files_from(root, &source)
            .map_err(|error| diagnostic(error.to_string()))?
        {
            let relative = source_path.strip_prefix(&strip).map_err(|_| {
                diagnostic(format!(
                    "payload '{}' does not lie below strip_prefix '{}'",
                    source_path.display(),
                    strip.display()
                ))
            })?;
            let destination = format!(
                "{destination}/{}",
                relative.to_string_lossy().replace('\\', "/")
            );
            let destination = packaging::normalize_archive_path(&destination)
                .map_err(|error| diagnostic(error.to_string()))?;
            if entries
                .insert(
                    destination.clone(),
                    SourceEntry {
                        source: source_path,
                        destination: destination.clone(),
                        mode: mapping.mode,
                    },
                )
                .is_some()
            {
                return Err(diagnostic(format!(
                    "package payload collision at '{destination}'"
                )));
            }
        }
    }
    Ok(entries.into_values().collect())
}

fn load_scripts(
    manifest: &evin_action_sdk::packaging::PackageManifest,
    tokens: &PackageTokens,
    format: PackageFormat,
    root: &Path,
) -> Result<Vec<(String, Vec<u8>)>, evin_diagnostic::Diagnostic> {
    let definitions: Vec<(&str, Option<&String>)> = match format {
        PackageFormat::Deb => {
            let scripts = &manifest
                .targets
                .deb
                .as_ref()
                .expect("validated target")
                .scripts;
            vec![
                ("preinst", scripts.preinst.as_ref()),
                ("postinst", scripts.postinst.as_ref()),
                ("prerm", scripts.prerm.as_ref()),
                ("postrm", scripts.postrm.as_ref()),
            ]
        }
        PackageFormat::Apk => {
            let scripts = &manifest
                .targets
                .apk
                .as_ref()
                .expect("validated target")
                .scripts;
            vec![
                (".pre-install", scripts.pre_install.as_ref()),
                (".post-install", scripts.post_install.as_ref()),
                (".pre-deinstall", scripts.pre_deinstall.as_ref()),
                (".post-deinstall", scripts.post_deinstall.as_ref()),
            ]
        }
        PackageFormat::TarGz => Vec::new(),
    };
    definitions
        .into_iter()
        .filter_map(|(name, path)| path.map(|path| (name, path)))
        .map(|(name, path)| {
            let path = packaging::render_intrinsic_tokens(path, tokens)
                .map_err(|error| diagnostic(error.to_string()))?;
            let path = packaging::safe_relative_path(&path)
                .map_err(|error| diagnostic(error.to_string()))?;
            let bytes = std::fs::read(root.join(path))
                .map_err(|error| diagnostic(format!("failed to read package script: {error}")))?;
            let text = std::str::from_utf8(&bytes)
                .map_err(|_| diagnostic("package scripts must be UTF-8 text"))?;
            Ok((
                name.to_string(),
                packaging::render_intrinsic_tokens(text, tokens)
                    .map_err(|error| diagnostic(error.to_string()))?
                    .into_bytes(),
            ))
        })
        .collect()
}

fn render_manifest_metadata(
    manifest: &mut evin_action_sdk::packaging::PackageManifest,
    tokens: &mut PackageTokens,
) -> Result<(), evin_diagnostic::Diagnostic> {
    manifest.identity.summary =
        packaging::render_intrinsic_tokens(&manifest.identity.summary, tokens)
            .map_err(|error| diagnostic(error.to_string()))?;
    tokens.summary = manifest.identity.summary.clone();
    manifest.identity.maintainer =
        packaging::render_intrinsic_tokens(&manifest.identity.maintainer, tokens)
            .map_err(|error| diagnostic(error.to_string()))?;
    tokens.maintainer = manifest.identity.maintainer.clone();
    manifest.identity.license =
        packaging::render_intrinsic_tokens(&manifest.identity.license, tokens)
            .map_err(|error| diagnostic(error.to_string()))?;
    if let Some(description) = &mut manifest.identity.description {
        *description = packaging::render_intrinsic_tokens(description, tokens)
            .map_err(|error| diagnostic(error.to_string()))?;
    }
    if let Some(homepage) = &mut manifest.identity.homepage {
        *homepage = packaging::render_intrinsic_tokens(homepage, tokens)
            .map_err(|error| diagnostic(error.to_string()))?;
    }
    if let Some(target) = &mut manifest.targets.deb {
        for dependency in &mut target.depends {
            *dependency = packaging::render_intrinsic_tokens(dependency, tokens)
                .map_err(|error| diagnostic(error.to_string()))?;
        }
    }
    if let Some(target) = &mut manifest.targets.apk {
        for dependency in &mut target.depends {
            *dependency = packaging::render_intrinsic_tokens(dependency, tokens)
                .map_err(|error| diagnostic(error.to_string()))?;
        }
    }
    Ok(())
}

fn write_deb(output: &Path, build: &PackageBuild) -> Result<(), packaging::PackagingError> {
    let target = build
        .manifest
        .targets
        .deb
        .as_ref()
        .expect("fixed encoder target");
    deb::write(
        output,
        deb::DebPackage {
            context: &build.context,
            name: &build.manifest.identity.name,
            version: &build.version,
            revision: build.revision,
            architecture: &build.arch,
            summary: &build.manifest.identity.summary,
            description: build.manifest.identity.description.as_deref(),
            maintainer: &build.manifest.identity.maintainer,
            homepage: build.manifest.identity.homepage.as_deref(),
            depends: target.depends.clone(),
            scripts: build.scripts.clone(),
        },
    )
}

fn write_apk(output: &Path, build: &PackageBuild) -> Result<(), packaging::PackagingError> {
    let target = build
        .manifest
        .targets
        .apk
        .as_ref()
        .expect("fixed encoder target");
    apk::write(
        output,
        apk::ApkPackage {
            context: &build.context,
            name: &build.manifest.identity.name,
            version: &build.version,
            revision: build.revision,
            architecture: &build.arch,
            summary: &build.manifest.identity.summary,
            homepage: build.manifest.identity.homepage.as_deref(),
            maintainer: &build.manifest.identity.maintainer,
            license: &build.manifest.identity.license,
            depends: target.depends.clone(),
            scripts: build.scripts.clone(),
        },
    )
}

fn resolve_and_map_files(
    ctx: &RunnerContext,
    config: &ArchiveConfig,
) -> Result<Vec<ArchiveEntry>, evin_diagnostic::Diagnostic> {
    let mut mapped_files: BTreeMap<PathBuf, ArchiveEntry> = BTreeMap::new();
    let prefix = config.prefix.as_deref().unwrap_or("");

    let mut exclude_patterns = Vec::new();
    for ex in &config.exclude {
        exclude_patterns.push(Pattern::new(ex).map_err(|e| {
            evin_locales::diagnostic(
                evin_diagnostic::IssueCode::new("invalid-glob").arg("error", e.to_string()),
            )
        })?);
    }

    for pattern in config.include.as_deref().unwrap_or_default() {
        for path in ctx.fs.matching_files(pattern).map_err(|e| {
            evin_locales::diagnostic(
                evin_diagnostic::IssueCode::new("issue-archive-safe-path-failed")
                    .arg("context", "source")
                    .arg("error", e.to_string()),
            )
        })? {
            let rel_path = match path.strip_prefix(ctx.fs.task_cwd()) {
                Ok(p) => p,
                Err(_) => {
                    if config.layout == LayoutMode::Relative {
                        return Err(evin_locales::diagnostic(
                            evin_diagnostic::IssueCode::new("relative-path-escape")
                                .arg("path", path.display().to_string()),
                        ));
                    }
                    path.strip_prefix(ctx.fs.workspace_root()).unwrap_or(&path)
                }
            };

            if exclude_patterns.iter().any(|p| p.matches_path(rel_path)) {
                continue;
            }

            // Apply Flatten & Prefix logic
            let dest_path = if config.layout == LayoutMode::Flat {
                let file_name = path.file_name().ok_or_else(|| {
                    evin_locales::diagnostic(
                        evin_diagnostic::IssueCode::new("invalid-file")
                            .arg("path", path.display().to_string()),
                    )
                })?;
                PathBuf::from(prefix).join(file_name)
            } else {
                PathBuf::from(prefix).join(rel_path)
            };

            let safe_source = ctx
                .fs
                .get_safe_physical_path(&path.to_string_lossy())
                .map_err(|e| {
                    evin_locales::diagnostic(
                        evin_diagnostic::IssueCode::new("issue-archive-safe-path-failed")
                            .arg("context", "source")
                            .arg("error", e.to_string()),
                    )
                })?;

            if let Some(existing) = mapped_files.get(&dest_path) {
                if existing.source != safe_source {
                    return Err(evin_locales::diagnostic(
                        evin_diagnostic::IssueCode::new("path-collision")
                            .arg("destination", dest_path.display().to_string())
                            .arg("source1", existing.source.display().to_string())
                            .arg("source2", safe_source.display().to_string()),
                    ));
                }
            } else {
                mapped_files.insert(
                    dest_path.clone(),
                    ArchiveEntry {
                        source: safe_source,
                        destination: dest_path,
                    },
                );
            }
        }
    }

    Ok(mapped_files.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ArchiveAction;
    use evin_action_sdk::Action;
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn spanned_value(value: serde_json::Value) -> evin_yaml::SpannedValue {
        let span = evin_yaml::Span::default();
        match value {
            serde_json::Value::Null => evin_yaml::SpannedValue::Null(span),
            serde_json::Value::Bool(value) => evin_yaml::SpannedValue::Bool(value, span),
            serde_json::Value::Number(value) => {
                if let Some(value) = value.as_i64() {
                    evin_yaml::SpannedValue::Integer(value, span)
                } else {
                    evin_yaml::SpannedValue::Float(value.as_f64().unwrap_or_default(), span)
                }
            }
            serde_json::Value::String(value) => evin_yaml::SpannedValue::String(value, span),
            serde_json::Value::Array(values) => evin_yaml::SpannedValue::Sequence(
                values.into_iter().map(spanned_value).collect(),
                span,
            ),
            serde_json::Value::Object(values) => evin_yaml::SpannedValue::Mapping(
                values
                    .into_iter()
                    .map(|(key, value)| {
                        (
                            evin_yaml::SpannedValue::String(key, span),
                            spanned_value(value),
                        )
                    })
                    .collect(),
                span,
            ),
        }
    }

    fn spanned_mapping(with: HashMap<String, serde_json::Value>) -> evin_yaml::SpannedValue {
        evin_yaml::SpannedValue::Mapping(
            with.into_iter()
                .map(|(key, value)| {
                    (
                        evin_yaml::SpannedValue::String(key, evin_yaml::Span::default()),
                        spanned_value(value),
                    )
                })
                .collect(),
            evin_yaml::Span::default(),
        )
    }

    #[tokio::test]
    async fn test_archive_creates_archive() {
        let temp_dir = TempDir::new().unwrap();
        let target_dir = temp_dir.path().join("target");
        let output_tar = temp_dir.path().join("out.tar.gz");

        std::fs::create_dir_all(&target_dir).unwrap();
        std::fs::write(target_dir.join("file1.txt"), "hello").unwrap();
        std::fs::write(target_dir.join("file2.txt"), "world").unwrap();

        let mut with = HashMap::new();
        with.insert(
            "path".to_string(),
            serde_json::Value::String(output_tar.to_string_lossy().to_string()),
        );
        with.insert(
            "include".to_string(),
            serde_json::Value::Array(vec![serde_json::Value::String("target/*.txt".to_string())]),
        );
        with.insert(
            "layout".to_string(),
            serde_json::Value::String("flat".to_string()),
        );

        let (log_tx, _) = tokio::sync::mpsc::unbounded_channel();
        let ctx = RunnerContext::new(
            None,
            "archive_test".to_string(),
            false,
            evin_action_sdk::PipelineMode::Normal,
            log_tx,
            vec![],
            Some(temp_dir.path().to_string_lossy().to_string()),
            std::collections::HashMap::from([("SOURCE_DATE_EPOCH".to_string(), "1".to_string())]),
            temp_dir.path().to_path_buf(),
            std::collections::HashMap::new(),
            std::sync::Arc::new(evin_action_sdk::git::GitProvider::new(
                temp_dir.path().to_path_buf(),
                evin_action_sdk::PipelineMode::Normal,
                evin_action_sdk::Logger::new(
                    "test".to_string(),
                    tokio::sync::mpsc::unbounded_channel().0,
                    std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default()),
                ),
            )),
            std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default()),
        );

        let action = ArchiveAction;
        let res = action.execute(&ctx, Some(spanned_mapping(with))).await;

        assert!(res.is_ok(), "Archive execution failed: {:?}", res);
        assert!(output_tar.exists(), "Archive was not created!");
        let outputs = ctx.state.outputs.lock().unwrap();
        assert_eq!(
            outputs.get("path"),
            Some(&output_tar.to_string_lossy().to_string())
        );
        assert!(outputs.contains_key("size"));
        assert!(!outputs.contains_key("archive_path"));
        assert!(!outputs.contains_key("archive_size"));
    }

    #[tokio::test]
    async fn test_archive_collision() {
        let temp_dir = TempDir::new().unwrap();
        let dir_a = temp_dir.path().join("a");
        let dir_b = temp_dir.path().join("b");
        let output_tar = temp_dir.path().join("out.tar.gz");

        std::fs::create_dir_all(&dir_a).unwrap();
        std::fs::create_dir_all(&dir_b).unwrap();
        std::fs::write(dir_a.join("file.txt"), "hello").unwrap();
        std::fs::write(dir_b.join("file.txt"), "world").unwrap();

        let mut with = HashMap::new();
        with.insert(
            "path".to_string(),
            serde_json::Value::String(output_tar.to_string_lossy().to_string()),
        );
        with.insert(
            "include".to_string(),
            serde_json::Value::Array(vec![
                serde_json::Value::String("a/*.txt".to_string()),
                serde_json::Value::String("b/*.txt".to_string()),
            ]),
        );
        with.insert(
            "layout".to_string(),
            serde_json::Value::String("flat".to_string()),
        );

        let (log_tx, _) = tokio::sync::mpsc::unbounded_channel();
        let ctx = RunnerContext::new(
            None,
            "archive_test".to_string(),
            false,
            evin_action_sdk::PipelineMode::Normal,
            log_tx,
            vec![],
            Some(temp_dir.path().to_string_lossy().to_string()),
            std::collections::HashMap::from([("SOURCE_DATE_EPOCH".to_string(), "1".to_string())]),
            temp_dir.path().to_path_buf(),
            std::collections::HashMap::new(),
            std::sync::Arc::new(evin_action_sdk::git::GitProvider::new(
                temp_dir.path().to_path_buf(),
                evin_action_sdk::PipelineMode::Normal,
                evin_action_sdk::Logger::new(
                    "test".to_string(),
                    tokio::sync::mpsc::unbounded_channel().0,
                    std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default()),
                ),
            )),
            std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default()),
        );

        let action = ArchiveAction;
        let res = action.execute(&ctx, Some(spanned_mapping(with))).await;

        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(
            err.to_string().contains("collision"),
            "Expected collision error, got: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_archive_exclude_globs() {
        let temp_dir = TempDir::new().unwrap();
        let target_dir = temp_dir.path().join("target");
        let output_tar = temp_dir.path().join("out.tar.gz");

        std::fs::create_dir_all(&target_dir).unwrap();
        std::fs::write(target_dir.join("file1.txt"), "hello").unwrap();
        std::fs::write(target_dir.join("file2.tmp"), "world").unwrap();

        let mut with = HashMap::new();
        with.insert(
            "path".to_string(),
            serde_json::Value::String(output_tar.to_string_lossy().to_string()),
        );
        with.insert(
            "include".to_string(),
            serde_json::Value::Array(vec![serde_json::Value::String("target/*".to_string())]),
        );
        with.insert(
            "exclude".to_string(),
            serde_json::Value::Array(vec![serde_json::Value::String("*.tmp".to_string())]),
        );
        with.insert(
            "layout".to_string(),
            serde_json::Value::String("flat".to_string()),
        );

        let (log_tx, _) = tokio::sync::mpsc::unbounded_channel();
        let ctx = RunnerContext::new(
            None,
            "archive_test".to_string(),
            false,
            evin_action_sdk::PipelineMode::Normal,
            log_tx,
            vec![],
            Some(temp_dir.path().to_string_lossy().to_string()),
            std::collections::HashMap::from([("SOURCE_DATE_EPOCH".to_string(), "1".to_string())]),
            temp_dir.path().to_path_buf(),
            std::collections::HashMap::new(),
            std::sync::Arc::new(evin_action_sdk::git::GitProvider::new(
                temp_dir.path().to_path_buf(),
                evin_action_sdk::PipelineMode::Normal,
                evin_action_sdk::Logger::new(
                    "test".to_string(),
                    tokio::sync::mpsc::unbounded_channel().0,
                    std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default()),
                ),
            )),
            std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default()),
        );

        let action = ArchiveAction;
        let res = action.execute(&ctx, Some(spanned_mapping(with))).await;

        assert!(res.is_ok(), "Archive execution failed: {:?}", res);
    }
    #[tokio::test]
    async fn test_archive_relative_layout_outside_cwd() {
        let temp_dir = TempDir::new().unwrap();
        let module_a = temp_dir.path().join("module_a");
        let module_b = temp_dir.path().join("module_b");
        let output_tar = temp_dir.path().join("out.tar.gz");

        std::fs::create_dir_all(&module_a).unwrap();
        std::fs::create_dir_all(&module_b).unwrap();
        std::fs::write(module_a.join("file.txt"), "hello").unwrap();
        std::fs::write(module_b.join("file.txt"), "world").unwrap();

        let mut with = HashMap::new();
        with.insert(
            "path".to_string(),
            serde_json::Value::String(output_tar.to_string_lossy().to_string()),
        );
        with.insert(
            "include".to_string(),
            serde_json::Value::Array(vec![serde_json::Value::String(
                "../module_b/*.txt".to_string(),
            )]),
        );
        with.insert(
            "layout".to_string(),
            serde_json::Value::String("relative".to_string()),
        );

        let (log_tx, _) = tokio::sync::mpsc::unbounded_channel();
        let ctx = RunnerContext::new(
            None,
            "archive_test".to_string(),
            false,
            evin_action_sdk::PipelineMode::Normal,
            log_tx,
            vec![],
            Some(module_a.to_string_lossy().to_string()), // CWD is module_a
            std::collections::HashMap::new(),
            temp_dir.path().to_path_buf(), // workspace_root is temp_dir
            std::collections::HashMap::new(),
            std::sync::Arc::new(evin_action_sdk::git::GitProvider::new(
                temp_dir.path().to_path_buf(),
                evin_action_sdk::PipelineMode::Normal,
                evin_action_sdk::Logger::new(
                    "test".to_string(),
                    tokio::sync::mpsc::unbounded_channel().0,
                    std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default()),
                ),
            )),
            std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default()),
        );

        let action = ArchiveAction;
        let res = action.execute(&ctx, Some(spanned_mapping(with))).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_archive_template_interpolation() {
        let temp_dir = TempDir::new().unwrap();
        let target_dir = temp_dir.path().join("target");
        let output_tar = temp_dir.path().join("dist/release-1.2.3.tar.gz");

        std::fs::create_dir_all(&target_dir).unwrap();
        std::fs::write(target_dir.join("file1.txt"), "hello").unwrap();

        let mut with = HashMap::new();
        with.insert(
            "path".to_string(),
            serde_json::Value::String(
                temp_dir
                    .path()
                    .join("dist/release-${bump.version}.tar.gz")
                    .to_string_lossy()
                    .to_string(),
            ),
        );
        with.insert(
            "include".to_string(),
            serde_json::Value::Array(vec![serde_json::Value::String("target/*.txt".to_string())]),
        );
        with.insert(
            "layout".to_string(),
            serde_json::Value::String("flat".to_string()),
        );
        with.insert(
            "prefix".to_string(),
            serde_json::Value::String("evin-${bump.version}".to_string()),
        );

        let mut bump_outputs = HashMap::new();
        bump_outputs.insert("version".to_string(), "1.2.3".to_string());
        let mut inputs = HashMap::new();
        inputs.insert("bump".to_string(), bump_outputs);

        let (log_tx, _) = tokio::sync::mpsc::unbounded_channel();
        let ctx = RunnerContext::new(
            None,
            "archive_test".to_string(),
            false,
            evin_action_sdk::PipelineMode::Normal,
            log_tx,
            vec![],
            Some(temp_dir.path().to_string_lossy().to_string()),
            std::collections::HashMap::from([("SOURCE_DATE_EPOCH".to_string(), "1".to_string())]),
            temp_dir.path().to_path_buf(),
            inputs,
            std::sync::Arc::new(evin_action_sdk::git::GitProvider::new(
                temp_dir.path().to_path_buf(),
                evin_action_sdk::PipelineMode::Normal,
                evin_action_sdk::Logger::new(
                    "test".to_string(),
                    tokio::sync::mpsc::unbounded_channel().0,
                    std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default()),
                ),
            )),
            std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default()),
        );

        let action = ArchiveAction;
        let res = action.execute(&ctx, Some(spanned_mapping(with))).await;

        assert!(res.is_ok(), "Archive execution failed: {:?}", res);
        assert!(output_tar.exists(), "Interpolated Archive was not created!");
        let outputs = ctx.state.outputs.lock().unwrap();
        assert_eq!(
            outputs.get("path"),
            Some(&output_tar.to_string_lossy().to_string())
        );

        // Verify the prefix inside the tar
        use flate2::read::GzDecoder;
        use std::fs::File;
        use tar::Archive;

        let tar_gz = File::open(output_tar).unwrap();
        let tar = GzDecoder::new(tar_gz);
        let mut archive = Archive::new(tar);
        let mut entries = archive.entries().unwrap();
        let first_entry = entries.next().unwrap().unwrap();
        let path = String::from_utf8_lossy(&first_entry.path_bytes()).into_owned();

        assert!(
            !path.contains('\\'),
            "archive path was not portable: {path}"
        );
        assert!(
            path.contains("evin-1.2.3"),
            "Prefix was not interpolated, got: {}",
            path
        );
    }

    #[tokio::test]
    async fn dry_run_missing_shell_artifact_is_unverified_not_failed() {
        let temp = TempDir::new().unwrap();
        let (log_tx, _) = tokio::sync::mpsc::unbounded_channel();
        let dry_run = evin_action_sdk::dry_run::DryRunState::default();
        let git = std::sync::Arc::new(evin_action_sdk::git::GitProvider::new_with_dry_run(
            temp.path().to_path_buf(),
            evin_action_sdk::PipelineMode::DryRun,
            evin_action_sdk::Logger::new(
                "package".to_string(),
                log_tx.clone(),
                std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default()),
            ),
            dry_run.clone(),
        ));
        let ctx = RunnerContext::new_with_dry_run(
            None,
            "package".to_string(),
            false,
            evin_action_sdk::PipelineMode::DryRun,
            log_tx,
            vec![],
            None,
            HashMap::new(),
            temp.path().to_path_buf(),
            [("build".to_string(), HashMap::new())]
                .into_iter()
                .collect(),
            ["build".to_string()].into_iter().collect(),
            git,
            std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default()),
            dry_run,
        );
        ctx.mark_unverified("Depends on skipped build");

        let with = evin_yaml::parse_yaml(
            "path: dist/app.tar.gz\ninclude: [target/release/app]\nlayout: flat",
        )
        .unwrap();
        ArchiveAction.execute(&ctx, Some(with)).await.unwrap();

        let outputs = ctx.state.outputs.lock().unwrap();
        assert_eq!(
            outputs.get("path").map(String::as_str),
            Some("dist/app.tar.gz")
        );
        assert_eq!(outputs.get("size").map(String::as_str), Some("unknown"));
        assert!(!ctx.uncertainty().is_empty());
        assert!(!temp.path().join("dist/app.tar.gz").exists());
    }

    #[tokio::test]
    async fn deb_and_apk_use_the_manifest_lut_and_are_reproducible() {
        use evin_action_sdk::packaging::{
            ApkTarget, DebTarget, LinuxArch, PackageIdentity, PackageManifest,
            PackageManifestCatalog, PackageManifestSource, PackageTargets, PayloadMapping,
        };

        let temp = TempDir::new().unwrap();
        std::fs::create_dir_all(temp.path().join("bin")).unwrap();
        std::fs::write(temp.path().join("bin/example"), "hello\n").unwrap();
        let manifest = PackageManifest {
            version: 1,
            identity: PackageIdentity {
                name: "example".into(),
                summary: "Example package".into(),
                description: None,
                maintainer: "Example <example@example.test>".into(),
                license: "MIT".into(),
                homepage: None,
            },
            payload: vec![PayloadMapping {
                source: "bin/example".into(),
                strip_prefix: "bin".into(),
                destination: "/usr/bin".into(),
                mode: Some(0o755),
            }],
            targets: PackageTargets {
                deb: Some(DebTarget {
                    depends: vec!["libc6".into()],
                    ..Default::default()
                }),
                apk: Some(ApkTarget {
                    depends: vec!["so:libc.musl-x86_64.so.1".into()],
                    ..Default::default()
                }),
            },
        };
        let catalog =
            std::sync::Arc::new(PackageManifestCatalog::new(vec![PackageManifestSource {
                root: temp.path().to_path_buf(),
                path: temp.path().join(".evin/package.yaml"),
                manifest,
            }]));
        let (log_tx, _) = tokio::sync::mpsc::unbounded_channel();
        let mut ctx = RunnerContext::new(
            None,
            "package".into(),
            false,
            evin_action_sdk::PipelineMode::Normal,
            log_tx,
            vec![],
            None,
            HashMap::from([("SOURCE_DATE_EPOCH".into(), "123".into())]),
            temp.path().to_path_buf(),
            HashMap::new(),
            std::sync::Arc::new(evin_action_sdk::git::GitProvider::new(
                temp.path().to_path_buf(),
                evin_action_sdk::PipelineMode::Normal,
                evin_action_sdk::Logger::new(
                    "test".into(),
                    tokio::sync::mpsc::unbounded_channel().0,
                    std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default()),
                ),
            )),
            std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default()),
        );
        ctx.package_manifests = catalog;
        let release = evin_action_sdk::release::ReleaseArtifact {
            schema: 1,
            release: evin_action_sdk::release::ReleaseIdentity {
                version: "1.2.3".into(),
                tag: "v1.2.3".into(),
            },
            packages: std::collections::BTreeMap::from([(
                "example".into(),
                evin_action_sdk::release::ArtifactPackage {
                    revisions: std::collections::BTreeMap::from([
                        (
                            "deb".into(),
                            evin_action_sdk::release::RevisionState {
                                version: "1.2.3".into(),
                                revision: 1,
                            },
                        ),
                        (
                            "apk".into(),
                            evin_action_sdk::release::RevisionState {
                                version: "1.2.3".into(),
                                revision: 1,
                            },
                        ),
                    ]),
                },
            )]),
        };
        ctx.state.inputs.insert(
            "bump".into(),
            HashMap::from([("release".into(), serde_json::to_string(&release).unwrap())]),
        );

        let deb_with = evin_yaml::parse_yaml(
            "format: deb\npath: dist/{{name}}_{{version}}_{{arch}}.deb\narchitecture: x86_64\npackage: example\nrelease: ${bump.release}\n",
        )
        .unwrap();
        ArchiveAction.execute(&ctx, Some(deb_with)).await.unwrap();
        let deb = std::fs::read(temp.path().join("dist/example_1.2.3_amd64.deb")).unwrap();
        assert!(deb.starts_with(b"!<arch>\n"));
        assert!(
            deb.windows(b"control.tar.gz".len())
                .any(|window| window == b"control.tar.gz")
        );
        assert!(
            deb.windows(b"data.tar.gz".len())
                .any(|window| window == b"data.tar.gz")
        );

        let apk_with = evin_yaml::parse_yaml("format: apk\npath: dist/{{name}}-{{version}}-r{{revision}}.apk\narchitecture: x86_64\npackage: example\nrelease: ${bump.release}\n").unwrap();
        ArchiveAction.execute(&ctx, Some(apk_with)).await.unwrap();
        let apk = std::fs::read(temp.path().join("dist/example-1.2.3-r1.apk")).unwrap();
        assert!(apk.starts_with(&[0x1f, 0x8b]));
        assert!(apk.len() > 64);
        assert_eq!(LinuxArch::X86_64.deb(), "amd64");
    }

    #[test]
    fn formats_reject_each_others_keys() {
        assert!(
            ArchiveConfig {
                format: "tar.gz".into(),
                path: "out.tar.gz".into(),
                include: Some(vec!["x".into()]),
                exclude: vec![],
                prefix: None,
                layout: LayoutMode::Flat,
                compression_level: None,
                architecture: Some(evin_action_sdk::packaging::LinuxArch::X86_64),
                package: None,
                release: None
            }
            .validate()
            .is_err()
        );
        assert!(
            ArchiveConfig {
                format: "deb".into(),
                path: "out.deb".into(),
                include: Some(vec!["x".into()]),
                exclude: vec![],
                prefix: None,
                layout: LayoutMode::Flat,
                compression_level: None,
                architecture: Some(evin_action_sdk::packaging::LinuxArch::X86_64),
                package: Some("app".into()),
                release: Some("${bump.release}".into())
            }
            .validate()
            .is_err()
        );
        let configured_default = evin_yaml::parse_yaml(
            "format: deb\npath: out.deb\narchitecture: x86_64\nlayout: flat\n",
        )
        .unwrap();
        assert!(validate_config(Some(&configured_default)).is_err());
    }

    #[test]
    fn package_release_reference_must_be_a_declared_need() {
        let with = evin_yaml::parse_yaml("format: deb\npath: out.deb\narchitecture: x86_64\npackage: app\nrelease: ${bump.release}\n").unwrap();
        let missing = evin_action_sdk::ActionValidationContext {
            task_name: "package",
            needs: &[],
        };
        assert!(validate_task_config(&missing, Some(&with)).is_err());
        let needs = vec!["bump".to_string()];
        let valid = evin_action_sdk::ActionValidationContext {
            task_name: "package",
            needs: &needs,
        };
        assert!(validate_task_config(&valid, Some(&with)).is_ok());
    }
}
