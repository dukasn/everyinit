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

use super::*;
use crate::env::mock::MockGitEnv;

#[tokio::test]
async fn test_install_fresh_hook() {
    let env = MockGitEnv::new();
    let manager = HookManager::new(env);
    let hook = GitHook::PreCommit;

    manager.install_hook(&hook, "exec evin run lint").await.unwrap();

    let content = manager.env.read_hook(&hook).await.unwrap().unwrap();
    assert!(content.contains("#!/usr/bin/env bash"));
    assert!(content.contains(START_MARKER));
    assert!(content.contains("exec evin run lint"));
    assert!(manager.env.hook_is_executable(&hook));
}

#[tokio::test]
async fn test_install_append_hook() {
    let env = MockGitEnv::new();
    let hook = GitHook::PreCommit;
    env.write_hook(&hook, "#!/usr/bin/env bash\necho 'hello'\n")
        .await
        .unwrap();

    let manager = HookManager::new(env);
    manager.install_hook(&hook, "exec evin run lint").await.unwrap();

    let content = manager.env.read_hook(&hook).await.unwrap().unwrap();
    assert!(content.contains("echo 'hello'"));
    assert!(content.contains("exec evin run lint"));
}

#[tokio::test]
async fn test_prune_hook() {
    let env = MockGitEnv::new();
    let hook = GitHook::PreCommit;
    let original = "#!/usr/bin/env bash\necho 'hello'\n# --- EVIN Start ---\nexec evin run lint\n# --- EVIN End ---\n";
    env.write_hook(&hook, original).await.unwrap();

    let manager = HookManager::new(env);
    manager.prune_hook(&hook).await.unwrap();

    let content = manager.env.read_hook(&hook).await.unwrap().unwrap();
    assert!(!content.contains(START_MARKER));
    assert!(!content.contains("exec evin run lint"));
    assert!(content.contains("echo 'hello'"));
}

#[tokio::test]
async fn plans_create_update_and_unchanged_hooks() {
    let env = MockGitEnv::new();
    let manager = HookManager::new(env);
    let hook = GitHook::PreCommit;

    let create = manager.plan_hook(&hook, "exec evin hook").await.unwrap();
    assert_eq!(create.change, HookChange::Create);

    manager.apply_plan(&create).await.unwrap();
    let unchanged = manager.plan_hook(&hook, "exec evin hook").await.unwrap();
    assert_eq!(unchanged.change, HookChange::Unchanged);

    let update = manager
        .plan_hook(&hook, "exec evin hook -- changed")
        .await
        .unwrap();
    assert_eq!(update.change, HookChange::Update);
}

#[tokio::test]
async fn non_executable_matching_hook_is_an_update() {
    let env = MockGitEnv::new();
    let manager = HookManager::new(env);
    let hook = GitHook::PreCommit;
    let create = manager.plan_hook(&hook, "exec evin hook").await.unwrap();
    manager.env.write_hook(&hook, &create.content).await.unwrap();

    let plan = manager.plan_hook(&hook, "exec evin hook").await.unwrap();

    assert_eq!(plan.change, HookChange::Update);
}
