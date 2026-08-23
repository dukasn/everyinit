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

use std::path::{Path, PathBuf};

/// Represents a validated file ready to be streamed into an archive.
#[derive(Debug)]
pub struct ArchiveEntry {
    /// The physical path on the local OS disk
    pub source: PathBuf,
    /// The computed path inside the archive (after layout/prefix handling)
    pub destination: PathBuf,
}

/// The core Strategy trait for format-agnostic compression
pub trait Archiver: Send + Sync {
    fn create_archive(
        &self,
        output_path: &Path,
        files: &[ArchiveEntry],
    ) -> Result<(), evin_diagnostic::Diagnostic>;
}
