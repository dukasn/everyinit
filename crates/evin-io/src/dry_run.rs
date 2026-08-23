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

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Debug, Default)]
struct DryRunData {
    files: HashMap<PathBuf, Vec<u8>>,
    staged_paths: HashSet<PathBuf>,
    stage_all: bool,
    committed_paths: HashSet<PathBuf>,
    committed_physical_paths: HashSet<PathBuf>,
    all_physical_changes_committed: bool,
    uncertainty: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Default)]
pub struct DryRunState {
    inner: Arc<Mutex<DryRunData>>,
}

impl DryRunState {
    pub fn reset(&self) {
        if let Ok(mut state) = self.inner.lock() {
            *state = DryRunData::default();
        }
    }

    pub fn write(&self, path: PathBuf, content: Vec<u8>) {
        if let Ok(mut state) = self.inner.lock() {
            state.committed_paths.remove(&path);
            state.files.insert(path, content);
        }
    }

    pub fn read(&self, path: &Path) -> Option<Vec<u8>> {
        self.inner
            .lock()
            .ok()
            .and_then(|state| state.files.get(path).cloned())
    }

    pub fn contains(&self, path: &Path) -> bool {
        self.inner
            .lock()
            .is_ok_and(|state| state.files.contains_key(path))
    }

    pub fn virtual_paths(&self) -> Vec<PathBuf> {
        self.inner
            .lock()
            .map(|state| state.files.keys().cloned().collect())
            .unwrap_or_default()
    }

    pub fn stage_all(&self) {
        if let Ok(mut state) = self.inner.lock() {
            state.stage_all = true;
        }
    }

    pub fn stage_paths(&self, paths: impl IntoIterator<Item = PathBuf>) {
        if let Ok(mut state) = self.inner.lock() {
            state.staged_paths.extend(paths);
        }
    }

    pub fn has_virtual_staged_changes(&self) -> bool {
        self.inner.lock().is_ok_and(|state| {
            (state.stage_all
                && state
                    .files
                    .keys()
                    .any(|path| !state.committed_paths.contains(path)))
                || state.staged_paths.iter().any(|path| {
                    state.files.contains_key(path) && !state.committed_paths.contains(path)
                })
        })
    }

    pub fn staging(&self) -> (bool, Vec<PathBuf>) {
        self.inner
            .lock()
            .map(|state| {
                (
                    state.stage_all,
                    state.staged_paths.iter().cloned().collect(),
                )
            })
            .unwrap_or_default()
    }

    pub fn physical_changes_consumed(&self) -> bool {
        self.inner
            .lock()
            .is_ok_and(|state| state.all_physical_changes_committed)
    }

    pub fn physical_path_consumed(&self, path: &Path) -> bool {
        self.inner.lock().is_ok_and(|state| {
            state.all_physical_changes_committed || state.committed_physical_paths.contains(path)
        })
    }

    pub fn commit_staging(&self) {
        if let Ok(mut state) = self.inner.lock() {
            if state.stage_all {
                state.committed_paths = state.files.keys().cloned().collect();
                state.all_physical_changes_committed = true;
            } else {
                let staged = state.staged_paths.clone();
                state.committed_paths.extend(staged.clone());
                state.committed_physical_paths.extend(staged);
            }
            state.stage_all = false;
            state.staged_paths.clear();
        }
    }

    pub fn mark_unverified(&self, task: &str, reason: impl Into<String>) {
        if let Ok(mut state) = self.inner.lock() {
            let reasons = state.uncertainty.entry(task.to_string()).or_default();
            let reason = reason.into();
            if !reasons.contains(&reason) {
                reasons.push(reason);
            }
        }
    }

    pub fn uncertainty_for(&self, task: &str) -> Vec<String> {
        self.inner
            .lock()
            .ok()
            .and_then(|state| state.uncertainty.get(task).cloned())
            .unwrap_or_default()
    }
}
