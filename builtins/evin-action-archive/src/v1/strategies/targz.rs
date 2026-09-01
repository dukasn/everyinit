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

use crate::v1::archiver::Archiver;
use evin_action_sdk::packaging::{PackageContext, normalized_tar_gz};
use evin_diagnostic::{Diagnostic, IssueCode};
use flate2::Compression;
use std::path::Path;

pub struct TarGzArchiver {
    pub compression_level: Option<u32>,
}

impl Archiver for TarGzArchiver {
    fn create_archive(
        &self,
        output_path: &Path,
        context: &PackageContext,
    ) -> Result<(), Diagnostic> {
        let compression = self
            .compression_level
            .map(Compression::new)
            .unwrap_or_default();

        let bytes = normalized_tar_gz(&context.entries, compression).map_err(|e| {
            evin_locales::diagnostic(
                IssueCode::new("finalize-archive-failed").arg("error", e.to_string()),
            )
        })?;
        std::fs::write(output_path, bytes).map_err(|e| {
            evin_locales::diagnostic(
                IssueCode::new("create-output-file-failed").arg("error", e.to_string()),
            )
        })?;
        Ok(())
    }
}
