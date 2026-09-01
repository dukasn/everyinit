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
    PackageContext, PreparedEntry, PreparedEntryKind, normalized_tar_gz, sha256_hex,
};
use flate2::Compression;
use std::io::Write;
use std::path::Path;

pub struct ApkPackage<'a> {
    pub context: &'a PackageContext,
    pub name: &'a str,
    pub version: &'a str,
    pub revision: u32,
    pub architecture: &'a str,
    pub summary: &'a str,
    pub homepage: Option<&'a str>,
    pub maintainer: &'a str,
    pub license: &'a str,
    pub depends: Vec<String>,
    pub scripts: Vec<(String, Vec<u8>)>,
}

pub fn write(
    output: &Path,
    mut package: ApkPackage<'_>,
) -> Result<(), evin_action_sdk::packaging::PackagingError> {
    let data = normalized_tar_gz(&package.context.entries, Compression::best())?;
    package.depends.sort();
    package.depends.dedup();
    let installed_size = package
        .context
        .entries
        .iter()
        .filter(|entry| entry.kind == PreparedEntryKind::File)
        .map(|entry| entry.data.len() as u64)
        .sum::<u64>();
    let mut pkginfo = format!(
        "pkgname = {}\npkgver = {}-r{}\npkgdesc = {}\narch = {}\nbuilddate = {}\npackager = {}\nsize = {}\nlicense = {}\ndatahash = {}\n",
        package.name,
        package.version,
        package.revision,
        package.summary,
        package.architecture,
        package.context.epoch,
        package.maintainer,
        installed_size,
        package.license,
        sha256_hex(&data),
    );
    if let Some(homepage) = package.homepage {
        pkginfo.push_str(&format!("url = {homepage}\n"));
    }
    for dependency in package.depends {
        pkginfo.push_str(&format!("depend = {dependency}\n"));
    }
    let mut control = vec![PreparedEntry {
        path: ".PKGINFO".into(),
        kind: PreparedEntryKind::File,
        data: pkginfo.into_bytes(),
        mode: 0o644,
        mtime: package.context.epoch,
        md5: None,
        sha256: None,
    }];
    for (path, data) in package.scripts {
        control.push(PreparedEntry {
            path,
            kind: PreparedEntryKind::File,
            data,
            mode: 0o755,
            mtime: package.context.epoch,
            md5: None,
            sha256: None,
        });
    }
    let control = normalized_tar_gz(&control, Compression::best())?;
    let mut file = std::fs::File::create(output)?;
    file.write_all(&control)?;
    file.write_all(&data)?;
    Ok(())
}
