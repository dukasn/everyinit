// Copyright (C) 2026 Eduardo Marques Santos
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.

#![allow(clippy::result_large_err)]
use super::config::{ApkConfig, ArchConfig, DebConfig, PackageConfig};
use super::diagnostic;
use super::strategies::{apk, arch, deb};
use evin_action_sdk::{
    RunnerContext,
    packaging::{self, PackageContext, PackageFormat, SourceEntry},
};
use std::io::Write;

pub(super) struct PackageMetadata {
    pub(super) package: PackageConfig,
    pub(super) format: PackageFormat,
    pub(super) arch: String,
    pub(super) revision: u32,
    pub(super) version: String,
    pub(super) description: Option<String>,
    pub(super) license: Option<String>,
    pub(super) depends: Vec<String>,
    pub(super) optdepends: Vec<String>,
    pub(super) provides: Vec<String>,
    pub(super) conflicts: Vec<String>,
    pub(super) replaces: Vec<String>,
    pub(super) backup: Vec<String>,
    pub(super) scripts: Vec<(String, Vec<u8>)>,
}

pub(super) struct PackageBuild {
    pub(super) context: PackageContext,
    pub(super) metadata: PackageMetadata,
}

pub(super) struct PackageEncoder {
    pub(super) format: PackageFormat,
    pub(super) write: fn(&mut dyn Write, &PackageBuild) -> Result<(), packaging::PackagingError>,
}

pub(super) const PACKAGE_ENCODERS: [PackageEncoder; 3] = [
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

pub(super) fn resolve_package_payloads(
    ctx: &RunnerContext,
    package: &PackageConfig,
) -> Result<Vec<SourceEntry>, evin_diagnostic::Diagnostic> {
    packaging::resolve_payloads(&ctx.fs, &package.payload)
        .map_err(evin_diagnostic::Diagnostic::from)
}

pub(super) fn load_scripts(
    ctx: &RunnerContext,
    definitions: Vec<(&str, Option<&String>)>,
) -> Result<Vec<(String, Vec<u8>)>, evin_diagnostic::Diagnostic> {
    definitions
        .into_iter()
        .filter_map(|(name, path)| path.map(|path| (name, path)))
        .map(|(name, path)| {
            let path =
                packaging::safe_relative_path(path).map_err(evin_diagnostic::Diagnostic::from)?;
            let source =
                packaging::resolve_workspace_source(&ctx.fs, &ctx.fs.task_cwd().join(path))
                    .map_err(evin_diagnostic::Diagnostic::from)?;
            let bytes = std::fs::read(source).map_err(|error| {
                diagnostic(
                    evin_diagnostic::IssueCode::new("archive-package-script-read-failed")
                        .arg("error", error.to_string()),
                )
            })?;
            Ok((name.to_string(), bytes))
        })
        .collect()
}

pub(super) fn write_deb(
    output: &mut dyn Write,
    build: &PackageBuild,
) -> Result<(), packaging::PackagingError> {
    deb::write(
        output,
        deb::DebPackage {
            context: &build.context,
            name: &build.metadata.package.name,
            version: &build.metadata.version,
            revision: build.metadata.revision,
            architecture: &build.metadata.arch,
            summary: &build.metadata.package.summary,
            description: build.metadata.description.as_deref(),
            maintainer: &build.metadata.package.maintainer,
            homepage: build.metadata.package.homepage.as_deref(),
            depends: build.metadata.depends.clone(),
            scripts: build.metadata.scripts.clone(),
        },
    )
}

pub(super) fn write_apk(
    output: &mut dyn Write,
    build: &PackageBuild,
) -> Result<(), packaging::PackagingError> {
    apk::write(
        output,
        apk::ApkPackage {
            context: &build.context,
            name: &build.metadata.package.name,
            version: &build.metadata.version,
            revision: build.metadata.revision,
            architecture: &build.metadata.arch,
            summary: &build.metadata.package.summary,
            homepage: build.metadata.package.homepage.as_deref(),
            maintainer: &build.metadata.package.maintainer,
            license: build
                .metadata
                .license
                .as_deref()
                .expect("APK license validated"),
            depends: build.metadata.depends.clone(),
            scripts: build.metadata.scripts.clone(),
        },
    )
}

pub(super) fn write_arch(
    output: &mut dyn Write,
    build: &PackageBuild,
) -> Result<(), packaging::PackagingError> {
    let install = build
        .metadata
        .scripts
        .iter()
        .find(|(path, _)| path == ".INSTALL")
        .map(|(_, data)| data.as_slice());
    arch::write(
        output,
        arch::ArchPackage {
            context: &build.context,
            name: &build.metadata.package.name,
            version: &build.metadata.version,
            revision: build.metadata.revision,
            architecture: &build.metadata.arch,
            summary: &build.metadata.package.summary,
            homepage: build.metadata.package.homepage.as_deref(),
            maintainer: &build.metadata.package.maintainer,
            license: build
                .metadata
                .license
                .as_deref()
                .expect("Arch license validated"),
            depends: &build.metadata.depends,
            optdepends: &build.metadata.optdepends,
            provides: &build.metadata.provides,
            conflicts: &build.metadata.conflicts,
            replaces: &build.metadata.replaces,
            backup: &build.metadata.backup,
            install,
        },
    )
}

pub(super) fn package_filename(
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

pub(super) fn validate_package(package: &PackageConfig) -> Result<(), evin_diagnostic::Diagnostic> {
    for (field, value) in [
        ("name", &package.name),
        ("summary", &package.summary),
        ("maintainer", &package.maintainer),
    ] {
        validate_metadata_string(field, value)?;
    }
    if let Some(homepage) = package.homepage.as_deref() {
        validate_metadata_string("homepage", homepage)?;
    }
    if !package.name.contains("${")
        && !package.name.chars().all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || character == '-'
                || character == '_'
                || character == '.'
        })
    {
        return Err(diagnostic(evin_diagnostic::IssueCode::new(
            "archive-package-name-invalid",
        )));
    }
    Ok(())
}

pub(super) fn validate_deb(config: &DebConfig) -> Result<(), evin_diagnostic::Diagnostic> {
    validate_package(&config.package)?;
    if let Some(description) = config.description.as_deref() {
        validate_metadata_string("description", description)?;
    }
    validate_relations("depends", &config.depends)?;
    for path in [
        config.scripts.preinst.as_deref(),
        config.scripts.postinst.as_deref(),
        config.scripts.prerm.as_deref(),
        config.scripts.postrm.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        validate_script_path(path)?;
    }
    Ok(())
}

pub(super) fn validate_apk(config: &ApkConfig) -> Result<(), evin_diagnostic::Diagnostic> {
    validate_package(&config.package)?;
    validate_metadata_string("license", &config.license)?;
    validate_relations("depends", &config.depends)?;
    for path in [
        config.scripts.pre_install.as_deref(),
        config.scripts.post_install.as_deref(),
        config.scripts.pre_deinstall.as_deref(),
        config.scripts.post_deinstall.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        validate_script_path(path)?;
    }
    Ok(())
}

pub(super) fn validate_arch(config: &ArchConfig) -> Result<(), evin_diagnostic::Diagnostic> {
    validate_package(&config.package)?;
    validate_metadata_string("license", &config.license)?;
    for (field, values) in [
        ("depends", &config.depends),
        ("optdepends", &config.optdepends),
        ("provides", &config.provides),
        ("conflicts", &config.conflicts),
        ("replaces", &config.replaces),
    ] {
        validate_relations(field, values)?;
    }
    validate_relations("backup", &config.backup)?;
    for path in &config.backup {
        packaging::normalize_archive_path(path).map_err(evin_diagnostic::Diagnostic::from)?;
    }
    if let Some(path) = config.scripts.install.as_deref() {
        validate_script_path(path)?;
    }
    Ok(())
}

fn validate_metadata_string(field: &str, value: &str) -> Result<(), evin_diagnostic::Diagnostic> {
    if value.trim().is_empty() || value.contains(['\n', '\r', '\0']) {
        return Err(diagnostic(
            evin_diagnostic::IssueCode::new("archive-package-metadata-invalid").arg("field", field),
        ));
    }
    Ok(())
}

fn validate_relations(field: &str, values: &[String]) -> Result<(), evin_diagnostic::Diagnostic> {
    let mut seen = std::collections::BTreeSet::new();
    for value in values {
        if value.trim().is_empty() || value.contains(['\n', '\r', '\0']) {
            return Err(diagnostic(
                evin_diagnostic::IssueCode::new("archive-package-relation-invalid")
                    .arg("field", field),
            ));
        }
        if !seen.insert(value) {
            return Err(diagnostic(
                evin_diagnostic::IssueCode::new("archive-package-relation-duplicate")
                    .arg("field", field)
                    .arg("value", value),
            ));
        }
    }
    Ok(())
}

fn validate_script_path(path: &str) -> Result<(), evin_diagnostic::Diagnostic> {
    if path.trim().is_empty() {
        return Err(diagnostic(evin_diagnostic::IssueCode::new(
            "archive-package-script-path-empty",
        )));
    }
    packaging::safe_relative_path(path).map_err(evin_diagnostic::Diagnostic::from)?;
    Ok(())
}
