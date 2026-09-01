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

use evin_action_sdk::packaging::{
    PackageContext, PreparedEntry, PreparedEntryKind, normalized_tar_gz,
};
use flate2::Compression;
use std::io::Write;
use std::path::Path;

pub struct DebPackage<'a> {
    pub context: &'a PackageContext,
    pub name: &'a str,
    pub version: &'a str,
    pub revision: u32,
    pub architecture: &'a str,
    pub summary: &'a str,
    pub description: Option<&'a str>,
    pub maintainer: &'a str,
    pub homepage: Option<&'a str>,
    pub depends: Vec<String>,
    pub scripts: Vec<(String, Vec<u8>)>,
}

pub fn write(
    output: &Path,
    mut package: DebPackage<'_>,
) -> Result<(), evin_action_sdk::packaging::PackagingError> {
    let data = normalized_tar_gz(&package.context.entries, Compression::best())?;
    let mut control = format!(
        "Package: {}\nVersion: {}-{}\nArchitecture: {}\nMaintainer: {}\nDescription: {}\n",
        package.name,
        package.version,
        package.revision,
        package.architecture,
        package.maintainer,
        package.summary,
    );
    if let Some(description) = package.description {
        for line in description.lines() {
            control.push_str(&format!(" {line}\n"));
        }
    }
    if let Some(homepage) = package.homepage {
        control.push_str(&format!("Homepage: {homepage}\n"));
    }
    package.depends.sort();
    package.depends.dedup();
    if !package.depends.is_empty() {
        control.push_str(&format!("Depends: {}\n", package.depends.join(", ")));
    }
    let mut control_entries = vec![PreparedEntry {
        path: "control".into(),
        kind: PreparedEntryKind::File,
        data: control.into_bytes(),
        mode: 0o644,
        mtime: package.context.epoch,
        md5: None,
        sha256: None,
    }];
    let md5sums = package
        .context
        .entries
        .iter()
        .filter(|entry| entry.kind == PreparedEntryKind::File)
        .filter_map(|entry| {
            entry
                .md5
                .as_ref()
                .map(|digest| format!("{digest}  {}\n", entry.path))
        })
        .collect::<String>();
    control_entries.push(PreparedEntry {
        path: "md5sums".into(),
        kind: PreparedEntryKind::File,
        data: md5sums.into_bytes(),
        mode: 0o644,
        mtime: package.context.epoch,
        md5: None,
        sha256: None,
    });
    for (path, data) in package.scripts {
        control_entries.push(PreparedEntry {
            path,
            kind: PreparedEntryKind::File,
            data,
            mode: 0o755,
            mtime: package.context.epoch,
            md5: None,
            sha256: None,
        });
    }
    let control = normalized_tar_gz(&control_entries, Compression::best())?;
    let mut file = std::fs::File::create(output)?;
    file.write_all(b"!<arch>\n")?;
    append_ar_member(&mut file, "debian-binary", b"2.0\n", package.context.epoch)?;
    append_ar_member(&mut file, "control.tar.gz", &control, package.context.epoch)?;
    append_ar_member(&mut file, "data.tar.gz", &data, package.context.epoch)?;
    Ok(())
}

fn append_ar_member(
    mut writer: impl Write,
    name: &str,
    bytes: &[u8],
    epoch: u64,
) -> std::io::Result<()> {
    let mode = format!("{:o}", 0o100644);
    let header = format!(
        "{name:<16}{epoch:<12}{uid:<6}{gid:<6}{mode:<8}{size:<10}`\n",
        uid = 0,
        gid = 0,
        size = bytes.len(),
    );
    writer.write_all(header.as_bytes())?;
    writer.write_all(bytes)?;
    if !bytes.len().is_multiple_of(2) {
        writer.write_all(b"\n")?;
    }
    Ok(())
}
