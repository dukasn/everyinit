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

#![allow(clippy::result_large_err)]
use super::archiver::ArchiveEntry;
use super::config::{LayoutMode, TarGzConfig};
use evin_action_sdk::RunnerContext;
use glob::Pattern;
use std::collections::BTreeMap;
use std::path::PathBuf;

pub(super) fn resolve_and_map_files(
    ctx: &RunnerContext,
    config: &TarGzConfig,
) -> Result<Vec<ArchiveEntry>, evin_diagnostic::Diagnostic> {
    let mut mapped_files: BTreeMap<PathBuf, ArchiveEntry> = BTreeMap::new();
    let prefix = config.prefix.as_deref().unwrap_or("");

    let mut exclude_patterns = Vec::new();
    for ex in &config.exclude {
        exclude_patterns.push(Pattern::new(ex).map_err(|e| {
            evin_locales::diagnostic(
                evin_diagnostic::IssueCode::new("invalid-glob").arg("error", e.to_string()),
            )
        })?);
    }

    for pattern in &config.include {
        for path in ctx.fs.matching_files(pattern).map_err(|e| {
            evin_locales::diagnostic(
                evin_diagnostic::IssueCode::new("issue-archive-safe-path-failed")
                    .arg("context", "source")
                    .arg("error", e.to_string()),
            )
        })? {
            let rel_path = match path.strip_prefix(ctx.fs.task_cwd()) {
                Ok(p) => p,
                Err(_) => {
                    if config.layout == LayoutMode::Relative {
                        return Err(evin_locales::diagnostic(
                            evin_diagnostic::IssueCode::new("relative-path-escape")
                                .arg("path", path.display().to_string()),
                        ));
                    }
                    path.strip_prefix(ctx.fs.workspace_root()).unwrap_or(&path)
                }
            };

            if exclude_patterns.iter().any(|p| p.matches_path(rel_path)) {
                continue;
            }

            // Apply Flatten & Prefix logic
            let dest_path = if config.layout == LayoutMode::Flat {
                let file_name = path.file_name().ok_or_else(|| {
                    evin_locales::diagnostic(
                        evin_diagnostic::IssueCode::new("invalid-file")
                            .arg("path", path.display().to_string()),
                    )
                })?;
                PathBuf::from(prefix).join(file_name)
            } else {
                PathBuf::from(prefix).join(rel_path)
            };

            let safe_source = ctx
                .fs
                .get_safe_physical_path(&path.to_string_lossy())
                .map_err(|e| {
                    evin_locales::diagnostic(
                        evin_diagnostic::IssueCode::new("issue-archive-safe-path-failed")
                            .arg("context", "source")
                            .arg("error", e.to_string()),
                    )
                })?;

            if let Some(existing) = mapped_files.get(&dest_path) {
                if existing.source != safe_source {
                    return Err(evin_locales::diagnostic(
                        evin_diagnostic::IssueCode::new("path-collision")
                            .arg("destination", dest_path.display().to_string())
                            .arg("source1", existing.source.display().to_string())
                            .arg("source2", safe_source.display().to_string()),
                    ));
                }
            } else {
                mapped_files.insert(
                    dest_path.clone(),
                    ArchiveEntry {
                        source: safe_source,
                        destination: dest_path,
                    },
                );
            }
        }
    }

    Ok(mapped_files.into_values().collect())
}
