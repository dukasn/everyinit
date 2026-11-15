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

use evin_release_sync::{ProtocolError, RemoteReleaseState, RemoteStateSource};
use std::path::Path;

pub struct FsRemoteStateSource<'a> {
    pub workspace_root: &'a Path,
    pub state_path: &'a str,
}

impl<'a> RemoteStateSource for FsRemoteStateSource<'a> {
    fn load_remote_state(&self) -> Result<RemoteReleaseState, ProtocolError> {
        let full_path = self.workspace_root.join(self.state_path);
        if !full_path.exists() {
            return Err(ProtocolError::StateIngestion(evin_locales::t!(
                "release-sync-state-file-not-found",
                path = full_path.display().to_string()
            )));
        }
        let content = std::fs::read_to_string(&full_path).map_err(|e| {
            ProtocolError::StateIngestion(evin_locales::t!(
                "release-sync-state-file-read-failed",
                error = e.to_string()
            ))
        })?;
        let state: RemoteReleaseState = serde_json::from_str(&content).map_err(|e| {
            ProtocolError::SchemaValidation(evin_locales::t!(
                "release-sync-state-json-invalid",
                error = e.to_string()
            ))
        })?;
        Ok(state)
    }
}
