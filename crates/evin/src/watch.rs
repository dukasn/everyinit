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

use crate::engine::{CliContext, PipelineCompletion, PipelineEngine};
use crate::events::PipelineEvent;
use chrono::Local;
use evin_config::parser::GitRulesSource;
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

const POLL_INTERVAL: Duration = Duration::from_millis(100);
const SHUTDOWN_GRACE: Duration = Duration::from_secs(2);
const DEFAULT_IGNORES: [&str; 4] = [".git/**", "target/**", "dist/**", ".evin/trace.json"];

#[derive(Clone)]
pub struct WatchSpec {
    workspace_root: PathBuf,
    display_name: String,
    config_paths: BTreeSet<String>,
    debounce: Duration,
    include_set: GlobSet,
    ignore_set: GlobSet,
    default_ignore_set: GlobSet,
    default_reinclude_set: GlobSet,
    explicitly_included_defaults: HashSet<&'static str>,
    scan_roots: BTreeSet<String>,
}

impl WatchSpec {
    fn build(
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

    fn matches(&self, relative: &str) -> bool {
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

    fn may_descend_default_ignored(&self, relative: &str) -> bool {
        self.explicitly_included_defaults.iter().any(|default| {
            let root = default.trim_end_matches("/**");
            relative == root || root.starts_with(&format!("{relative}/"))
        })
    }

    #[cfg(test)]
    fn debounce(&self) -> Duration {
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

fn compile_globs(patterns: &[String]) -> Result<GlobSet, evin_diagnostic::Diagnostic> {
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

    let mut config_paths = BTreeSet::from([".evin/workspace.yaml".to_string()]);
    // Creation and deletion matter too, so absent optional configuration
    // inputs remain in the snapshot as watched exact paths.
    config_paths.insert(".evin/environment.yaml".to_string());
    if let GitRulesSource::Local(path) = &config.gitrules_source {
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

fn merge_watch_configs(
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

fn normalize_relative(path: impl AsRef<Path>) -> String {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileStamp {
    modified: Option<SystemTime>,
    len: u64,
}

pub struct PollingWatcher {
    spec: WatchSpec,
    snapshot: HashMap<String, FileStamp>,
}

impl PollingWatcher {
    pub fn new(spec: WatchSpec) -> Self {
        let snapshot = snapshot(&spec);
        Self { spec, snapshot }
    }

    pub fn replace_spec(&mut self, spec: WatchSpec) {
        self.snapshot = snapshot(&spec);
        self.spec = spec;
    }

    fn apply_resolved_spec(
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

pub struct WatchChange {
    paths: BTreeSet<String>,
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

fn snapshot(spec: &WatchSpec) -> HashMap<String, FileStamp> {
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

fn diff_snapshots(
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

#[derive(Clone)]
pub struct RunSession {
    pub target: String,
    pub args: Vec<String>,
    pub mode: evin_action_sdk::PipelineMode,
    pub verbose: bool,
    pub forced_execution: Option<evin_config::workspace::ExecutionMode>,
}

impl RunSession {
    fn context(
        &self,
        cancel_token: CancellationToken,
        watch_session: bool,
        summarize_cancelled_session: Arc<AtomicBool>,
    ) -> CliContext {
        CliContext {
            mode: self.mode,
            targets: vec![self.target.clone()],
            run_payload: self.args.clone(),
            forced_execution: self.forced_execution,
            verbose: self.verbose,
            cancel_token,
            watch_session,
            summarize_cancelled_session,
            ..Default::default()
        }
    }
}

/// Runs complete, freshly prepared pipeline sessions until a termination
/// signal. The reporter is intentionally owned by the caller and therefore
/// survives every session boundary.
pub async fn supervise(
    sender: mpsc::UnboundedSender<PipelineEvent>,
    request: RunSession,
    initial_spec: WatchSpec,
) -> bool {
    let mut watcher = PollingWatcher::new(initial_spec);
    let mut signals = SignalCoordinator::install();

    loop {
        let cancel_token = CancellationToken::new();
        let summarize_cancelled_session = Arc::new(AtomicBool::new(false));
        let context = request.context(
            cancel_token.clone(),
            true,
            summarize_cancelled_session.clone(),
        );
        let engine = PipelineEngine::new(sender.clone());
        let mut session =
            tokio::spawn(engine.execute_pipeline(crate::commands::run_phases(), context));

        let change = tokio::select! {
            result = &mut session => {
                let _completion = result.unwrap_or(PipelineCompletion::Failed);
                tokio::select! {
                    change = watcher.next_change() => change,
                    _ = signals.next() => {
                        let _ = sender.send(PipelineEvent::SupervisorTerminated { forced: false });
                        return false;
                    }
                }
            }
            change = watcher.next_change() => change,
            _ = signals.next() => {
                summarize_cancelled_session.store(true, Ordering::Relaxed);
                cancel_token.cancel();
                let cleanup = finish_session(&mut session, &mut signals).await;
                let _ = sender.send(PipelineEvent::SupervisorTerminated {
                    forced: cleanup != CleanupOutcome::Graceful,
                });
                return false;
            }
        };

        let reload_started = std::time::Instant::now();
        let timestamp = Local::now().format("%H:%M:%S");
        let _ = sender.send(PipelineEvent::WatchEvent {
            line: format!("{timestamp}  change   {}", change.summary()),
        });
        let _ = sender.send(PipelineEvent::WatchEvent {
            line: format!("{timestamp}  restart  source change detected"),
        });

        if !session.is_finished() {
            cancel_token.cancel();
            if let RestartCleanup::Terminate { forced } =
                finish_restart_session(&mut session, &summarize_cancelled_session, &mut signals)
                    .await
            {
                let _ = sender.send(PipelineEvent::SupervisorTerminated { forced });
                return false;
            }
        }

        // Re-resolve from disk. Invalid configuration is rendered by the next
        // session; the last valid matcher remains installed so fixing that file
        // can always trigger recovery.
        watcher.apply_resolved_spec(resolve_watch_spec(&request.target).await);
        let _ = sender.send(PipelineEvent::Restarting {
            display_name: watcher.spec.display_name.clone(),
            duration: reload_started.elapsed(),
        });
    }
}

pub async fn run_once_with_signals(
    sender: mpsc::UnboundedSender<PipelineEvent>,
    request: RunSession,
) -> PipelineCompletion {
    let mut signals = SignalCoordinator::install();
    let cancel_token = CancellationToken::new();
    let context = request.context(
        cancel_token.clone(),
        false,
        Arc::new(AtomicBool::new(false)),
    );
    let engine = PipelineEngine::new(sender.clone());
    let mut session = tokio::spawn(engine.execute_pipeline(crate::commands::run_phases(), context));
    tokio::select! {
        result = &mut session => result.unwrap_or(PipelineCompletion::Failed),
        _ = signals.next() => {
            cancel_token.cancel();
            finish_session(&mut session, &mut signals).await;
            // If forced cleanup aborted the engine before it could publish its
            // terminal event, this guarantees the one-shot reporter can exit.
            let _ = sender.send(PipelineEvent::PipelineCancelled);
            PipelineCompletion::Cancelled
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CleanupOutcome {
    Graceful,
    TimedOut,
    Forced,
}

async fn finish_session(
    session: &mut tokio::task::JoinHandle<PipelineCompletion>,
    signals: &mut SignalCoordinator,
) -> CleanupOutcome {
    finish_session_until(
        session,
        signals,
        tokio::time::Instant::now() + SHUTDOWN_GRACE,
    )
    .await
}

async fn finish_session_until(
    session: &mut tokio::task::JoinHandle<PipelineCompletion>,
    signals: &mut SignalCoordinator,
    deadline: tokio::time::Instant,
) -> CleanupOutcome {
    tokio::select! {
        _ = &mut *session => CleanupOutcome::Graceful,
        _ = tokio::time::sleep_until(deadline) => {
            session.abort();
            let _ = (&mut *session).await;
            CleanupOutcome::TimedOut
        }
        _ = signals.next() => {
            session.abort();
            let _ = (&mut *session).await;
            CleanupOutcome::Forced
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RestartCleanup {
    Continue,
    Terminate { forced: bool },
}

async fn finish_restart_session(
    session: &mut tokio::task::JoinHandle<PipelineCompletion>,
    summarize_cancelled_session: &AtomicBool,
    signals: &mut SignalCoordinator,
) -> RestartCleanup {
    let deadline = tokio::time::Instant::now() + SHUTDOWN_GRACE;
    tokio::select! {
        _ = &mut *session => RestartCleanup::Continue,
        _ = tokio::time::sleep_until(deadline) => {
            session.abort();
            let _ = (&mut *session).await;
            RestartCleanup::Continue
        }
        _ = signals.next() => {
            // This is the first signal: disable the pending restart, then use
            // the remaining grace window so a second signal can force cleanup.
            summarize_cancelled_session.store(true, Ordering::Relaxed);
            let outcome = finish_session_until(session, signals, deadline).await;
            RestartCleanup::Terminate { forced: outcome != CleanupOutcome::Graceful }
        }
    }
}

struct SignalCoordinator {
    #[cfg(unix)]
    pending: Arc<AtomicBool>,
    #[cfg(unix)]
    registrations: Vec<signal_hook::SigId>,
    #[cfg(not(unix))]
    task: tokio::task::JoinHandle<()>,
    #[cfg(not(unix))]
    receiver: mpsc::UnboundedReceiver<()>,
}

impl SignalCoordinator {
    fn install() -> Self {
        #[cfg(unix)]
        {
            use signal_hook::consts::signal::{SIGINT, SIGTERM};
            let pending = Arc::new(AtomicBool::new(false));
            let registrations = [SIGINT, SIGTERM]
                .into_iter()
                .map(|signal| {
                    signal_hook::flag::register(signal, pending.clone())
                        .expect("termination signal handler")
                })
                .collect();

            Self {
                pending,
                registrations,
            }
        }

        #[cfg(not(unix))]
        {
            let (sender, receiver) = mpsc::unbounded_channel();
            let signal_sender = sender.clone();
            let task = tokio::spawn(async move {
                while tokio::signal::ctrl_c().await.is_ok() {
                    if signal_sender.send(()).is_err() {
                        break;
                    }
                }
            });
            Self { task, receiver }
        }
    }

    async fn next(&mut self) {
        #[cfg(unix)]
        loop {
            if self.pending.swap(false, Ordering::SeqCst) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        #[cfg(not(unix))]
        let _ = self.receiver.recv().await;
    }

    #[cfg(test)]
    fn inject(&self) {
        #[cfg(unix)]
        self.pending.store(true, Ordering::SeqCst);
    }
}

#[cfg(unix)]
impl Drop for SignalCoordinator {
    fn drop(&mut self) {
        for registration in self.registrations.drain(..) {
            signal_hook::low_level::unregister(registration);
        }
    }
}

#[cfg(not(unix))]
impl Drop for SignalCoordinator {
    fn drop(&mut self) {
        self.task.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(root: &Path, paths: &[&str], ignores: &[&str]) -> WatchSpec {
        spec_with_debounce(root, paths, ignores, Duration::from_millis(300))
    }

    fn spec_with_debounce(
        root: &Path,
        paths: &[&str],
        ignores: &[&str],
        debounce: Duration,
    ) -> WatchSpec {
        WatchSpec::build(
            root.to_path_buf(),
            "Task description".to_string(),
            paths.iter().map(|value| value.to_string()).collect(),
            ignores.iter().map(|value| value.to_string()).collect(),
            BTreeSet::from([".evin/workspace.yaml".to_string()]),
            debounce,
        )
        .unwrap()
    }

    #[test]
    fn configured_ignores_win_and_specific_paths_reinclude_defaults() {
        let root = Path::new(".");
        let regular = spec(root, &["**"], &["generated/**"]);
        assert!(regular.matches("crates/evin/src/main.rs"));
        assert!(!regular.matches("generated/output.rs"));
        assert!(!regular.matches("target/debug/evin"));

        let target = spec(root, &["**", "target/snapshots/**"], &[]);
        assert!(target.matches("target/snapshots/result.txt"));
        assert!(!target.matches("target/debug/evin"));
    }

    #[test]
    fn configuration_inputs_are_mandatory() {
        let value = spec(Path::new("."), &["crates/**"], &[".evin/**"]);
        assert!(value.matches(".evin/workspace.yaml"));
        assert!(!value.matches(".evin/trace.json"));
        assert_eq!(value.debounce(), Duration::from_millis(300));
    }

    #[test]
    fn snapshot_diff_tracks_create_edit_and_delete() {
        let previous = HashMap::from([
            (
                "same".into(),
                FileStamp {
                    modified: None,
                    len: 1,
                },
            ),
            (
                "deleted".into(),
                FileStamp {
                    modified: None,
                    len: 1,
                },
            ),
            (
                "edited".into(),
                FileStamp {
                    modified: None,
                    len: 1,
                },
            ),
        ]);
        let current = HashMap::from([
            (
                "same".into(),
                FileStamp {
                    modified: None,
                    len: 1,
                },
            ),
            (
                "created".into(),
                FileStamp {
                    modified: None,
                    len: 1,
                },
            ),
            (
                "edited".into(),
                FileStamp {
                    modified: None,
                    len: 2,
                },
            ),
        ]);
        assert_eq!(
            diff_snapshots(&previous, &current),
            BTreeSet::from(["created".into(), "deleted".into(), "edited".into()])
        );
    }

    #[test]
    fn change_summary_is_concise() {
        let one = WatchChange {
            paths: BTreeSet::from(["crates/evin/src/main.rs".into()]),
        };
        assert_eq!(one.summary(), "crates/evin/src/main.rs");
        let many = WatchChange {
            paths: BTreeSet::from(["crates/a.rs".into(), "crates/b.rs".into()]),
        };
        assert_eq!(many.summary(), "2 files in crates");
        let nested = WatchChange {
            paths: BTreeSet::from([
                "crates/evin/src/main.rs".into(),
                "crates/evin/src/watch.rs".into(),
            ]),
        };
        assert_eq!(nested.summary(), "2 files in crates/evin/src");
    }

    #[tokio::test]
    async fn polling_watcher_detects_debounced_create_edit_and_delete() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join("src")).unwrap();
        let mut watcher = PollingWatcher::new(spec_with_debounce(
            temp.path(),
            &["src/**"],
            &["src/ignored/**"],
            Duration::from_millis(60),
        ));

        let first = temp.path().join("src/first.rs");
        let second = temp.path().join("src/second.rs");
        let first_for_task = first.clone();
        let second_for_task = second.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(120)).await;
            std::fs::write(first_for_task, "one").unwrap();
            tokio::time::sleep(Duration::from_millis(20)).await;
            std::fs::write(second_for_task, "two").unwrap();
        });
        let created = watcher.next_change().await;
        assert_eq!(created.summary(), "2 files in src");

        let edited_path = first.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(120)).await;
            std::fs::write(edited_path, "changed length").unwrap();
        });
        assert_eq!(watcher.next_change().await.summary(), "src/first.rs");

        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(120)).await;
            std::fs::remove_file(second).unwrap();
        });
        assert_eq!(watcher.next_change().await.summary(), "src/second.rs");
    }

    #[test]
    fn ignored_files_do_not_enter_snapshots() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("generated")).unwrap();
        std::fs::write(temp.path().join("generated/output.rs"), "ignored").unwrap();
        let value = spec(temp.path(), &["**"], &["generated/**"]);
        assert!(!snapshot(&value).contains_key("generated/output.rs"));
    }

    #[test]
    fn snapshots_use_literal_glob_roots() {
        let value = spec(
            Path::new("."),
            &["crates/**", "builtins/docs/**", "Cargo.toml"],
            &[],
        );
        assert_eq!(
            value.scan_roots,
            BTreeSet::from([
                "Cargo.toml".to_string(),
                "builtins/docs".to_string(),
                "crates".to_string(),
            ])
        );
    }

    #[test]
    fn selected_task_watch_specs_union_and_use_largest_debounce() {
        let tasks = vec![
            evin_config::models::TaskObject {
                name: "one".into(),
                watch: Some(evin_config::models::WatchConfig {
                    paths: vec!["crates/**".into()],
                    ignore: vec!["generated/**".into()],
                    debounce: "300ms".into(),
                }),
                ..Default::default()
            },
            evin_config::models::TaskObject {
                name: "two".into(),
                watch: Some(evin_config::models::WatchConfig {
                    paths: vec!["builtins/**".into()],
                    ignore: vec!["snapshots/**".into()],
                    debounce: "1s".into(),
                }),
                ..Default::default()
            },
        ];
        let (paths, ignores, debounce) = merge_watch_configs(&["one".into(), "two".into()], &tasks)
            .unwrap()
            .unwrap();
        assert_eq!(
            paths,
            BTreeSet::from(["builtins/**".into(), "crates/**".into()])
        );
        assert_eq!(
            ignores,
            BTreeSet::from(["generated/**".into(), "snapshots/**".into()])
        );
        assert_eq!(debounce, Duration::from_secs(1));
    }

    #[test]
    fn invalid_reload_configuration_retains_the_last_valid_matcher() {
        let root = Path::new(".");
        let mut watcher = PollingWatcher::new(spec(root, &["crates/**"], &[]));
        let invalid = Err(evin_diagnostic::Diagnostic::new(
            "Invalid YAML",
            evin_diagnostic::IssueCode::custom("invalid YAML"),
        ));
        assert!(!watcher.apply_resolved_spec(invalid));
        assert!(watcher.spec.matches("crates/evin/src/main.rs"));
        assert!(!watcher.spec.matches("builtins/docs/src/lib.rs"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn cleanup_distinguishes_graceful_completion_from_a_forced_signal() {
        let mut signals = SignalCoordinator::install();
        let mut completed = tokio::spawn(async { PipelineCompletion::Cancelled });
        let graceful = finish_session_until(
            &mut completed,
            &mut signals,
            tokio::time::Instant::now() + Duration::from_secs(1),
        )
        .await;
        assert_eq!(graceful, CleanupOutcome::Graceful);

        let mut pending = tokio::spawn(std::future::pending::<PipelineCompletion>());
        signals.inject();
        let forced = finish_session_until(
            &mut pending,
            &mut signals,
            tokio::time::Instant::now() + Duration::from_secs(1),
        )
        .await;
        assert_eq!(forced, CleanupOutcome::Forced);
        assert!(pending.is_finished());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn signal_coordinator_observes_process_termination_signal() {
        use signal_hook::consts::signal::SIGTERM;

        let mut signals = SignalCoordinator::install();
        signal_hook::low_level::raise(SIGTERM).expect("raise SIGTERM");
        tokio::time::timeout(Duration::from_secs(1), signals.next())
            .await
            .expect("signal coordinator timed out");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn signal_coordinator_remains_armed_after_a_cancelled_wait() {
        use signal_hook::consts::signal::SIGTERM;

        let mut signals = SignalCoordinator::install();
        tokio::select! {
            biased;
            _ = std::future::ready(()) => {}
            _ = signals.next() => panic!("unexpected signal"),
        }
        signal_hook::low_level::raise(SIGTERM).expect("raise SIGTERM");
        tokio::time::timeout(Duration::from_secs(1), signals.next())
            .await
            .expect("signal coordinator timed out after cancelled wait");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn signal_coordinator_remains_armed_after_a_subprocess() {
        use signal_hook::consts::signal::SIGTERM;

        let mut signals = SignalCoordinator::install();
        assert!(
            tokio::process::Command::new("true")
                .status()
                .await
                .unwrap()
                .success()
        );
        signal_hook::low_level::raise(SIGTERM).expect("raise SIGTERM");
        tokio::time::timeout(Duration::from_secs(1), signals.next())
            .await
            .expect("signal coordinator timed out after subprocess");
    }
}
