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

pub fn run_phases() -> Vec<Box<dyn Phase>> {
    vec![Box::new(ResolveConfigPhase), Box::new(ExecuteTasksPhase)]
}

pub fn sync_phases() -> Vec<Box<dyn Phase>> {
    vec![
        Box::new(ResolveConfigPhase),
        Box::new(SyncNativeActionsPhase),
        Box::new(SyncGitHooksPhase),
    ]
}

pub fn init_phases(
    workspace: bool,
    gitrules: bool,
    env: bool,
    all: bool,
    module: Option<String>,
    target_path: Option<std::path::PathBuf>,
) -> Vec<Box<dyn Phase>> {
    vec![
        Box::new(ResolveInitPhase),
        Box::new(InitWorkspacePhase {
            workspace,
            gitrules,
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
            "evin/git-release@v1",
            Arc::new(evin_action_git_release::GitReleaseAction),
        ),
        ("evin/docs@v1", Arc::new(evin_action_docs::DocsAction)),
    ]
    .into_iter()
    .map(|(name, action)| (name.to_string(), action))
    .collect()
}
