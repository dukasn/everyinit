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

use evin_config::parser::RepositoryPolicySource;
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

const DEFAULT_IGNORES: [&str; 5] = [
    ".git/**",
    "target/**",
    "dist/**",
    ".evin/trace.json",
    "**/.evin/trace.json",
];

#[derive(Clone)]
pub struct WatchSpec {
    pub(crate) workspace_root: PathBuf,
    pub(crate) display_name: String,
    pub(crate) config_paths: BTreeSet<String>,
    pub(crate) debounce: Duration,
    pub(crate) include_set: GlobSet,
    pub(crate) ignore_set: GlobSet,
    pub(crate) default_ignore_set: GlobSet,
    pub(crate) default_reinclude_set: GlobSet,
    pub(crate) explicitly_included_defaults: HashSet<&'static str>,
    pub(crate) scan_roots: BTreeSet<String>,
}

impl WatchSpec {
    pub(crate) fn build(
        workspace_root: PathBuf,
        display_name: String,
        paths: BTreeSet<String>,
        ignores: BTreeSet<String>,
        config_paths: BTreeSet<String>,
        debounce: Duration,
    ) -> Result<Self, evin_diagnostic::Diagnostic> {
        let paths: Vec<_> = paths.into_iter().collect();
        let ignores: Vec<_> = ignores.into_iter().collect();
        let include_set = compile_globs(&paths)?;
        let ignore_set = compile_globs(&ignores)?;
        let default_ignore_set = compile_globs(&DEFAULT_IGNORES.map(str::to_string))?;
        let default_reinclude_patterns: Vec<_> = paths
            .iter()
            .filter(|path| {
                DEFAULT_IGNORES.into_iter().any(|default| {
                    let root = default.trim_end_matches("/**");
                    *path == root || path.starts_with(&format!("{root}/"))
                })
            })
            .cloned()
            .collect();
        let default_reinclude_set = compile_globs(&default_reinclude_patterns)?;
        let explicitly_included_defaults = DEFAULT_IGNORES
            .into_iter()
            .filter(|default| {
                let root = default.trim_end_matches("/**");
                paths.iter().any(|path| {
                    path == root
                        || path.starts_with(&format!("{root}/"))
                        || (root == ".evin/trace.json" && path == root)
                        || (root == "**/.evin/trace.json"
                            && (path == ".evin/trace.json" || path.ends_with("/.evin/trace.json")))
                })
            })
            .collect();
        let mut scan_roots: BTreeSet<_> = paths
            .iter()
            .map(|pattern| glob_scan_root(pattern))
            .collect();
        // A broad root subsumes every nested root and avoids duplicate walks.
        let roots = scan_roots.clone();
        scan_roots.retain(|candidate| {
            !roots.iter().any(|other| {
                other != candidate
                    && (other.is_empty() || candidate.starts_with(&format!("{other}/")))
            })
        });

        Ok(Self {
            workspace_root,
            display_name,
            config_paths,
            debounce,
            include_set,
            ignore_set,
            default_ignore_set,
            default_reinclude_set,
            explicitly_included_defaults,
            scan_roots,
        })
    }

    pub(crate) fn matches(&self, relative: &str) -> bool {
        if self.config_paths.contains(relative) {
            return true;
        }
        if !self.include_set.is_match(relative) || self.ignore_set.is_match(relative) {
            return false;
        }
        if self.default_ignore_set.is_match(relative) {
            return self.default_reinclude_set.is_match(relative);
        }
        true
    }

    pub(crate) fn may_descend_default_ignored(&self, relative: &str) -> bool {
        self.explicitly_included_defaults.iter().any(|default| {
            let root = default.trim_end_matches("/**");
            relative == root || root.starts_with(&format!("{relative}/"))
        })
    }

    #[cfg(test)]
    pub(crate) fn debounce(&self) -> Duration {
        self.debounce
    }
}

fn glob_scan_root(pattern: &str) -> String {
    let mut root = Vec::new();
    for component in pattern.split('/') {
        if component
            .chars()
            .any(|character| matches!(character, '*' | '?' | '[' | '{'))
        {
            break;
        }
        root.push(component);
    }
    root.join("/")
}

pub(crate) fn compile_globs(patterns: &[String]) -> Result<GlobSet, evin_diagnostic::Diagnostic> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = Glob::new(pattern).map_err(|error| {
            evin_diagnostic::Diagnostic::new(
                "Invalid watch pattern",
                evin_diagnostic::IssueCode::custom(error.to_string()),
            )
        })?;
        builder.add(glob);
    }
    builder.build().map_err(|error| {
        evin_diagnostic::Diagnostic::new(
            "Invalid watch configuration",
            evin_diagnostic::IssueCode::custom(error.to_string()),
        )
    })
}

/// Resolves and merges the watch declarations for the concrete tasks selected
/// by `target`. Dependencies contribute configuration inputs, but only selected
/// tasks opt the invocation into watch mode.
pub async fn resolve_watch_spec(
    target: &str,
) -> Result<Option<WatchSpec>, evin_diagnostic::Diagnostic> {
    let workspace_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let config = evin_config::load_workspace_tasks().await?;
    let registry = crate::resolver::TargetRegistry::build(config.tasks.clone());
    let (_, selected) = registry.resolve(target)?;

    let by_name: HashMap<_, _> = config
        .tasks
        .iter()
        .map(|task| (task.name.as_str(), task))
        .collect();
    let display_name = if selected.len() == 1 {
        by_name
            .get(selected[0].as_str())
            .and_then(|task| task.description.clone())
            .unwrap_or_else(|| selected[0].clone())
    } else {
        target.to_string()
    };
    let Some((paths, ignores, debounce)) = merge_watch_configs(&selected, &config.tasks)? else {
        return Ok(None);
    };

    let mut config_paths = BTreeSet::from([evin_config::paths::WORKSPACE_MANIFEST.to_string()]);
    // Creation and deletion matter too, so absent optional configuration
    // inputs remain in the snapshot as watched exact paths.
    config_paths.insert(evin_config::paths::ENVIRONMENT_MANIFEST.to_string());
    if let RepositoryPolicySource::Local(path) = &config.policy_source {
        config_paths.insert(normalize_relative(path));
    }

    // Configuration sources for the selected DAG include each transitive
    // dependency's module file, even when that dependency has no watch block.
    let mut pending = selected;
    let mut visited = HashSet::new();
    while let Some(name) = pending.pop() {
        if !visited.insert(name.clone()) {
            continue;
        }
        if let Some(task) = by_name.get(name.as_str()) {
            if let Some(source) = &task.source_file {
                config_paths.insert(normalize_relative(source));
            }
            pending.extend(task.needs.clone().unwrap_or_default());
        }
    }

    WatchSpec::build(
        workspace_root,
        display_name,
        paths,
        ignores,
        config_paths,
        debounce,
    )
    .map(Some)
}

type MergedWatch = (BTreeSet<String>, BTreeSet<String>, Duration);

pub(crate) fn merge_watch_configs(
    selected: &[String],
    tasks: &[evin_config::models::TaskObject],
) -> Result<Option<MergedWatch>, evin_diagnostic::Diagnostic> {
    let mut paths = BTreeSet::new();
    let mut ignores = BTreeSet::new();
    let mut debounce = Duration::from_millis(300);
    let mut enabled = false;
    for name in selected {
        if let Some(watch) = tasks
            .iter()
            .find(|task| task.name == *name)
            .and_then(|task| task.watch.as_ref())
        {
            enabled = true;
            paths.extend(watch.paths.iter().cloned());
            ignores.extend(watch.ignore.iter().cloned());
            debounce = debounce.max(watch.debounce_duration().map_err(|message| {
                evin_diagnostic::Diagnostic::new(
                    "Invalid watch configuration",
                    evin_diagnostic::IssueCode::custom(message),
                )
            })?);
        }
    }
    Ok(enabled.then_some((paths, ignores, debounce)))
}

pub(crate) fn normalize_relative(path: impl AsRef<Path>) -> String {
    path.as_ref()
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(value) => Some(value.to_string_lossy()),
            std::path::Component::CurDir => None,
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}
