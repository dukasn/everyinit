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

use crate::v1::archiver::{ArchiveEntry, Archiver};
use evin_diagnostic::{Diagnostic, IssueCode};
use flate2::Compression;
use flate2::write::GzEncoder;
use std::fs::File;
use std::path::Path;

pub struct TarGzArchiver {
    pub compression_level: Option<u32>,
}

impl Archiver for TarGzArchiver {
    fn create_archive(&self, output_path: &Path, files: &[ArchiveEntry]) -> Result<(), Diagnostic> {
        let file = File::create(output_path).map_err(|e| {
            evin_locales::diagnostic(
                IssueCode::new("create-output-file-failed").arg("error", e.to_string()),
            )
        })?;

        let compression = self
            .compression_level
            .map(Compression::new)
            .unwrap_or_default();

        let enc = GzEncoder::new(file, compression);
        let mut builder = tar::Builder::new(enc);

        for file_target in files {
            builder
                .append_path_with_name(&file_target.source, &file_target.destination)
                .map_err(|e| {
                    evin_locales::diagnostic(
                        IssueCode::new("archive-failed")
                            .arg("path", file_target.destination.display().to_string())
                            .arg("error", e.to_string()),
                    )
                })?;
        }

        builder.finish().map_err(|e| {
            evin_locales::diagnostic(
                IssueCode::new("finalize-archive-failed").arg("error", e.to_string()),
            )
        })?;

        Ok(())
    }
}
