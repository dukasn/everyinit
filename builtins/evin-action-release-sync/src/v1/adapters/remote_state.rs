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
            return Err(ProtocolError::StateIngestion(format!(
                "Remote state file not found at '{}'",
                full_path.display()
            )));
        }
        let content = std::fs::read_to_string(&full_path).map_err(|e| {
            ProtocolError::StateIngestion(format!("Failed to read state file: {e}"))
        })?;
        let state: RemoteReleaseState = serde_json::from_str(&content)
            .map_err(|e| ProtocolError::SchemaValidation(format!("Invalid state JSON: {e}")))?;
        Ok(state)
    }
}
