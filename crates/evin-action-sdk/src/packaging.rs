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

use flate2::{Compression, GzBuilder};
use md5::{Digest as Md5Digest, Md5};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::BTreeMap;
use std::io::{self};
use std::path::{Component, Path, PathBuf};
use tar::{Builder, EntryType, Header};

#[derive(Debug, thiserror::Error)]
pub enum PackagingError {
    #[error("invalid package path '{0}'")]
    InvalidPath(String),
    #[error("package path collision at '{0}'")]
    Collision(String),
    #[error("cannot read package input '{path}': {source}")]
    Read { path: PathBuf, source: io::Error },
    #[error("invalid SOURCE_DATE_EPOCH: {0}")]
    InvalidEpoch(String),
    #[error("unknown package token '{{{{{0}}}}}'")]
    UnknownToken(String),
    #[error("unterminated package token")]
    UnterminatedToken,
    #[error("archive writer failed: {0}")]
    Archive(#[from] io::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

impl LinuxArch {
    pub fn deb(self) -> &'static str {
        match self {
            Self::X86_64 => "amd64",
            Self::Aarch64 => "arm64",
            Self::X86 => "i386",
            Self::Armv7 => "armhf",
            Self::Riscv64 => "riscv64",
            Self::Ppc64le => "ppc64el",
            Self::S390x => "s390x",
            Self::Noarch => "all",
        }
    }

    pub fn apk(self) -> &'static str {
        match self {
            Self::X86_64 => "x86_64",
            Self::Aarch64 => "aarch64",
            Self::X86 => "x86",
            Self::Armv7 => "armv7",
            Self::Riscv64 => "riscv64",
            Self::Ppc64le => "ppc64le",
            Self::S390x => "s390x",
            Self::Noarch => "noarch",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageFormat {
    TarGz,
    Deb,
    Apk,
}

impl PackageFormat {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "tar.gz" => Some(Self::TarGz),
            "deb" => Some(Self::Deb),
            "apk" => Some(Self::Apk),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::TarGz => "tar.gz",
            Self::Deb => "deb",
            Self::Apk => "apk",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PackageManifestSource {
    pub root: PathBuf,
    pub path: PathBuf,
    pub manifest: PackageManifest,
}

#[derive(Debug, Clone, Default)]
pub struct PackageManifestCatalog {
    manifests: Vec<PackageManifestSource>,
}

impl PackageManifestCatalog {
    pub fn new(mut manifests: Vec<PackageManifestSource>) -> Self {
        manifests.sort_by(|left, right| left.root.cmp(&right.root));
        Self { manifests }
    }

    pub fn manifests(&self) -> &[PackageManifestSource] {
        &self.manifests
    }

    pub fn select(&self, task_cwd: &Path) -> Option<&PackageManifestSource> {
        self.manifests
            .iter()
            .filter(|entry| task_cwd.starts_with(&entry.root))
            .max_by_key(|entry| entry.root.components().count())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageManifest {
    pub version: u32,
    pub identity: PackageIdentity,
    #[serde(default)]
    pub payload: Vec<PayloadMapping>,
    #[serde(default)]
    pub targets: PackageTargets,
}

impl PackageManifest {
    pub fn validate(&self) -> Result<(), PackagingError> {
        if self.version != 1
            || self.identity.name.trim().is_empty()
            || self.identity.name.contains("{{")
        {
            return Err(PackagingError::InvalidPath(
                "package identity or manifest version".into(),
            ));
        }
        if self.identity.summary.trim().is_empty()
            || self.identity.maintainer.trim().is_empty()
            || self.identity.license.trim().is_empty()
        {
            return Err(PackagingError::InvalidPath(
                "required package identity field".into(),
            ));
        }
        let token_check = PackageTokens {
            name: self.identity.name.clone(),
            version: "version".into(),
            arch: "arch".into(),
            revision: 1,
            maintainer: self.identity.maintainer.clone(),
            summary: self.identity.summary.clone(),
        };
        for value in [
            &self.identity.summary,
            &self.identity.maintainer,
            &self.identity.license,
        ]
        .into_iter()
        .chain(self.identity.description.iter())
        .chain(self.identity.homepage.iter())
        {
            render_intrinsic_tokens(value, &token_check)?;
        }
        for mapping in &self.payload {
            if mapping.source.trim().is_empty() || mapping.strip_prefix.trim().is_empty() {
                return Err(PackagingError::InvalidPath(
                    "empty payload source or strip_prefix".into(),
                ));
            }
            normalize_install_dir(&mapping.destination)?;
            render_intrinsic_tokens(&mapping.source, &token_check)?;
            render_intrinsic_tokens(&mapping.strip_prefix, &token_check)?;
            render_intrinsic_tokens(&mapping.destination, &token_check)?;
            if let Some(mode) = mapping.mode
                && mode > 0o777
            {
                return Err(PackagingError::InvalidPath(format!(
                    "invalid file mode {mode:o}"
                )));
            }
        }
        if let Some(target) = &self.targets.deb {
            for dependency in &target.depends {
                render_intrinsic_tokens(dependency, &token_check)?;
            }
        }
        if let Some(target) = &self.targets.apk {
            for dependency in &target.depends {
                render_intrinsic_tokens(dependency, &token_check)?;
            }
        }
        Ok(())
    }
}

impl evin_yaml::YamlDocument for PackageManifest {}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageIdentity {
    pub name: String,
    pub summary: String,
    #[serde(default)]
    pub description: Option<String>,
    pub maintainer: String,
    pub license: String,
    #[serde(default)]
    pub homepage: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PayloadMapping {
    pub source: String,
    pub strip_prefix: String,
    pub destination: String,
    #[serde(default)]
    pub mode: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageTargets {
    #[serde(default)]
    pub deb: Option<DebTarget>,
    #[serde(default)]
    pub apk: Option<ApkTarget>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DebTarget {
    #[serde(default)]
    pub depends: Vec<String>,
    #[serde(default)]
    pub scripts: DebScripts,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DebScripts {
    pub preinst: Option<String>,
    pub postinst: Option<String>,
    pub prerm: Option<String>,
    pub postrm: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApkTarget {
    #[serde(default)]
    pub depends: Vec<String>,
    #[serde(default)]
    pub scripts: ApkScripts,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApkScripts {
    pub pre_install: Option<String>,
    pub post_install: Option<String>,
    pub pre_deinstall: Option<String>,
    pub post_deinstall: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PackageTokens {
    pub name: String,
    pub version: String,
    pub arch: String,
    pub revision: u32,
    pub maintainer: String,
    pub summary: String,
}

pub fn render_intrinsic_tokens(
    value: &str,
    tokens: &PackageTokens,
) -> Result<String, PackagingError> {
    let mut rendered = String::with_capacity(value.len());
    let mut remaining = value;
    while let Some(start) = remaining.find("{{") {
        rendered.push_str(&remaining[..start]);
        let after_start = &remaining[start + 2..];
        let Some(end) = after_start.find("}}") else {
            return Err(PackagingError::UnterminatedToken);
        };
        let token = after_start[..end].trim();
        let replacement = match token {
            "name" => &tokens.name,
            "version" => &tokens.version,
            "arch" => &tokens.arch,
            "revision" => {
                rendered.push_str(&tokens.revision.to_string());
                remaining = &after_start[end + 2..];
                continue;
            }
            "maintainer" => &tokens.maintainer,
            "summary" => &tokens.summary,
            other => return Err(PackagingError::UnknownToken(other.to_string())),
        };
        rendered.push_str(replacement);
        remaining = &after_start[end + 2..];
    }
    rendered.push_str(remaining);
    Ok(rendered)
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
                        md5: Some(format!("{:x}", md5.finalize())),
                        sha256: Some(format!("{:x}", sha.finalize())),
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

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
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
}
