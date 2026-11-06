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

mod polling;
mod shutdown;
mod spec;
mod supervisor;

#[cfg(test)]
pub(crate) use polling::{FileStamp, PollingWatcher, WatchChange, diff_snapshots, snapshot};
#[cfg(test)]
pub(crate) use shutdown::{CleanupOutcome, SignalCoordinator, finish_session_until};
#[cfg(test)]
pub(crate) use spec::merge_watch_configs;
pub use spec::{WatchSpec, resolve_watch_spec};
pub use supervisor::{RunSession, run_once_with_signals, supervise};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::PipelineCompletion;
    use std::collections::{BTreeSet, HashMap};
    use std::path::Path;
    use std::time::Duration;

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
            BTreeSet::from([".evin/workspace.flow".to_string()]),
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
        assert!(value.matches(".evin/workspace.flow"));
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
            "Invalid Flow",
            evin_diagnostic::IssueCode::custom("invalid Flow"),
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
