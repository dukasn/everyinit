// Copyright (C) 2026 Eduardo Marques Santos
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.

use evin_action_sdk::packaging::{
    PackageContext, PreparedEntry, PreparedEntryKind, normalized_tar,
};
use std::io::Write;

pub(crate) struct ArchPackage<'a> {
    pub(crate) context: &'a PackageContext,
    pub(crate) name: &'a str,
    pub(crate) version: &'a str,
    pub(crate) revision: u32,
    pub(crate) architecture: &'a str,
    pub(crate) summary: &'a str,
    pub(crate) homepage: Option<&'a str>,
    pub(crate) maintainer: &'a str,
    pub(crate) license: &'a str,
    pub(crate) depends: &'a [String],
    pub(crate) optdepends: &'a [String],
    pub(crate) provides: &'a [String],
    pub(crate) conflicts: &'a [String],
    pub(crate) replaces: &'a [String],
    pub(crate) backup: &'a [String],
    pub(crate) install: Option<&'a [u8]>,
}

pub fn write(
    output: &mut dyn Write,
    package: ArchPackage<'_>,
) -> Result<(), evin_action_sdk::packaging::PackagingError> {
    let mut entries = package.context.entries.clone();
    entries.extend(metadata_entries(&package));
    let tar = normalized_tar(&entries)?;
    let encoded = zstandard::encode_all(&tar).map_err(|error| {
        evin_action_sdk::packaging::PackagingError::Archive(std::io::Error::other(
            error.to_string(),
        ))
    })?;
    output.write_all(&encoded)?;
    Ok(())
}

fn metadata_entries(package: &ArchPackage<'_>) -> Vec<PreparedEntry> {
    let mut entries = vec![
        PreparedEntry {
            path: ".PKGINFO".into(),
            kind: PreparedEntryKind::File,
            data: render_pkginfo(package),
            mode: 0o644,
            mtime: package.context.epoch,
            md5: None,
            sha256: None,
        },
        PreparedEntry {
            path: ".MTREE".into(),
            kind: PreparedEntryKind::File,
            data: render_mtree(package.context),
            mode: 0o644,
            mtime: package.context.epoch,
            md5: None,
            sha256: None,
        },
    ];
    if let Some(data) = package.install {
        entries.push(PreparedEntry {
            path: ".INSTALL".into(),
            kind: PreparedEntryKind::File,
            data: data.to_vec(),
            mode: 0o755,
            mtime: package.context.epoch,
            md5: None,
            sha256: None,
        });
    }
    entries
}

fn render_pkginfo(package: &ArchPackage<'_>) -> Vec<u8> {
    let installed_size = package
        .context
        .entries
        .iter()
        .filter(|entry| entry.kind == PreparedEntryKind::File)
        .map(|entry| entry.data.len() as u64)
        .sum::<u64>();
    let mut pkginfo = format!(
        "pkgname = {}\npkgbase = {}\nxdata = pkgtype=pkg\npkgver = {}-{}\npkgdesc = {}\nurl = {}\nbuilddate = {}\npackager = {}\nsize = {}\narch = {}\nlicense = {}\n",
        package.name,
        package.name,
        package.version,
        package.revision,
        package.summary,
        package.homepage.unwrap_or(""),
        package.context.epoch,
        package.maintainer,
        installed_size,
        package.architecture,
        package.license,
    );
    for (key, values) in [
        ("depend", package.depends),
        ("optdepend", package.optdepends),
        ("provides", package.provides),
        ("conflict", package.conflicts),
        ("replaces", package.replaces),
        ("backup", package.backup),
    ] {
        for value in sorted_unique(values) {
            pkginfo.push_str(&format!("{key} = {value}\n"));
        }
    }
    pkginfo.into_bytes()
}

fn render_mtree(context: &PackageContext) -> Vec<u8> {
    let mut mtree = String::from("#mtree\n/set uid=0 gid=0\n");
    for entry in &context.entries {
        let kind = if entry.kind == PreparedEntryKind::Directory {
            "dir"
        } else {
            "file"
        };
        mtree.push_str(&format!(
            "./{} type={} mode={:o} time={}",
            entry.path, kind, entry.mode, entry.mtime
        ));
        if let Some(digest) = &entry.sha256 {
            mtree.push_str(&format!(" sha256digest={digest}"));
        }
        mtree.push('\n');
    }
    mtree.into_bytes()
}

fn sorted_unique(values: &[String]) -> Vec<&str> {
    let mut values = values.iter().map(String::as_str).collect::<Vec<_>>();
    values.sort_unstable();
    values.dedup();
    values
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> PackageContext {
        PackageContext {
            epoch: 123,
            entries: vec![
                PreparedEntry {
                    path: "usr".into(),
                    kind: PreparedEntryKind::Directory,
                    data: Vec::new(),
                    mode: 0o755,
                    mtime: 123,
                    md5: None,
                    sha256: None,
                },
                PreparedEntry {
                    path: "usr/bin/example".into(),
                    kind: PreparedEntryKind::File,
                    data: b"payload".to_vec(),
                    mode: 0o755,
                    mtime: 123,
                    md5: None,
                    sha256: Some("digest".into()),
                },
            ],
        }
    }

    fn package<'a>(
        context: &'a PackageContext,
        depends: &'a [String],
        install: Option<&'a [u8]>,
    ) -> ArchPackage<'a> {
        ArchPackage {
            context,
            name: "example",
            version: "1.2.3",
            revision: 4,
            architecture: "x86_64",
            summary: "Example package",
            homepage: Some("https://example.test"),
            maintainer: "Example <example@example.test>",
            license: "MIT",
            depends,
            optdepends: &[],
            provides: &[],
            conflicts: &[],
            replaces: &[],
            backup: &[],
            install,
        }
    }

    #[test]
    fn pkginfo_and_mtree_are_canonical() {
        let context = context();
        let depends = vec!["zlib".into(), "glibc".into(), "zlib".into()];
        let artifact = package(&context, &depends, None);

        assert_eq!(
            String::from_utf8(render_pkginfo(&artifact)).unwrap(),
            "pkgname = example\npkgbase = example\nxdata = pkgtype=pkg\npkgver = 1.2.3-4\npkgdesc = Example package\nurl = https://example.test\nbuilddate = 123\npackager = Example <example@example.test>\nsize = 7\narch = x86_64\nlicense = MIT\ndepend = glibc\ndepend = zlib\n"
        );
        assert_eq!(
            String::from_utf8(render_mtree(&context)).unwrap(),
            "#mtree\n/set uid=0 gid=0\n./usr type=dir mode=755 time=123\n./usr/bin/example type=file mode=755 time=123 sha256digest=digest\n"
        );
    }

    #[test]
    fn metadata_entries_and_encoded_tar_preserve_normalized_headers() {
        let context = context();
        let depends = vec!["glibc".into()];
        let artifact = package(&context, &depends, Some(b"post_install() { :; }\n"));
        let metadata = metadata_entries(&artifact);
        assert_eq!(
            metadata
                .iter()
                .map(|entry| entry.path.as_str())
                .collect::<Vec<_>>(),
            [".PKGINFO", ".MTREE", ".INSTALL"]
        );
        assert_eq!(metadata[2].mode, 0o755);

        let mut first = Vec::new();
        write(&mut first, artifact).unwrap();
        let mut second = Vec::new();
        write(
            &mut second,
            package(&context, &depends, Some(b"post_install() { :; }\n")),
        )
        .unwrap();
        assert_eq!(first, second);

        let tar = zstandard::decode_all(first.as_slice()).unwrap();
        let mut archive = tar::Archive::new(std::io::Cursor::new(tar));
        for entry in archive.entries().unwrap() {
            let entry = entry.unwrap();
            assert_eq!(entry.header().uid().unwrap(), 0);
            assert_eq!(entry.header().gid().unwrap(), 0);
            assert_eq!(entry.header().mtime().unwrap(), 123);
        }
    }
}
