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

use evin_release_sync::{ArtifactInspector, LocalArtifactInfo, ProtocolError};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;

pub struct FsArtifactInspector<'a> {
    pub workspace_root: &'a Path,
    pub artifacts_dir: &'a str,
}

impl<'a> ArtifactInspector for FsArtifactInspector<'a> {
    fn local_artifacts(&self, _tag: &str) -> Result<Vec<LocalArtifactInfo>, ProtocolError> {
        let dir = self.workspace_root.join(self.artifacts_dir);
        if !dir.is_dir() {
            return Ok(Vec::new());
        }

        let mut artifacts = Vec::new();
        let entries = std::fs::read_dir(&dir).map_err(|e| {
            ProtocolError::ArtifactInspection(evin_locales::t!(
                "release-sync-artifacts-directory-read-failed",
                error = e.to_string()
            ))
        })?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                let metadata = entry.metadata().map_err(|e| {
                    ProtocolError::ArtifactInspection(evin_locales::t!(
                        "release-sync-artifact-metadata-read-failed",
                        error = e.to_string()
                    ))
                })?;
                let mut file = std::fs::File::open(&path).map_err(|e| {
                    ProtocolError::ArtifactInspection(evin_locales::t!(
                        "release-sync-artifact-open-failed",
                        error = e.to_string()
                    ))
                })?;
                let mut hasher = Sha256::new();
                let mut buffer = [0; 64 * 1024];
                loop {
                    let read = file.read(&mut buffer).map_err(|e| {
                        ProtocolError::ArtifactInspection(evin_locales::t!(
                            "release-sync-artifact-hash-failed",
                            error = e.to_string()
                        ))
                    })?;
                    if read == 0 {
                        break;
                    }
                    hasher.update(&buffer[..read]);
                }
                let file_name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();
                let relative = path
                    .strip_prefix(self.workspace_root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .to_string();

                artifacts.push(LocalArtifactInfo {
                    local_path: relative,
                    destination_name: file_name,
                    size_bytes: metadata.len(),
                    sha256: Some(format!("{:x}", hasher.finalize())),
                });
            }
        }

        artifacts.sort_by(|left, right| left.destination_name.cmp(&right.destination_name));
        Ok(artifacts)
    }
}
