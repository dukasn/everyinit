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

use super::spec::{WatchSpec, normalize_relative};
use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::time::SystemTime;

const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FileStamp {
    pub(crate) modified: Option<SystemTime>,
    pub(crate) len: u64,
}

pub(crate) struct PollingWatcher {
    pub(crate) spec: WatchSpec,
    snapshot: HashMap<String, FileStamp>,
}

impl PollingWatcher {
    pub(crate) fn new(spec: WatchSpec) -> Self {
        let snapshot = snapshot(&spec);
        Self { spec, snapshot }
    }

    pub fn replace_spec(&mut self, spec: WatchSpec) {
        self.snapshot = snapshot(&spec);
        self.spec = spec;
    }

    pub(crate) fn apply_resolved_spec(
        &mut self,
        resolved: Result<Option<WatchSpec>, evin_diagnostic::Diagnostic>,
    ) -> bool {
        if let Ok(Some(spec)) = resolved {
            self.replace_spec(spec);
            true
        } else {
            false
        }
    }

    pub async fn next_change(&mut self) -> WatchChange {
        loop {
            tokio::time::sleep(POLL_INTERVAL).await;
            let current = snapshot(&self.spec);
            let changed = diff_snapshots(&self.snapshot, &current);
            self.snapshot = current;
            if changed.is_empty() {
                continue;
            }

            let mut all_changed = changed;
            loop {
                tokio::time::sleep(self.spec.debounce).await;
                let current = snapshot(&self.spec);
                let more = diff_snapshots(&self.snapshot, &current);
                self.snapshot = current;
                if more.is_empty() {
                    break;
                }
                all_changed.extend(more);
            }
            return WatchChange { paths: all_changed };
        }
    }
}

pub(crate) struct WatchChange {
    pub(crate) paths: BTreeSet<String>,
}

impl WatchChange {
    pub fn summary(&self) -> String {
        if self.paths.len() == 1 {
            return self.paths.iter().next().cloned().unwrap_or_default();
        }
        let mut paths = self.paths.iter();
        let mut common: Vec<_> = paths
            .next()
            .map(|path| path.split('/').collect())
            .unwrap_or_default();
        common.pop();
        for path in paths {
            let mut components: Vec<_> = path.split('/').collect();
            components.pop();
            let shared = common
                .iter()
                .zip(&components)
                .take_while(|(left, right)| left == right)
                .count();
            common.truncate(shared);
        }
        let location = if common.is_empty() {
            "workspace".to_string()
        } else {
            common.join("/")
        };
        format!("{} files in {location}", self.paths.len())
    }
}

pub(crate) fn snapshot(spec: &WatchSpec) -> HashMap<String, FileStamp> {
    let mut result = HashMap::new();
    for root in &spec.scan_roots {
        let path = spec.workspace_root.join(root);
        if path.is_dir() {
            visit_directory(spec, &path, &mut result);
        } else {
            record_file(spec, &path, &mut result);
        }
    }
    for config_path in &spec.config_paths {
        record_file(spec, &spec.workspace_root.join(config_path), &mut result);
    }
    result
}

fn record_file(spec: &WatchSpec, path: &Path, result: &mut HashMap<String, FileStamp>) {
    let Ok(relative_path) = path.strip_prefix(&spec.workspace_root) else {
        return;
    };
    let relative = normalize_relative(relative_path);
    if !spec.matches(&relative) {
        return;
    }
    if let Ok(metadata) = path.metadata()
        && metadata.is_file()
    {
        result.insert(
            relative,
            FileStamp {
                modified: metadata.modified().ok(),
                len: metadata.len(),
            },
        );
    }
}

fn visit_directory(spec: &WatchSpec, directory: &Path, result: &mut HashMap<String, FileStamp>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(relative_path) = path.strip_prefix(&spec.workspace_root) else {
            continue;
        };
        let relative = normalize_relative(relative_path);
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            let default_ignored = matches!(relative.as_str(), ".git" | "target" | "dist");
            if !default_ignored || spec.may_descend_default_ignored(&relative) {
                visit_directory(spec, &path, result);
            }
        } else if file_type.is_file() {
            record_file(spec, &path, result);
        }
    }
}

pub(crate) fn diff_snapshots(
    previous: &HashMap<String, FileStamp>,
    current: &HashMap<String, FileStamp>,
) -> BTreeSet<String> {
    previous
        .keys()
        .chain(current.keys())
        .filter(|path| previous.get(*path) != current.get(*path))
        .cloned()
        .collect()
}
