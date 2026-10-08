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

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn copy_tree(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).expect("create locale output directory");
    for entry in fs::read_dir(source).expect("read locale source directory") {
        let entry = entry.expect("read locale source entry");
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if source_path.is_dir() {
            copy_tree(&source_path, &destination_path);
        } else {
            fs::copy(&source_path, &destination_path).expect("copy locale resource");
        }
    }
}

fn main() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let locale_source = manifest_dir
        .join("../../locales")
        .canonicalize()
        .expect("resolve locale source directory");
    println!("cargo:rerun-if-changed={}", locale_source.display());

    let output_dir = PathBuf::from(env::var_os("OUT_DIR").expect("output dir")).join("locales");
    copy_tree(&locale_source, &output_dir);
    println!("cargo:rustc-env=EVIN_LOCALES_DIR={}", output_dir.display());
}
