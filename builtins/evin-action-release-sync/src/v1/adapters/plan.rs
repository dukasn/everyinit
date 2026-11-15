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

use super::super::config::ReleaseSyncConfig;
use evin_release_sync::{PlanSink, ProtocolError, ReleaseSyncPlan};
use std::path::{Path, PathBuf};

pub struct FsPlanSink<'a> {
    pub workspace_root: &'a Path,
    pub config: &'a ReleaseSyncConfig,
    pub written_notes: Vec<PathBuf>,
}

impl<'a> FsPlanSink<'a> {
    pub fn new(workspace_root: &'a Path, config: &'a ReleaseSyncConfig) -> Self {
        Self {
            workspace_root,
            config,
            written_notes: Vec::new(),
        }
    }
}

impl<'a> PlanSink for FsPlanSink<'a> {
    fn write_notes_file(&mut self, filename: &str, content: &str) -> Result<String, ProtocolError> {
        let relative_path = Path::new(&self.config.notes_dir).join(filename);
        let full_path = self.workspace_root.join(&relative_path);

        if !self.config.dry_run {
            if let Some(parent) = full_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    ProtocolError::PlanWrite(format!(
                        "Failed to create notes dir '{}': {e}",
                        parent.display()
                    ))
                })?;
            }
            std::fs::write(&full_path, content).map_err(|e| {
                ProtocolError::PlanWrite(format!(
                    "Failed to write note file '{}': {e}",
                    full_path.display()
                ))
            })?;
        }

        self.written_notes.push(relative_path.clone());
        Ok(relative_path.to_string_lossy().to_string())
    }

    fn write_plan(&mut self, plan: &ReleaseSyncPlan) -> Result<(), ProtocolError> {
        let full_path = self.workspace_root.join(&self.config.plan_out);
        if !self.config.dry_run {
            if let Some(parent) = full_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    ProtocolError::PlanWrite(format!(
                        "Failed to create plan dir '{}': {e}",
                        parent.display()
                    ))
                })?;
            }
            let serialized = serde_json::to_string_pretty(plan).map_err(|e| {
                ProtocolError::PlanWrite(format!("Failed to serialize plan JSON: {e}"))
            })?;
            std::fs::write(&full_path, serialized).map_err(|e| {
                ProtocolError::PlanWrite(format!(
                    "Failed to write plan file '{}': {e}",
                    full_path.display()
                ))
            })?;
        }
        Ok(())
    }
}
