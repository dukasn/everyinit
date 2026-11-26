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

use crate::engine::Phase;
use crate::phases::{init::*, run::*, sync::*};

pub mod check;
pub mod discovery;

pub(crate) fn file_uri(
    path: impl AsRef<std::path::Path>,
) -> Result<String, evin_diagnostic::Diagnostic> {
    let path = path.as_ref();
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| path_diagnostic(path, error))?
            .join(path)
    };
    let absolute = absolute.canonicalize().unwrap_or(absolute);
    url::Url::from_file_path(&absolute)
        .map(String::from)
        .map_err(|()| path_diagnostic(&absolute, "path cannot be represented as a file URI"))
}

fn path_diagnostic(
    path: &std::path::Path,
    error: impl std::fmt::Display,
) -> evin_diagnostic::Diagnostic {
    evin_diagnostic::Diagnostic::new(
        "Invalid file path",
        evin_diagnostic::IssueCode::custom(format!("{}: {error}", path.display())),
    )
}

pub fn run_phases() -> Vec<Box<dyn Phase>> {
    vec![Box::new(ResolveConfigPhase), Box::new(ExecuteTasksPhase)]
}

pub fn sync_phases() -> Vec<Box<dyn Phase>> {
    vec![
        Box::new(ResolveConfigPhase),
        Box::new(ReconcileReleaseLockPhase),
        Box::new(SyncNativeActionsPhase),
        Box::new(SyncGitHooksPhase),
    ]
}

pub fn init_phases(
    workspace: bool,
    policy: bool,
    env: bool,
    all: bool,
    module: Option<String>,
    target_path: Option<std::path::PathBuf>,
) -> Vec<Box<dyn Phase>> {
    vec![
        Box::new(ResolveInitPhase),
        Box::new(InitWorkspacePhase {
            workspace,
            policy,
            env,
            all,
            module,
            target_path,
        }),
    ]
}

pub(crate) fn builtin_actions()
-> std::collections::HashMap<String, std::sync::Arc<dyn evin_action_sdk::Action>> {
    use std::sync::Arc;
    [
        (
            "evin/version-bumper@v1",
            Arc::new(evin_action_version_bumper::VersionBumperAction)
                as Arc<dyn evin_action_sdk::Action>,
        ),
        (
            "evin/changelog@v1",
            Arc::new(evin_action_changelog::ChangelogAction),
        ),
        (
            "evin/archive@v1",
            Arc::new(evin_action_archive::ArchiveAction),
        ),
        (
            "evin/package-deb@v1",
            Arc::new(evin_action_archive::DebPackageAction),
        ),
        (
            "evin/package-apk@v1",
            Arc::new(evin_action_archive::ApkPackageAction),
        ),
        (
            "evin/package-arch@v1",
            Arc::new(evin_action_archive::ArchPackageAction),
        ),
        (
            "evin/git-release@v1",
            Arc::new(evin_action_git_release::GitReleaseAction),
        ),
        (
            "evin/review-request@v1",
            Arc::new(evin_action_review_request::ReviewRequestAction),
        ),
        ("evin/docs@v1", Arc::new(evin_action_docs::DocsAction)),
        (
            "evin/catalog@v1",
            Arc::new(evin_action_catalog::CatalogAction),
        ),
        (
            "evin/release-sync@v1",
            Arc::new(evin_action_release_sync::ReleaseSyncAction),
        ),
    ]
    .into_iter()
    .map(|(name, action)| (name.to_string(), action))
    .collect()
}

#[cfg(test)]
mod tests {
    use super::builtin_actions;
    use super::file_uri;

    #[test]
    fn file_uri_encodes_an_absolute_path() {
        let temp = tempfile::tempdir().unwrap();
        let directory = temp.path().join("directory with spaces");
        std::fs::create_dir(&directory).unwrap();

        let uri = file_uri(&directory).unwrap();
        let parsed = url::Url::parse(&uri).unwrap();

        assert_eq!(parsed.scheme(), "file");
        assert_eq!(
            parsed.to_file_path().unwrap(),
            directory.canonicalize().unwrap()
        );
        assert!(!uri.contains('\\'));
    }

    #[test]
    fn every_builtin_action_declares_editor_metadata() {
        for (id, action) in builtin_actions() {
            let descriptor = action.editor_descriptor().expect("builtin descriptor");
            assert!(!descriptor.summary.is_empty(), "{id} summary");
            assert!(
                descriptor.children_at(None).next().is_some(),
                "{id} root fields"
            );
            for field in descriptor.fields {
                assert!(!field.description.is_empty(), "{id} field {}", field.path);
                assert!(
                    !field.snippet().is_empty(),
                    "{id} field {} snippet",
                    field.path
                );
            }
        }
    }

    #[test]
    fn native_package_formats_have_distinct_action_boundaries() {
        let actions = builtin_actions();
        for id in [
            "evin/package-deb@v1",
            "evin/package-apk@v1",
            "evin/package-arch@v1",
        ] {
            assert!(actions.contains_key(id), "missing {id}");
            assert!(actions[id].package_release_contract().is_some(), "{id}");
        }
        assert!(
            actions["evin/archive@v1"]
                .package_release_contract()
                .is_none()
        );
    }
}
