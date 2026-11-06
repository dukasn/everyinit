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
mod archiver;
pub mod config;
mod strategies;

use archiver::{ArchiveEntry, Archiver};
use config::{ArchiveConfig, LayoutMode, PackageConfig};
use strategies::{apk, arch, deb, targz::TarGzArchiver};

use evin_action_sdk::{
    RunnerContext,
    packaging::{self, PackageContext, PackageFormat, SourceEntry},
};
use glob::Pattern;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;

struct PackageBuild {
    context: PackageContext,
    package: PackageConfig,
    format: PackageFormat,
    arch: String,
    revision: u32,
    version: String,
    scripts: Vec<(String, Vec<u8>)>,
}

struct ArchiveInputReport {
    path: String,
}

#[derive(Default)]
struct ArchivePathNode {
    terminal: bool,
    children: BTreeMap<String, ArchivePathNode>,
}

struct ArchiveReport {
    inputs: Vec<ArchiveInputReport>,
    input_bytes: u64,
}

struct PackageEncoder {
    format: PackageFormat,
    write: fn(&mut dyn Write, &PackageBuild) -> Result<(), packaging::PackagingError>,
}

const PACKAGE_ENCODERS: [PackageEncoder; 3] = [
    PackageEncoder {
        format: PackageFormat::Deb,
        write: write_deb,
    },
    PackageEncoder {
        format: PackageFormat::Apk,
        write: write_apk,
    },
    PackageEncoder {
        format: PackageFormat::Arch,
        write: write_arch,
    },
];

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
        message: "release must be exactly '${task.release}'".into(),
        span: None,
    })?;
    if !ctx.needs.iter().any(|need| need == task) {
        return Err(evin_flow::ConfigError {
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
                return Err(diagnostic("package has no deb target"));
            }
            PackageFormat::Apk if package.targets.apk.is_none() => {
                return Err(diagnostic("package has no apk target"));
            }
            PackageFormat::Arch if package.targets.arch.is_none() => {
                return Err(diagnostic("package has no arch target"));
            }
            _ => {}
        }
        let artifact: evin_action_sdk::release::ReleaseArtifact =
            serde_json::from_str(config.release.as_deref().expect("validated release"))
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
            .get(&package.name)
            .and_then(|package| package.revisions.get(target_name))
            .ok_or_else(|| {
                diagnostic(format!(
                    "release has no resolved '{target_name}' revision for package '{}'",
                    package.name
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
            ctx.mark_unverified(format!(
                "Archive inputs could not be verified: {}",
                config.include.as_deref().unwrap_or_default().join(", ")
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
        evin_locales::diagnostic(evin_diagnostic::IssueCode::custom(format!(
            "Blocking task failed: {}",
            e
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

fn archive_input_report(context: &PackageContext) -> ArchiveReport {
    let inputs = context
        .entries
        .iter()
        .filter(|entry| entry.kind == evin_action_sdk::packaging::PreparedEntryKind::File)
        .map(|entry| ArchiveInputReport {
            path: entry.path.clone(),
        })
        .collect::<Vec<_>>();
    let input_bytes = context
        .entries
        .iter()
        .filter(|entry| entry.kind == evin_action_sdk::packaging::PreparedEntryKind::File)
        .map(|entry| entry.data.len() as u64)
        .sum();
    ArchiveReport {
        inputs,
        input_bytes,
    }
}

fn compact_archive_paths(inputs: &[ArchiveInputReport]) -> String {
    let mut root = ArchivePathNode::default();
    for input in inputs {
        let mut node = &mut root;
        for component in input.path.split('/') {
            node = node.children.entry(component.to_string()).or_default();
        }
        node.terminal = true;
    }
    render_archive_paths(&root)
}

fn render_archive_paths(node: &ArchivePathNode) -> String {
    if node.children.is_empty() {
        return String::new();
    }

    let extension = node
        .children
        .iter()
        .all(|(_, child)| child.terminal && child.children.is_empty())
        .then(|| {
            node.children
                .keys()
                .next()
                .and_then(|name| name.rsplit_once('.'))
                .and_then(|(stem, extension)| {
                    (!stem.is_empty() && !extension.is_empty()).then_some(extension)
                })
        })
        .flatten()
        .filter(|extension| {
            node.children.keys().all(|name| {
                name.rsplit_once('.')
                    .is_some_and(|(stem, candidate)| !stem.is_empty() && candidate == *extension)
            })
        });

    if node.children.len() > 1
        && let Some(extension) = extension
    {
        return format!("*.{extension}({})", node.children.len());
    }

    let mut branches = node
        .children
        .iter()
        .map(|(component, child)| {
            let component = escape_archive_path_component(component);
            let suffix = render_archive_paths(child);
            if suffix.is_empty() {
                component
            } else {
                format!("{component}/{suffix}")
            }
        })
        .collect::<Vec<_>>();
    if node.terminal {
        branches.insert(0, String::new());
    }
    if branches.len() == 1 {
        branches.pop().unwrap_or_default()
    } else {
        format!("{{{}}}", branches.join(","))
    }
}

fn escape_archive_path_component(component: &str) -> String {
    component
        .chars()
        .map(|character| {
            if "\\,{}*?[]".contains(character) {
                format!("\\{character}")
            } else {
                character.to_string()
            }
        })
        .collect()
}

fn format_byte_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
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
        return Err(diagnostic(format!(
            "'{key}' is not valid when format is '{}'",
            format.as_str()
        )));
    }
    Ok(())
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
    package: &PackageConfig,
) -> Result<Vec<SourceEntry>, evin_diagnostic::Diagnostic> {
    let mut entries = BTreeMap::<String, SourceEntry>::new();
    let root = ctx.fs.task_cwd();
    for mapping in &package.payload {
        if mapping.source.trim().is_empty() || mapping.strip_prefix.trim().is_empty() {
            return Err(diagnostic(
                "package payload source and strip_prefix must not be empty",
            ));
        }
        if mapping.mode.is_some_and(|mode| mode > 0o777) {
            return Err(diagnostic("package payload mode must be at most 0777"));
        }
        let strip = packaging::safe_relative_path(&mapping.strip_prefix)
            .map_err(|error| diagnostic(error.to_string()))?;
        let destination = packaging::normalize_install_dir(&mapping.destination)
            .map_err(|error| diagnostic(error.to_string()))?;
        let strip = root.join(strip);
        for source_path in ctx
            .fs
            .matching_files_from(root, &mapping.source)
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
    ctx: &RunnerContext,
    package: &PackageConfig,
    format: PackageFormat,
) -> Result<Vec<(String, Vec<u8>)>, evin_diagnostic::Diagnostic> {
    let definitions: Vec<(&str, Option<&String>)> = match format {
        PackageFormat::Deb => {
            let scripts = &package
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
            let scripts = &package
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
        PackageFormat::Arch => vec![(
            ".INSTALL",
            package
                .targets
                .arch
                .as_ref()
                .expect("validated target")
                .scripts
                .install
                .as_ref(),
        )],
        PackageFormat::TarGz => Vec::new(),
    };
    definitions
        .into_iter()
        .filter_map(|(name, path)| path.map(|path| (name, path)))
        .map(|(name, path)| {
            let path = packaging::safe_relative_path(path)
                .map_err(|error| diagnostic(error.to_string()))?;
            let bytes = std::fs::read(ctx.fs.task_cwd().join(path))
                .map_err(|error| diagnostic(format!("failed to read package script: {error}")))?;
            Ok((name.to_string(), bytes))
        })
        .collect()
}

fn write_deb(
    output: &mut dyn Write,
    build: &PackageBuild,
) -> Result<(), packaging::PackagingError> {
    let target = build
        .package
        .targets
        .deb
        .as_ref()
        .expect("fixed encoder target");
    deb::write(
        output,
        deb::DebPackage {
            context: &build.context,
            name: &build.package.name,
            version: &build.version,
            revision: build.revision,
            architecture: &build.arch,
            summary: &build.package.summary,
            description: build.package.description.as_deref(),
            maintainer: &build.package.maintainer,
            homepage: build.package.homepage.as_deref(),
            depends: target.depends.clone(),
            scripts: build.scripts.clone(),
        },
    )
}

fn write_apk(
    output: &mut dyn Write,
    build: &PackageBuild,
) -> Result<(), packaging::PackagingError> {
    let target = build
        .package
        .targets
        .apk
        .as_ref()
        .expect("fixed encoder target");
    apk::write(
        output,
        apk::ApkPackage {
            context: &build.context,
            name: &build.package.name,
            version: &build.version,
            revision: build.revision,
            architecture: &build.arch,
            summary: &build.package.summary,
            homepage: build.package.homepage.as_deref(),
            maintainer: &build.package.maintainer,
            license: &build.package.license,
            depends: target.depends.clone(),
            scripts: build.scripts.clone(),
        },
    )
}

fn write_arch(
    output: &mut dyn Write,
    build: &PackageBuild,
) -> Result<(), packaging::PackagingError> {
    let target = build
        .package
        .targets
        .arch
        .as_ref()
        .expect("fixed encoder target");
    let install = build
        .scripts
        .iter()
        .find(|(path, _)| path == ".INSTALL")
        .map(|(_, data)| data.as_slice());
    arch::write(
        output,
        arch::ArchPackage {
            context: &build.context,
            name: &build.package.name,
            version: &build.version,
            revision: build.revision,
            architecture: &build.arch,
            summary: &build.package.summary,
            homepage: build.package.homepage.as_deref(),
            maintainer: &build.package.maintainer,
            license: &build.package.license,
            depends: &target.depends,
            optdepends: &target.optdepends,
            provides: &target.provides,
            conflicts: &target.conflicts,
            replaces: &target.replaces,
            backup: &target.backup,
            install,
        },
    )
}

fn package_filename(
    format: PackageFormat,
    name: &str,
    version: &str,
    revision: u32,
    arch: &str,
) -> String {
    match format {
        PackageFormat::Deb => format!("{name}_{version}-{revision}_{arch}.deb"),
        PackageFormat::Apk => format!("{name}-{version}-r{revision}.apk"),
        PackageFormat::Arch => format!("{name}-{version}-{revision}-{arch}.pkg.tar.zst"),
        PackageFormat::TarGz => unreachable!(),
    }
}

fn validate_package(
    package: &PackageConfig,
    format: PackageFormat,
) -> Result<(), evin_diagnostic::Diagnostic> {
    for (field, value) in [
        ("name", &package.name),
        ("summary", &package.summary),
        ("maintainer", &package.maintainer),
        ("license", &package.license),
    ] {
        validate_metadata_string(field, value)?;
    }
    for (field, value) in [
        ("description", package.description.as_deref()),
        ("homepage", package.homepage.as_deref()),
    ] {
        if let Some(value) = value {
            validate_metadata_string(field, value)?;
        }
    }
    if !package.name.chars().all(|character| {
        character.is_ascii_lowercase()
            || character.is_ascii_digit()
            || character == '-'
            || character == '_'
            || character == '.'
    }) {
        return Err(diagnostic("package name contains unsupported characters"));
    }
    if let Some(target) = &package.targets.arch {
        for (field, values) in [
            ("depends", &target.depends),
            ("optdepends", &target.optdepends),
            ("provides", &target.provides),
            ("conflicts", &target.conflicts),
            ("replaces", &target.replaces),
        ] {
            validate_relations(field, values)?;
        }
        validate_relations("backup", &target.backup)?;
        for path in &target.backup {
            packaging::normalize_archive_path(path)
                .map_err(|error| diagnostic(error.to_string()))?;
        }
    }
    if let Some(target) = &package.targets.deb {
        validate_relations("depends", &target.depends)?;
        for path in [
            target.scripts.preinst.as_deref(),
            target.scripts.postinst.as_deref(),
            target.scripts.prerm.as_deref(),
            target.scripts.postrm.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            validate_script_path(path)?;
        }
    }
    if let Some(target) = &package.targets.apk {
        validate_relations("depends", &target.depends)?;
        for path in [
            target.scripts.pre_install.as_deref(),
            target.scripts.post_install.as_deref(),
            target.scripts.pre_deinstall.as_deref(),
            target.scripts.post_deinstall.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            validate_script_path(path)?;
        }
    }
    if let Some(target) = &package.targets.arch
        && let Some(path) = target.scripts.install.as_deref()
    {
        validate_script_path(path)?;
    }
    let _ = format;
    Ok(())
}

fn validate_metadata_string(field: &str, value: &str) -> Result<(), evin_diagnostic::Diagnostic> {
    if value.trim().is_empty() || value.contains(['\n', '\r', '\0']) {
        return Err(diagnostic(format!(
            "package {field} must be non-empty and contain no line breaks"
        )));
    }
    Ok(())
}

fn validate_relations(field: &str, values: &[String]) -> Result<(), evin_diagnostic::Diagnostic> {
    let mut seen = std::collections::BTreeSet::new();
    for value in values {
        if value.trim().is_empty() || value.contains(['\n', '\r', '\0']) {
            return Err(diagnostic(format!(
                "package {field} entries must be non-empty single-line strings"
            )));
        }
        if !seen.insert(value) {
            return Err(diagnostic(format!(
                "package {field} contains '{value}' more than once"
            )));
        }
    }
    Ok(())
}

fn validate_script_path(path: &str) -> Result<(), evin_diagnostic::Diagnostic> {
    if path.trim().is_empty() {
        return Err(diagnostic("package script path must not be empty"));
    }
    packaging::safe_relative_path(path).map_err(|error| diagnostic(error.to_string()))?;
    Ok(())
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

    fn spanned_value(value: serde_json::Value) -> evin_flow::SpannedValue {
        let span = evin_flow::Span::default();
        match value {
            serde_json::Value::Null => evin_flow::SpannedValue::Null(span),
            serde_json::Value::Bool(value) => evin_flow::SpannedValue::Bool(value, span),
            serde_json::Value::Number(value) => {
                if let Some(value) = value.as_i64() {
                    evin_flow::SpannedValue::Integer(value, span)
                } else {
                    evin_flow::SpannedValue::Float(value.as_f64().unwrap_or_default(), span)
                }
            }
            serde_json::Value::String(value) => evin_flow::SpannedValue::String(value, span),
            serde_json::Value::Array(values) => evin_flow::SpannedValue::Sequence(
                values.into_iter().map(spanned_value).collect(),
                span,
            ),
            serde_json::Value::Object(values) => evin_flow::SpannedValue::Mapping(
                values
                    .into_iter()
                    .map(|(key, value)| {
                        (
                            evin_flow::SpannedValue::String(key, span),
                            spanned_value(value),
                        )
                    })
                    .collect(),
                span,
            ),
        }
    }

    fn spanned_mapping(with: HashMap<String, serde_json::Value>) -> evin_flow::SpannedValue {
        evin_flow::SpannedValue::Mapping(
            with.into_iter()
                .map(|(key, value)| {
                    (
                        evin_flow::SpannedValue::String(key, evin_flow::Span::default()),
                        spanned_value(value),
                    )
                })
                .collect(),
            evin_flow::Span::default(),
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

        let with = evin_flow::parse_yaml(
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
    async fn distro_packages_use_inline_config_and_derived_paths() {
        let temp = TempDir::new().unwrap();
        std::fs::create_dir_all(temp.path().join("bin")).unwrap();
        std::fs::write(temp.path().join("bin/example"), "hello\n").unwrap();
        std::fs::write(
            temp.path().join("bin/install"),
            "post_install() { echo installed; }\n",
        )
        .unwrap();
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
        ctx.values = evin_action_sdk::Values::new(HashMap::from([(
            "package_name".into(),
            "example".into(),
        )]));
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
                        (
                            "arch".into(),
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

        let package = "package:\n  name: ${package_name}\n  summary: Example package\n  maintainer: Example <example@example.test>\n  license: MIT\n  payload:\n    - source: bin/example\n      strip_prefix: bin\n      destination: /usr/bin\n      mode: 493\n  targets:\n    deb:\n      depends: [libc6]\n    apk:\n      depends: [so:libc.musl-x86_64.so.1]\n    arch:\n      depends: [glibc]\n      optdepends: ['bash: completion support']\n      provides: [example-cli]\n      conflicts: [example-old]\n      replaces: [example-old]\n      backup: [etc/example.conf]\n      scripts:\n        install: bin/install\n";
        let deb_with = evin_flow::parse_yaml(&format!("format: deb\noutput_dir: dist\narchitecture: x86_64\nrelease: ${{bump.release}}\n{package}")).unwrap();
        ArchiveAction.execute(&ctx, Some(deb_with)).await.unwrap();
        let deb = std::fs::read(temp.path().join("dist/example_1.2.3-1_amd64.deb")).unwrap();
        assert!(deb.starts_with(b"!<arch>\n"));
        assert!(
            deb.windows(b"control.tar.gz".len())
                .any(|window| window == b"control.tar.gz")
        );
        assert!(
            deb.windows(b"data.tar.gz".len())
                .any(|window| window == b"data.tar.gz")
        );

        let apk_with = evin_flow::parse_yaml(&format!("format: apk\noutput_dir: dist\narchitecture: x86_64\nrelease: ${{bump.release}}\n{package}")).unwrap();
        ArchiveAction.execute(&ctx, Some(apk_with)).await.unwrap();
        let apk = std::fs::read(temp.path().join("dist/example-1.2.3-r1.apk")).unwrap();
        assert!(apk.starts_with(&[0x1f, 0x8b]));
        assert!(apk.len() > 64);
        let arch_with = evin_flow::parse_yaml(&format!("format: arch\noutput_dir: dist\narchitecture: x86_64\nrelease: ${{bump.release}}\n{package}")).unwrap();
        ArchiveAction
            .execute(&ctx, Some(arch_with.clone()))
            .await
            .unwrap();
        let arch_path = temp.path().join("dist/example-1.2.3-1-x86_64.pkg.tar.zst");
        let bytes = std::fs::read(&arch_path).unwrap();
        let tar = zstandard::decode_all(bytes.as_slice()).unwrap();
        let mut archive = tar::Archive::new(std::io::Cursor::new(tar));
        let mut members = BTreeMap::new();
        for entry in archive.entries().unwrap() {
            let mut entry = entry.unwrap();
            let path = entry.path().unwrap().to_string_lossy().into_owned();
            let mut contents = Vec::new();
            std::io::Read::read_to_end(&mut entry, &mut contents).unwrap();
            members.insert(path, contents);
        }
        let names = members.keys().cloned().collect::<Vec<_>>();
        assert_eq!(names, {
            let mut sorted = names.clone();
            sorted.sort();
            sorted
        });
        let pkginfo = std::str::from_utf8(&members[".PKGINFO"]).unwrap();
        assert!(pkginfo.contains("pkgbase = example\n"));
        assert!(pkginfo.contains("xdata = pkgtype=pkg\n"));
        assert!(pkginfo.contains("pkgver = 1.2.3-1\n"));
        assert!(pkginfo.contains("optdepend = bash: completion support\n"));
        assert!(pkginfo.contains("provides = example-cli\n"));
        assert!(pkginfo.contains("conflict = example-old\n"));
        assert!(pkginfo.contains("replaces = example-old\n"));
        assert!(pkginfo.contains("backup = etc/example.conf\n"));
        assert!(
            std::str::from_utf8(&members[".MTREE"])
                .unwrap()
                .contains("./usr/bin/example type=file mode=755 time=123")
        );
        assert_eq!(members[".INSTALL"], b"post_install() { echo installed; }\n");
        ArchiveAction.execute(&ctx, Some(arch_with)).await.unwrap();
        assert_eq!(bytes, std::fs::read(arch_path).unwrap());
        assert_eq!(
            evin_action_sdk::packaging::LinuxArch::X86_64.arch(),
            "x86_64"
        );
    }

    #[test]
    fn formats_reject_each_others_keys() {
        let tar_with_package =
            evin_flow::parse_yaml("path: out.tar.gz\ninclude: [x]\npackage: { name: app }\n")
                .unwrap();
        assert!(validate_config(Some(&tar_with_package)).is_err());
        let package_with_path = evin_flow::parse_yaml(
            "format: deb\npath: out.deb\noutput_dir: dist\narchitecture: x86_64\nrelease: ${bump.release}\npackage: { name: app, summary: app, maintainer: A, license: MIT, targets: { deb: {} } }\n",
        ).unwrap();
        assert!(validate_config(Some(&package_with_path)).is_err());
        let configured_default = evin_flow::parse_yaml(
            "format: deb\noutput_dir: dist\narchitecture: x86_64\nlayout: flat\n",
        )
        .unwrap();
        assert!(validate_config(Some(&configured_default)).is_err());
    }

    #[test]
    fn package_release_reference_must_be_a_declared_need() {
        let with = evin_flow::parse_yaml("format: deb\noutput_dir: dist\narchitecture: x86_64\npackage: { name: app, summary: app, maintainer: A, license: MIT, targets: { deb: {} } }\nrelease: ${bump.release}\n").unwrap();
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

    #[test]
    fn package_validation_rejects_invalid_relations_and_backup_paths() {
        let invalid_relation = evin_flow::parse_yaml(
            "format: arch\noutput_dir: dist\narchitecture: x86_64\nrelease: ${bump.release}\npackage: { name: app, summary: app, maintainer: A, license: MIT, targets: { arch: { depends: [glibc, glibc] } } }\n",
        )
        .unwrap();
        let config: ArchiveConfig = evin_action_sdk::parse_config(Some(&invalid_relation)).unwrap();
        assert!(validate_package(config.package.as_ref().unwrap(), PackageFormat::Arch).is_err());

        let unsafe_backup = evin_flow::parse_yaml(
            "format: arch\noutput_dir: dist\narchitecture: x86_64\nrelease: ${bump.release}\npackage: { name: app, summary: app, maintainer: A, license: MIT, targets: { arch: { backup: ['../etc/app.conf'] } } }\n",
        )
        .unwrap();
        let config: ArchiveConfig = evin_action_sdk::parse_config(Some(&unsafe_backup)).unwrap();
        assert!(validate_package(config.package.as_ref().unwrap(), PackageFormat::Arch).is_err());
    }
}
