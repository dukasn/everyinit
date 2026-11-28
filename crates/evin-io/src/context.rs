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

use crate::cache::SourceCache;
use crate::dry_run::DryRunState;
use crate::error::IoError;
use crate::path::clean_path;
use crate::source::SourceFile;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IoMode {
    #[default]
    Normal,
    DryRun,
}

#[derive(Clone)]
pub struct IoContext {
    workspace_root: PathBuf,
    task_cwd: PathBuf,
    mode: IoMode,
    dry_run: DryRunState,
    cache: SourceCache,
}

impl IoContext {
    pub fn new(workspace_root: PathBuf, task_cwd: PathBuf) -> Self {
        Self::with_dry_run(
            workspace_root,
            task_cwd,
            IoMode::Normal,
            DryRunState::default(),
        )
    }

    pub fn with_dry_run(
        workspace_root: PathBuf,
        task_cwd: PathBuf,
        mode: IoMode,
        dry_run: DryRunState,
    ) -> Self {
        Self {
            workspace_root: clean_path(&workspace_root),
            task_cwd: clean_path(&task_cwd),
            mode,
            dry_run,
            cache: SourceCache::default(),
        }
    }

    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    pub fn task_cwd(&self) -> &Path {
        &self.task_cwd
    }

    pub fn mode(&self) -> IoMode {
        self.mode
    }

    pub fn dry_run_state(&self) -> &DryRunState {
        &self.dry_run
    }

    pub fn source_cache(&self) -> &SourceCache {
        &self.cache
    }

    pub fn resolve(&self, requested: &str) -> Result<PathBuf, IoError> {
        let req_path = Path::new(requested);
        let joined = if req_path.is_absolute() {
            req_path.to_path_buf()
        } else {
            self.task_cwd.join(req_path)
        };

        let cleaned = clean_path(&joined);

        if !cleaned.starts_with(&self.workspace_root) {
            return Err(IoError::PathTraversal {
                path: cleaned.display().to_string(),
                root: self.workspace_root.display().to_string(),
            });
        }

        Ok(cleaned)
    }

    pub fn read_source(&self, path: &str) -> Result<Arc<SourceFile>, IoError> {
        let safe_path = self.resolve(path)?;
        let safe_path_str = safe_path.to_string_lossy().to_string();

        if let Some(cached) = self.cache.get(&safe_path_str) {
            return Ok(cached);
        }

        let content = if self.mode == IoMode::DryRun {
            if let Some(content_bytes) = self.dry_run.read(&safe_path) {
                String::from_utf8(content_bytes).map_err(|_| IoError::Encoding {
                    path: safe_path_str.clone(),
                })?
            } else {
                fs::read_to_string(&safe_path)?
            }
        } else {
            fs::read_to_string(&safe_path)?
        };

        Ok(self.cache.insert(safe_path_str, content))
    }

    pub fn read_bytes(&self, path: &str) -> Result<Vec<u8>, IoError> {
        let safe_path = self.resolve(path)?;
        if self.mode == IoMode::DryRun
            && let Some(content) = self.dry_run.read(&safe_path)
        {
            return Ok(content);
        }
        Ok(fs::read(&safe_path)?)
    }

    pub fn write(&self, path: &str, content: impl AsRef<[u8]>) -> Result<(), IoError> {
        let safe_path = self.resolve(path)?;
        let safe_path_str = safe_path.to_string_lossy().to_string();

        self.cache.evict(&safe_path_str);

        if self.mode == IoMode::DryRun {
            self.dry_run.write(safe_path, content.as_ref().to_vec());
            return Ok(());
        }

        if let Some(parent) = safe_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(safe_path, content)?;
        Ok(())
    }

    pub fn write_text(&self, path: &str, content: &str) -> Result<(), IoError> {
        self.write(path, content.as_bytes())
    }

    pub fn exists(&self, path: &str) -> bool {
        self.resolve(path)
            .map(|p| (self.mode == IoMode::DryRun && self.dry_run.contains(&p)) || p.exists())
            .unwrap_or(false)
    }

    pub fn matching_files(&self, pattern: &str) -> Result<Vec<PathBuf>, IoError> {
        let safe_pattern = self.resolve(pattern)?;
        let pattern_text = glob_pattern_text(&safe_pattern);
        let mut matches = std::collections::BTreeSet::new();

        for path in glob::glob(&pattern_text)
            .map_err(|e| IoError::InvalidPattern(e.to_string()))?
            .flatten()
        {
            if path.is_file() {
                matches.insert(path);
            }
        }

        if self.mode == IoMode::DryRun
            && let Ok(pattern) = glob::Pattern::new(&pattern_text)
        {
            for path in self.dry_run.virtual_paths() {
                if pattern.matches_path(&path) {
                    matches.insert(path);
                }
            }
        }

        Ok(matches.into_iter().collect())
    }

    /// Match a manifest-owned pattern without changing the task's cwd. The
    /// caller supplies a workspace-contained directory and patterns may not
    /// escape it.
    pub fn matching_files_from(&self, base: &Path, pattern: &str) -> Result<Vec<PathBuf>, IoError> {
        if !base.starts_with(&self.workspace_root) {
            return Err(IoError::PathTraversal {
                path: base.display().to_string(),
                root: self.workspace_root.display().to_string(),
            });
        }
        let pattern_path = Path::new(pattern);
        if pattern_path.is_absolute()
            || pattern_path
                .components()
                .any(|part| part == std::path::Component::ParentDir)
        {
            return Err(IoError::PathTraversal {
                path: pattern.to_string(),
                root: base.display().to_string(),
            });
        }
        let joined = base.join(pattern);
        let pattern_text = glob_pattern_text(&joined);
        let mut matches = std::collections::BTreeSet::new();
        for path in glob::glob(&pattern_text)
            .map_err(|e| IoError::InvalidPattern(e.to_string()))?
            .flatten()
        {
            if path.is_file() && path.starts_with(base) && path.starts_with(&self.workspace_root) {
                matches.insert(path);
            }
        }
        Ok(matches.into_iter().collect())
    }

    pub fn create_dir_all(&self, path: &str) -> Result<(), IoError> {
        let safe_path = self.resolve(path)?;
        if self.mode == IoMode::DryRun {
            return Ok(());
        }
        fs::create_dir_all(safe_path)?;
        Ok(())
    }
}

fn glob_pattern_text(path: &Path) -> String {
    let text = path.to_string_lossy();
    #[cfg(windows)]
    {
        text.replace('\\', "/")
    }
    #[cfg(not(windows))]
    {
        text.into_owned()
    }
}
