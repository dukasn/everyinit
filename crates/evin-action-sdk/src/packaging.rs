// Copyright (C) 2026 Eduardo Marques Santos
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.

//! Backend-neutral, reproducible packaging primitives.
//!
//! This module deliberately owns package-domain data and deterministic stream
//! construction, but not workspace globbing or a distribution's outer
//! container. Native actions provide those two boundaries.

#![allow(clippy::result_large_err)]

use crate::fs::WorkspaceFs;
use flate2::{Compression, GzBuilder};
use md5::{Digest as Md5Digest, Md5};
use serde::Deserialize;
use sha2::Sha256;
use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
use tar::{Builder, EntryType, Header};

#[derive(Debug, thiserror::Error)]
pub enum PackagingError {
    #[error("invalid package path '{0}'")]
    InvalidPath(String),
    #[error("package path collision at '{0}'")]
    Collision(String),
    #[error("package payload collision at '{0}'")]
    PayloadCollision(String),
    #[error("cannot read package input '{path}': {source}")]
    Read { path: PathBuf, source: io::Error },
    #[error("package payload source and strip prefix must not be empty")]
    EmptyPayloadSourceOrPrefix,
    #[error("package payload mode {0:o} is outside the supported range")]
    InvalidPayloadMode(u32),
    #[error("could not resolve package payload source '{pattern}': {error}")]
    PayloadSourceDiscovery { pattern: String, error: String },
    #[error("package payload '{path}' is outside strip prefix '{prefix}'")]
    PayloadOutsideStripPrefix { path: PathBuf, prefix: PathBuf },
    #[error("could not safely resolve package payload source '{path}': {error}")]
    UnsafePayloadSource { path: PathBuf, error: String },
    #[error("invalid SOURCE_DATE_EPOCH: {0}")]
    InvalidEpoch(String),
    #[error("archive writer failed: {0}")]
    Archive(#[from] io::Error),
}

impl From<PackagingError> for evin_diagnostic::Diagnostic {
    fn from(error: PackagingError) -> Self {
        use evin_diagnostic::IssueCode;

        let code = match error {
            PackagingError::InvalidPath(path) => {
                IssueCode::new("package-path-invalid").arg("path", path)
            }
            PackagingError::Collision(path) => {
                IssueCode::new("package-path-collision").arg("path", path)
            }
            PackagingError::PayloadCollision(destination) => {
                IssueCode::new("package-payload-collision").arg("destination", destination)
            }
            PackagingError::Read { path, source } => IssueCode::new("package-input-read-failed")
                .arg("path", path.display().to_string())
                .arg("error", source.to_string()),
            PackagingError::EmptyPayloadSourceOrPrefix => {
                IssueCode::new("package-payload-source-prefix-required")
            }
            PackagingError::InvalidPayloadMode(_) => IssueCode::new("package-payload-mode-invalid"),
            PackagingError::PayloadSourceDiscovery { pattern, error } => {
                IssueCode::new("package-payload-source-discovery-failed")
                    .arg("source", pattern)
                    .arg("error", error)
            }
            PackagingError::PayloadOutsideStripPrefix { path, prefix } => {
                IssueCode::new("package-payload-outside-strip-prefix")
                    .arg("source", path.display().to_string())
                    .arg("prefix", prefix.display().to_string())
            }
            PackagingError::UnsafePayloadSource { path, error } => {
                IssueCode::new("package-payload-safe-path-failed")
                    .arg("source", path.display().to_string())
                    .arg("error", error)
            }
            PackagingError::InvalidEpoch(value) => {
                IssueCode::new("package-source-date-epoch-invalid").arg("value", value)
            }
            PackagingError::Archive(error) => {
                IssueCode::new("package-archive-write-failed").arg("error", error.to_string())
            }
        };
        evin_locales::diagnostic(code)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinuxArch {
    X86_64,
    Aarch64,
    X86,
    Armv7,
    Riscv64,
    Ppc64le,
    S390x,
    Noarch,
}

pub fn parse_source_date_epoch(value: &str) -> Result<u64, PackagingError> {
    value
        .parse::<u64>()
        .map_err(|_| PackagingError::InvalidEpoch(value.to_string()))
}

#[derive(Debug, Clone)]
pub struct SourceEntry {
    pub source: PathBuf,
    pub destination: String,
    pub mode: Option<u32>,
}

/// A task-relative source mapping into a package installation directory.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PayloadMapping {
    pub source: String,
    pub strip_prefix: String,
    pub destination: String,
    #[serde(default)]
    pub mode: Option<u32>,
}

/// Resolve package payload mappings through Evin's workspace-safe filesystem.
///
/// The result is ordered by installation path and rejects escaping sources,
/// unsafe physical paths, invalid modes, and destination collisions before a
/// package encoder reads any payload bytes.
pub fn resolve_payloads(
    fs: &WorkspaceFs,
    mappings: &[PayloadMapping],
) -> Result<Vec<SourceEntry>, PackagingError> {
    let root = fs.task_cwd();
    let mut entries = BTreeMap::<String, SourceEntry>::new();
    for mapping in mappings {
        if mapping.source.trim().is_empty() || mapping.strip_prefix.trim().is_empty() {
            return Err(PackagingError::EmptyPayloadSourceOrPrefix);
        }
        if mapping.mode.is_some_and(|mode| mode > 0o777) {
            return Err(PackagingError::InvalidPayloadMode(
                mapping.mode.unwrap_or_default(),
            ));
        }
        let strip = safe_relative_path(&mapping.strip_prefix)?;
        let destination = normalize_install_dir(&mapping.destination)?;
        let strip_path = root.join(strip);
        for source_path in fs
            .matching_files_from(root, &mapping.source)
            .map_err(|error| PackagingError::PayloadSourceDiscovery {
                pattern: mapping.source.clone(),
                error: error.to_string(),
            })?
        {
            let relative = source_path.strip_prefix(&strip_path).map_err(|_| {
                PackagingError::PayloadOutsideStripPrefix {
                    path: source_path.clone(),
                    prefix: strip_path.clone(),
                }
            })?;
            let install_path = format!(
                "{destination}/{}",
                relative.to_string_lossy().replace('\\', "/")
            );
            let install_path = normalize_archive_path(&install_path)?;
            let safe_source = resolve_workspace_source(fs, &source_path)?;
            if entries
                .insert(
                    install_path.clone(),
                    SourceEntry {
                        source: safe_source,
                        destination: install_path.clone(),
                        mode: mapping.mode,
                    },
                )
                .is_some()
            {
                return Err(PackagingError::PayloadCollision(install_path));
            }
        }
    }
    Ok(entries.into_values().collect())
}

/// Resolve an existing task input to a physical path within the workspace.
/// Missing dry-run inputs keep their checked lexical path for planning.
pub fn resolve_workspace_source(
    fs: &WorkspaceFs,
    source: &Path,
) -> Result<PathBuf, PackagingError> {
    if source.exists() {
        let workspace_root = fs.workspace_root().canonicalize().map_err(|error| {
            PackagingError::UnsafePayloadSource {
                path: source.to_path_buf(),
                error: error.to_string(),
            }
        })?;
        let task_cwd =
            fs.task_cwd()
                .canonicalize()
                .map_err(|error| PackagingError::UnsafePayloadSource {
                    path: source.to_path_buf(),
                    error: error.to_string(),
                })?;
        if !task_cwd.starts_with(&workspace_root) {
            return Err(PackagingError::UnsafePayloadSource {
                path: source.to_path_buf(),
                error: "task cwd resolves outside the workspace".into(),
            });
        }
        let physical_source =
            source
                .canonicalize()
                .map_err(|error| PackagingError::UnsafePayloadSource {
                    path: source.to_path_buf(),
                    error: error.to_string(),
                })?;
        if !physical_source.starts_with(&workspace_root) {
            return Err(PackagingError::UnsafePayloadSource {
                path: source.to_path_buf(),
                error: "source resolves outside the workspace".into(),
            });
        }
        Ok(physical_source)
    } else {
        // Dry-run virtual inputs do not exist on disk; retain their checked
        // workspace-relative spelling for planning without dereferencing them.
        fs.get_safe_physical_path(&source.to_string_lossy())
            .map_err(|error| PackagingError::UnsafePayloadSource {
                path: source.to_path_buf(),
                error: error.to_string(),
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreparedEntryKind {
    Directory,
    File,
}

#[derive(Debug, Clone)]
pub struct PreparedEntry {
    pub path: String,
    pub kind: PreparedEntryKind,
    pub data: Vec<u8>,
    pub mode: u32,
    pub mtime: u64,
    pub md5: Option<String>,
    pub sha256: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PackageContext {
    pub epoch: u64,
    pub entries: Vec<PreparedEntry>,
}

impl PackageContext {
    pub fn prepare(
        epoch: u64,
        entries: impl IntoIterator<Item = SourceEntry>,
    ) -> Result<Self, PackagingError> {
        let mut output = BTreeMap::new();
        for entry in entries {
            if entry.mode.is_some_and(|mode| mode > 0o777) {
                return Err(PackagingError::InvalidPayloadMode(
                    entry.mode.unwrap_or_default(),
                ));
            }
            let path = normalize_archive_path(&entry.destination)?;
            let data = std::fs::read(&entry.source).map_err(|source| PackagingError::Read {
                path: entry.source.clone(),
                source,
            })?;
            let mode = entry.mode.unwrap_or_else(|| {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if std::fs::metadata(&entry.source)
                        .map(|m| m.permissions().mode() & 0o111 != 0)
                        .unwrap_or(false)
                    {
                        0o755
                    } else {
                        0o644
                    }
                }
                #[cfg(not(unix))]
                {
                    0o644
                }
            });
            let mut md5 = Md5::new();
            md5.update(&data);
            let mut sha = Sha256::new();
            sha.update(&data);
            if output
                .insert(
                    path.clone(),
                    PreparedEntry {
                        path: path.clone(),
                        kind: PreparedEntryKind::File,
                        data,
                        mode,
                        mtime: epoch,
                        md5: Some(digest_hex(md5.finalize().as_ref())),
                        sha256: Some(digest_hex(sha.finalize().as_ref())),
                    },
                )
                .is_some()
            {
                return Err(PackagingError::Collision(path));
            }
        }
        let files = output.values().cloned().collect::<Vec<_>>();
        for file in files {
            for parent in parent_paths(&file.path) {
                if let Some(existing) = output.get(&parent) {
                    if existing.kind != PreparedEntryKind::Directory {
                        return Err(PackagingError::Collision(parent));
                    }
                } else {
                    output.insert(
                        parent.clone(),
                        PreparedEntry {
                            path: parent,
                            kind: PreparedEntryKind::Directory,
                            data: Vec::new(),
                            mode: 0o755,
                            mtime: epoch,
                            md5: None,
                            sha256: None,
                        },
                    );
                }
            }
        }
        Ok(Self {
            epoch,
            entries: output.into_values().collect(),
        })
    }
}

pub fn normalized_tar_gz(
    entries: &[PreparedEntry],
    compression: Compression,
) -> Result<Vec<u8>, PackagingError> {
    let mut ordered = entries.to_vec();
    ordered.sort_by(|a, b| a.path.as_bytes().cmp(b.path.as_bytes()));
    let output = Vec::new();
    let encoder = GzBuilder::new()
        .mtime(0)
        .operating_system(255)
        .write(output, compression);
    let mut tar = Builder::new(encoder);
    for entry in ordered {
        let mut header = Header::new_ustar();
        header.set_path(&entry.path)?;
        header.set_mode(entry.mode);
        header.set_uid(0);
        header.set_gid(0);
        header.set_mtime(entry.mtime);
        header.set_username("root")?;
        header.set_groupname("root")?;
        match entry.kind {
            PreparedEntryKind::Directory => {
                header.set_entry_type(EntryType::Directory);
                header.set_size(0);
                header.set_cksum();
                tar.append(&header, io::empty())?;
            }
            PreparedEntryKind::File => {
                header.set_entry_type(EntryType::Regular);
                header.set_size(entry.data.len() as u64);
                header.set_cksum();
                tar.append(&header, entry.data.as_slice())?;
            }
        }
    }
    tar.finish()?;
    Ok(tar.into_inner()?.finish()?)
}

pub fn normalized_tar(entries: &[PreparedEntry]) -> Result<Vec<u8>, PackagingError> {
    let mut ordered = entries.to_vec();
    ordered.sort_by(|a, b| a.path.as_bytes().cmp(b.path.as_bytes()));
    let mut tar = Builder::new(Vec::new());
    for entry in ordered {
        let mut header = Header::new_ustar();
        header.set_path(&entry.path)?;
        header.set_mode(entry.mode);
        header.set_uid(0);
        header.set_gid(0);
        header.set_mtime(entry.mtime);
        header.set_username("root")?;
        header.set_groupname("root")?;
        match entry.kind {
            PreparedEntryKind::Directory => {
                header.set_entry_type(EntryType::Directory);
                header.set_size(0);
                header.set_cksum();
                tar.append(&header, io::empty())?;
            }
            PreparedEntryKind::File => {
                header.set_entry_type(EntryType::Regular);
                header.set_size(entry.data.len() as u64);
                header.set_cksum();
                tar.append(&header, entry.data.as_slice())?;
            }
        }
    }
    tar.finish()?;
    Ok(tar.into_inner()?)
}

/// Publish an artifact only after its complete deterministic encoding succeeds.
///
/// The temporary file is created beside the destination so persistence is a
/// same-filesystem replacement. A failed encoder or persist operation drops
/// the temporary file and leaves an existing artifact untouched.
pub fn write_atomically(
    output: &Path,
    write: impl FnOnce(&mut dyn Write) -> Result<(), PackagingError>,
) -> Result<(), PackagingError> {
    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut temporary = tempfile::Builder::new()
        .prefix(".evin-artifact-")
        .tempfile_in(parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        temporary
            .as_file_mut()
            .set_permissions(std::fs::Permissions::from_mode(0o644))?;
    }
    write(temporary.as_file_mut())?;
    temporary.as_file_mut().flush()?;
    temporary.as_file_mut().sync_all()?;
    temporary
        .persist(output)
        .map_err(|error| PackagingError::Archive(error.error))?;
    Ok(())
}

/// Summary of the payload successfully prepared and written into an artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedArtifactReport {
    pub files: usize,
    pub input_bytes: u64,
    pub inputs: Vec<String>,
}

/// Prepare a deterministic package payload and publish the encoded artifact
/// atomically. The writer receives the normalized payload and owns all
/// format-specific metadata and container rules.
pub fn prepare_and_write_atomically(
    output: &Path,
    epoch: u64,
    sources: impl IntoIterator<Item = SourceEntry>,
    write: impl FnOnce(&mut dyn Write, &PackageContext) -> Result<(), PackagingError>,
) -> Result<PreparedArtifactReport, PackagingError> {
    let context = PackageContext::prepare(epoch, sources)?;
    let report = PreparedArtifactReport {
        files: context
            .entries
            .iter()
            .filter(|entry| entry.kind == PreparedEntryKind::File)
            .count(),
        input_bytes: context
            .entries
            .iter()
            .filter(|entry| entry.kind == PreparedEntryKind::File)
            .map(|entry| entry.data.len() as u64)
            .sum(),
        inputs: context
            .entries
            .iter()
            .filter(|entry| entry.kind == PreparedEntryKind::File)
            .map(|entry| entry.path.clone())
            .collect(),
    };
    write_atomically(output, |writer| write(writer, &context))?;
    Ok(report)
}

/// Shared action-level output handling for archive and package builders.
/// Callers resolve metadata and payload sources and provide a format writer;
/// this function owns safe output resolution, dry-run outputs, reproducible
/// preparation, and atomic publication.
pub async fn publish_artifact(
    ctx: &crate::RunnerContext,
    path: String,
    format: String,
    unresolved_inputs: Vec<String>,
    sources: Vec<SourceEntry>,
    compression_level: Option<u32>,
    writer: impl FnOnce(&mut dyn Write, &PackageContext) -> Result<(), PackagingError> + Send + 'static,
) -> Result<PreparedArtifactReport, evin_diagnostic::Diagnostic> {
    if sources.is_empty() {
        if ctx.mode == crate::PipelineMode::DryRun && !ctx.uncertainty().is_empty() {
            ctx.mark_unverified(evin_locales::t!(
                "artifact-dry-run-inputs-unverified",
                includes = unresolved_inputs.join(", ")
            ));
            ctx.log
                .plan(crate::activity::ArtifactPlanUnresolved { path: path.clone() });
            ctx.state.set_output("path", &path);
            ctx.state.set_output("size", "unknown");
            return Ok(PreparedArtifactReport {
                files: 0,
                input_bytes: 0,
                inputs: Vec::new(),
            });
        }
        return Err(evin_locales::diagnostic(evin_diagnostic::IssueCode::new(
            "no-files-matched",
        )));
    }

    let output_path = ctx.fs.get_safe_physical_path(&path).map_err(|error| {
        evin_locales::diagnostic(
            evin_diagnostic::IssueCode::new("issue-artifact-safe-path-failed")
                .arg("context", "artifact path")
                .arg("error", error.to_string()),
        )
    })?;
    if ctx.mode == crate::PipelineMode::DryRun {
        ctx.log.plan(crate::activity::ArtifactPlan {
            files: sources.len(),
            path: path.clone(),
        });
        ctx.state.set_output("path", &path);
        ctx.state.set_output("size", "0");
        return Ok(PreparedArtifactReport {
            files: sources.len(),
            input_bytes: 0,
            inputs: Vec::new(),
        });
    }
    if let Some(parent) = output_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        tokio::fs::create_dir_all(parent).await.map_err(|error| {
            evin_locales::diagnostic(
                evin_diagnostic::IssueCode::new("create-output-dir-failed")
                    .arg("error", error.to_string()),
            )
        })?;
    }
    crate::action_event!(
        ctx.log,
        crate::activity::ArtifactPreparing {
            files: sources.len(),
            path: path.clone(),
        }
    );
    let epoch = match ctx.env.get("SOURCE_DATE_EPOCH") {
        Some(value) => parse_source_date_epoch(value).map_err(|error| {
            evin_locales::diagnostic(
                evin_diagnostic::IssueCode::new("artifact-source-date-epoch-invalid")
                    .arg("error", error.to_string()),
            )
        })?,
        None => ctx.git.head_commit_epoch().await.map_err(|error| {
            evin_locales::diagnostic(
                evin_diagnostic::IssueCode::new("artifact-source-date-epoch-unavailable")
                    .arg("error", error.to_string()),
            )
        })?,
    };
    let write_path = output_path.clone();
    let report = tokio::task::spawn_blocking(move || {
        prepare_and_write_atomically(&write_path, epoch, sources, writer)
            .map_err(evin_diagnostic::Diagnostic::from)
    })
    .await
    .map_err(|error| {
        evin_locales::diagnostic(
            evin_diagnostic::IssueCode::new("artifact-blocking-task-failed")
                .arg("error", error.to_string()),
        )
    })??;

    let output_bytes = tokio::fs::metadata(&output_path)
        .await
        .map_err(|error| {
            evin_locales::diagnostic(
                evin_diagnostic::IssueCode::new("artifact-output-size-unavailable")
                    .arg("path", path.as_str())
                    .arg("error", error.to_string()),
            )
        })?
        .len();
    ctx.state.set_output("path", &path);
    ctx.state.set_output("size", &output_bytes.to_string());
    ctx.log.activity(crate::activity::ArtifactCreated {
        format: format.clone(),
        path: path.clone(),
        input_files: report.files,
        input_size: format!("{} bytes", report.input_bytes),
        output_size: format!("{output_bytes} bytes"),
    });
    ctx.log.debug_event(crate::debug::ArtifactInputs {
        paths: report.inputs.join(", "),
    });
    ctx.log.debug_event(crate::debug::ArtifactOutput {
        format,
        path,
        size_bytes: output_bytes,
        input_bytes: report.input_bytes,
        files: report.files,
        compression_level,
    });
    Ok(report)
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    digest_hex(hasher.finalize().as_ref())
}

fn digest_hex(digest: &[u8]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn normalize_archive_path(path: &str) -> Result<String, PackagingError> {
    let path = path.replace('\\', "/");
    if path.is_empty() || path.starts_with('/') {
        return Err(PackagingError::InvalidPath(path));
    }
    let mut parts = Vec::new();
    for part in path.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." || part.contains('\0') {
            return Err(PackagingError::InvalidPath(path));
        }
        parts.push(part);
    }
    if parts.is_empty() {
        return Err(PackagingError::InvalidPath(path));
    }
    Ok(parts.join("/"))
}

pub fn normalize_install_dir(path: &str) -> Result<String, PackagingError> {
    if !path.starts_with('/') {
        return Err(PackagingError::InvalidPath(path.to_string()));
    }
    normalize_archive_path(path.trim_start_matches('/'))
}

fn parent_paths(path: &str) -> impl Iterator<Item = String> + '_ {
    let parts = path.split('/').collect::<Vec<_>>();
    (1..parts.len()).map(move |len| parts[..len].join("/"))
}

pub fn safe_relative_path(path: &str) -> Result<PathBuf, PackagingError> {
    let candidate = Path::new(path);
    if candidate.is_absolute()
        || candidate.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(PackagingError::InvalidPath(path.to_string()));
    }
    Ok(candidate.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn workspace_fs(root: &Path) -> WorkspaceFs {
        let (tx, _) = tokio::sync::mpsc::unbounded_channel();
        let secrets = std::sync::Arc::new(crate::secrets::SecretsProvider::default());
        let log = crate::Logger::new("packaging-test".into(), tx, secrets);
        WorkspaceFs::new(
            root.to_path_buf(),
            root.to_path_buf(),
            crate::PipelineMode::Normal,
            log,
        )
    }

    fn payload(source: &str, strip_prefix: &str, destination: &str) -> PayloadMapping {
        PayloadMapping {
            source: source.into(),
            strip_prefix: strip_prefix.into(),
            destination: destination.into(),
            mode: None,
        }
    }

    #[test]
    fn packaging_errors_render_through_fluent_diagnostics() {
        let cases = [
            (
                PackagingError::InvalidPath("../escape".into()),
                "package-path-invalid",
                evin_locales::t!("package-path-invalid", path = "../escape"),
                evin_locales::t!("package-path-invalid-title"),
            ),
            (
                PackagingError::Collision("usr/bin/tool".into()),
                "package-path-collision",
                evin_locales::t!("package-path-collision", path = "usr/bin/tool"),
                evin_locales::t!("package-path-collision-title"),
            ),
            (
                PackagingError::PayloadCollision("usr/bin/tool".into()),
                "package-payload-collision",
                evin_locales::t!("package-payload-collision", destination = "usr/bin/tool"),
                evin_locales::t!("package-payload-collision", destination = "usr/bin/tool"),
            ),
            (
                PackagingError::Read {
                    path: PathBuf::from("source.tar"),
                    source: io::Error::other("permission denied"),
                },
                "package-input-read-failed",
                evin_locales::t!(
                    "package-input-read-failed",
                    path = "source.tar",
                    error = "permission denied"
                ),
                evin_locales::t!("package-input-read-failed-title"),
            ),
            (
                PackagingError::InvalidEpoch("tomorrow".into()),
                "package-source-date-epoch-invalid",
                evin_locales::t!("package-source-date-epoch-invalid", value = "tomorrow"),
                evin_locales::t!("package-source-date-epoch-invalid-title"),
            ),
            (
                PackagingError::Archive(io::Error::other("disk full")),
                "package-archive-write-failed",
                evin_locales::t!("package-archive-write-failed", error = "disk full"),
                evin_locales::t!("package-archive-write-failed-title"),
            ),
        ];

        for (error, key, expected, expected_title) in cases {
            let diagnostic = evin_diagnostic::Diagnostic::from(error);
            assert_eq!(diagnostic.code.key, key);
            assert_eq!(evin_locales::render_issue(&diagnostic.code), expected);
            assert_eq!(diagnostic.title, expected_title);
        }
    }

    #[test]
    fn payload_resolution_maps_files_safely_and_deterministically() {
        let temp = TempDir::new().unwrap();
        std::fs::create_dir_all(temp.path().join("bin")).unwrap();
        std::fs::write(temp.path().join("bin/zeta"), "z").unwrap();
        std::fs::write(temp.path().join("bin/alpha"), "a").unwrap();
        let fs = workspace_fs(temp.path());

        let entries = resolve_payloads(&fs, &[payload("bin/*", "bin", "/usr/bin")]).unwrap();

        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.destination.as_str())
                .collect::<Vec<_>>(),
            ["usr/bin/alpha", "usr/bin/zeta"]
        );
        assert_eq!(
            entries[0].source,
            temp.path().join("bin/alpha").canonicalize().unwrap()
        );
        assert_eq!(entries[0].mode, None);
    }

    #[test]
    fn payload_resolution_rejects_empty_fields_invalid_modes_and_bad_globs() {
        let temp = TempDir::new().unwrap();
        let fs = workspace_fs(temp.path());

        assert!(matches!(
            resolve_payloads(&fs, &[payload("", "bin", "/usr/bin")]),
            Err(PackagingError::EmptyPayloadSourceOrPrefix)
        ));

        let mut invalid_mode = payload("bin/*", "bin", "/usr/bin");
        invalid_mode.mode = Some(0o1000);
        assert!(matches!(
            resolve_payloads(&fs, &[invalid_mode]),
            Err(PackagingError::InvalidPayloadMode(0o1000))
        ));

        assert!(matches!(
            resolve_payloads(&fs, &[payload("[", ".", "/usr")]),
            Err(PackagingError::PayloadSourceDiscovery { .. })
        ));
    }

    #[test]
    fn payload_resolution_rejects_strip_prefix_escapes_and_collisions() {
        let temp = TempDir::new().unwrap();
        std::fs::create_dir_all(temp.path().join("one")).unwrap();
        std::fs::create_dir_all(temp.path().join("two")).unwrap();
        std::fs::write(temp.path().join("one/tool"), "one").unwrap();
        std::fs::write(temp.path().join("two/tool"), "two").unwrap();
        let fs = workspace_fs(temp.path());

        assert!(matches!(
            resolve_payloads(&fs, &[payload("one/*", "two", "/usr/bin")]),
            Err(PackagingError::PayloadOutsideStripPrefix { .. })
        ));
        assert!(matches!(
            resolve_payloads(
                &fs,
                &[
                    payload("one/*", "one", "/usr/bin"),
                    payload("two/*", "two", "/usr/bin")
                ]
            ),
            Err(PackagingError::PayloadCollision(path)) if path == "usr/bin/tool"
        ));
    }

    #[cfg(unix)]
    #[test]
    fn payload_resolution_rejects_symlinks_that_escape_the_workspace() {
        let temp = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        std::fs::write(outside.path().join("secret"), "secret").unwrap();
        std::os::unix::fs::symlink(outside.path(), temp.path().join("linked")).unwrap();
        let fs = workspace_fs(temp.path());

        assert!(matches!(
            resolve_payloads(&fs, &[payload("linked/*", "linked", "/usr/share")]),
            Err(PackagingError::UnsafePayloadSource { .. })
        ));
    }

    #[test]
    fn prepared_payload_rejects_modes_outside_unix_permission_bits() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("input");
        std::fs::write(&file, "input").unwrap();

        assert!(matches!(
            PackageContext::prepare(
                1,
                [SourceEntry {
                    source: file,
                    destination: "usr/bin/tool".into(),
                    mode: Some(0o1000),
                }]
            ),
            Err(PackagingError::InvalidPayloadMode(0o1000))
        ));
    }

    #[test]
    fn synthesized_directories_are_sorted_and_normalized() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("example");
        std::fs::write(&file, "x").unwrap();
        let context = PackageContext::prepare(
            42,
            [SourceEntry {
                source: file,
                destination: "usr/bin/example".into(),
                mode: Some(0o755),
            }],
        )
        .unwrap();
        assert_eq!(
            context
                .entries
                .iter()
                .map(|entry| entry.path.as_str())
                .collect::<Vec<_>>(),
            ["usr", "usr/bin", "usr/bin/example"]
        );
        assert_eq!(context.entries[0].mode, 0o755);
        assert_eq!(context.entries[0].mtime, 42);
    }

    #[test]
    fn gzip_bytes_are_reproducible() {
        let entries = vec![
            PreparedEntry {
                path: "b".into(),
                kind: PreparedEntryKind::File,
                data: b"b".to_vec(),
                mode: 0o644,
                mtime: 1,
                md5: None,
                sha256: None,
            },
            PreparedEntry {
                path: "a".into(),
                kind: PreparedEntryKind::File,
                data: b"a".to_vec(),
                mode: 0o644,
                mtime: 1,
                md5: None,
                sha256: None,
            },
        ];
        assert_eq!(
            normalized_tar_gz(&entries, Compression::best()).unwrap(),
            normalized_tar_gz(&entries, Compression::best()).unwrap()
        );
    }

    #[test]
    fn atomic_write_replaces_only_a_complete_artifact() {
        let temp = TempDir::new().unwrap();
        let output = temp.path().join("artifact");
        std::fs::write(&output, b"old").unwrap();

        write_atomically(&output, |writer| {
            writer.write_all(b"new")?;
            Ok(())
        })
        .unwrap();

        assert_eq!(std::fs::read(&output).unwrap(), b"new");
        assert!(std::fs::read_dir(temp.path()).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".evin-artifact-")
        }));
    }

    #[test]
    fn atomic_write_failure_preserves_existing_artifact_and_cleans_up() {
        let temp = TempDir::new().unwrap();
        let output = temp.path().join("artifact");
        std::fs::write(&output, b"old").unwrap();

        let error = write_atomically(&output, |writer| {
            writer.write_all(b"partial")?;
            Err(PackagingError::InvalidPath("injected failure".into()))
        });

        assert!(error.is_err());
        assert_eq!(std::fs::read(&output).unwrap(), b"old");
        assert!(std::fs::read_dir(temp.path()).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".evin-artifact-")
        }));
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_publishes_readable_non_executable_artifacts() {
        use std::os::unix::fs::PermissionsExt;

        let temp = TempDir::new().unwrap();
        let output = temp.path().join("artifact");
        write_atomically(&output, |writer| {
            writer.write_all(b"artifact")?;
            Ok(())
        })
        .unwrap();

        assert_eq!(
            std::fs::metadata(output).unwrap().permissions().mode() & 0o777,
            0o644
        );
    }

    #[test]
    fn prepared_artifact_report_counts_files_and_preserves_previous_output_on_failure() {
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("input");
        let output = temp.path().join("artifact");
        std::fs::write(&source, b"payload").unwrap();
        std::fs::write(&output, b"old").unwrap();

        let report = prepare_and_write_atomically(
            &output,
            42,
            [SourceEntry {
                source,
                destination: "usr/bin/example".into(),
                mode: Some(0o755),
            }],
            |writer, payload| {
                assert_eq!(payload.entries[0].path, "usr");
                writer.write_all(b"complete")?;
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(report.files, 1);
        assert_eq!(report.input_bytes, 7);
        assert_eq!(report.inputs, ["usr/bin/example"]);
        assert_eq!(std::fs::read(&output).unwrap(), b"complete");

        let failure = prepare_and_write_atomically(
            &output,
            42,
            [SourceEntry {
                source: temp.path().join("input"),
                destination: "usr/bin/example".into(),
                mode: None,
            }],
            |writer, _| {
                writer.write_all(b"partial")?;
                Err(PackagingError::InvalidPath("injected failure".into()))
            },
        );
        assert!(failure.is_err());
        assert_eq!(std::fs::read(output).unwrap(), b"complete");
    }
}
