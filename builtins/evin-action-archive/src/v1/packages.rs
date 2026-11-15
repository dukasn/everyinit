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
use super::config::PackageConfig;
use super::diagnostic;
use super::strategies::{apk, arch, deb};
use evin_action_sdk::{
    RunnerContext,
    packaging::{self, PackageContext, PackageFormat, SourceEntry},
};
use std::collections::BTreeMap;
use std::io::Write;

pub(super) struct PackageBuild {
    pub(super) context: PackageContext,
    pub(super) package: PackageConfig,
    pub(super) format: PackageFormat,
    pub(super) arch: String,
    pub(super) revision: u32,
    pub(super) version: String,
    pub(super) scripts: Vec<(String, Vec<u8>)>,
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
    let mut entries = BTreeMap::<String, SourceEntry>::new();
    let root = ctx.fs.task_cwd();
    for mapping in &package.payload {
        if mapping.source.trim().is_empty() || mapping.strip_prefix.trim().is_empty() {
            return Err(diagnostic(evin_locales::t!(
                "archive-payload-source-prefix-required"
            )));
        }
        if mapping.mode.is_some_and(|mode| mode > 0o777) {
            return Err(diagnostic(evin_locales::t!("archive-payload-mode-invalid")));
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
                diagnostic(evin_locales::t!(
                    "archive-payload-outside-strip-prefix",
                    source = source_path.display().to_string(),
                    prefix = strip.display().to_string()
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
                return Err(diagnostic(evin_locales::t!(
                    "archive-payload-collision",
                    destination = destination
                )));
            }
        }
    }
    Ok(entries.into_values().collect())
}

pub(super) fn load_scripts(
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
            let bytes = std::fs::read(ctx.fs.task_cwd().join(path)).map_err(|error| {
                diagnostic(evin_locales::t!(
                    "archive-package-script-read-failed",
                    error = error.to_string()
                ))
            })?;
            Ok((name.to_string(), bytes))
        })
        .collect()
}

pub(super) fn write_deb(
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

pub(super) fn write_apk(
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

pub(super) fn write_arch(
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

pub(super) fn validate_package(
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
        return Err(diagnostic(evin_locales::t!("archive-package-name-invalid")));
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
        return Err(diagnostic(evin_locales::t!(
            "archive-package-metadata-invalid",
            field = field
        )));
    }
    Ok(())
}

fn validate_relations(field: &str, values: &[String]) -> Result<(), evin_diagnostic::Diagnostic> {
    let mut seen = std::collections::BTreeSet::new();
    for value in values {
        if value.trim().is_empty() || value.contains(['\n', '\r', '\0']) {
            return Err(diagnostic(evin_locales::t!(
                "archive-package-relation-invalid",
                field = field
            )));
        }
        if !seen.insert(value) {
            return Err(diagnostic(evin_locales::t!(
                "archive-package-relation-duplicate",
                field = field,
                value = value
            )));
        }
    }
    Ok(())
}

fn validate_script_path(path: &str) -> Result<(), evin_diagnostic::Diagnostic> {
    if path.trim().is_empty() {
        return Err(diagnostic(evin_locales::t!(
            "archive-package-script-path-empty"
        )));
    }
    packaging::safe_relative_path(path).map_err(|error| diagnostic(error.to_string()))?;
    Ok(())
}
