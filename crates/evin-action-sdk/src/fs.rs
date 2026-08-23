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

pub use evin_io::DryRunState;
use evin_io::{IoContext, IoMode};
use std::io;
use std::path::{Path, PathBuf}; // Re-export for compatibility

#[derive(Clone)]
pub struct WorkspaceFs {
    ctx: IoContext,
    log: crate::Logger,
}

impl WorkspaceFs {
    pub fn new(
        workspace_root: PathBuf,
        task_cwd: PathBuf,
        mode: crate::PipelineMode,
        log: crate::Logger,
    ) -> Self {
        Self::new_with_dry_run(workspace_root, task_cwd, mode, log, DryRunState::default())
    }

    pub fn new_with_dry_run(
        workspace_root: PathBuf,
        task_cwd: PathBuf,
        mode: crate::PipelineMode,
        log: crate::Logger,
        dry_run: DryRunState,
    ) -> Self {
        let io_mode = match mode {
            crate::PipelineMode::DryRun => IoMode::DryRun,
            _ => IoMode::Normal,
        };
        Self {
            ctx: IoContext::with_dry_run(workspace_root, task_cwd, io_mode, dry_run),
            log,
        }
    }

    pub fn workspace_root(&self) -> &Path {
        self.ctx.workspace_root()
    }

    pub fn task_cwd(&self) -> &Path {
        self.ctx.task_cwd()
    }

    pub fn get_safe_physical_path(&self, requested: &str) -> io::Result<PathBuf> {
        self.ctx
            .resolve(requested)
            .map_err(|e| io::Error::new(io::ErrorKind::PermissionDenied, e.to_string()))
    }

    pub fn read_to_string(&self, path: &str) -> io::Result<String> {
        self.ctx
            .read_source(path)
            .map(|s| s.text.to_string())
            .map_err(|e| io::Error::other(e.to_string()))
    }

    pub fn write(&self, path: &str, content: impl AsRef<[u8]>) -> io::Result<()> {
        if self.ctx.mode() == IoMode::DryRun {
            let safe_path = self.get_safe_physical_path(path)?;
            self.log.plan(crate::activity::WriteFilePlan {
                path: safe_path.display().to_string(),
            });
        }
        self.ctx
            .write(path, content)
            .map_err(|e| io::Error::other(e.to_string()))
    }

    pub fn exists(&self, path: &str) -> bool {
        self.ctx.exists(path)
    }

    pub fn matching_files(&self, pattern: &str) -> io::Result<Vec<PathBuf>> {
        self.ctx
            .matching_files(pattern)
            .map_err(|e| io::Error::other(e.to_string()))
    }
}

// Tests omitted or migrated
