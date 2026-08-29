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

use evin_action_ci::CiAction;
use evin_action_sdk::{Action, Logger, RunnerContext, secrets::SecretsProvider};
use std::sync::Arc;
use tempfile::tempdir;

#[tokio::test]
async fn test_ci_action_reconciles_and_emits_plan_in_actions_folder() {
    let temp = tempdir().unwrap();
    let root = temp.path();

    // 1. Initialize git repo and create a tag
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(root)
        .output()
        .unwrap();

    std::process::Command::new("git")
        .args(["config", "user.name", "Evin Test"])
        .current_dir(root)
        .output()
        .unwrap();

    std::process::Command::new("git")
        .args(["config", "user.email", "test@evin.dev"])
        .current_dir(root)
        .output()
        .unwrap();

    std::process::Command::new("git")
        .args(["config", "commit.gpgSign", "false"])
        .current_dir(root)
        .output()
        .unwrap();

    std::fs::write(root.join("file.txt"), "hello").unwrap();
    std::process::Command::new("git")
        .args(["add", "file.txt"])
        .current_dir(root)
        .output()
        .unwrap();

    std::process::Command::new("git")
        .args(["commit", "-m", "chore: initial commit"])
        .current_dir(root)
        .output()
        .unwrap();

    std::process::Command::new("git")
        .args([
            "tag",
            "-a",
            "v1.0.0-canary.1",
            "-m",
            "v1.0.0-canary.1 Release",
        ])
        .current_dir(root)
        .output()
        .unwrap();

    // 2. Create CHANGELOG.md
    let changelog_content = r#"# Changelog
## [v1.0.0-canary.1] - 2026-08-25
### Features
- added ECP-1 delivery protocol
"#;
    std::fs::write(root.join("CHANGELOG.md"), changelog_content).unwrap();

    // 3. Create dist/ artifact
    let dist_dir = root.join("dist");
    std::fs::create_dir_all(&dist_dir).unwrap();
    std::fs::write(
        dist_dir.join("evin-v1.0.0-canary.1-linux.tar.gz"),
        "binary content",
    )
    .unwrap();

    // 4. Create .evin/actions/ci/remote-state.v1.json (empty remote releases)
    let ci_dir = root.join(".evin/actions/ci");
    std::fs::create_dir_all(&ci_dir).unwrap();
    let remote_state_json = r#"{
      "$schema": "https://evin.pages.dev/schemas/v1/ci-remote-state.json",
      "v": 1,
      "platform": "github",
      "repository": "dukasn/evin",
      "releases": []
    }"#;
    std::fs::write(ci_dir.join("remote-state.v1.json"), remote_state_json).unwrap();

    // 5. Run Action
    let (log_tx, _) = tokio::sync::mpsc::unbounded_channel();
    let secrets = Arc::new(SecretsProvider::new(root));
    let logger = Logger::new("ci".to_string(), log_tx.clone(), secrets.clone());
    let ctx = RunnerContext::new(
        None,
        "ci".to_string(),
        false,
        evin_action_sdk::PipelineMode::Normal,
        log_tx,
        vec![],
        None,
        std::collections::HashMap::new(),
        root.to_path_buf(),
        std::collections::HashMap::new(),
        std::sync::Arc::new(evin_action_sdk::git::GitProvider::new(
            root.to_path_buf(),
            evin_action_sdk::PipelineMode::Normal,
            logger,
        )),
        secrets,
    );

    let action = CiAction;
    let result = action.execute(&ctx, None).await;
    assert!(result.is_ok(), "Action execution failed: {:?}", result);

    // 6. Verify outputs are in .evin/actions/ci/
    let plan_path = root.join(".evin/actions/ci/plan.v1.json");
    assert!(
        plan_path.exists(),
        "Plan file was not created at expected location"
    );

    let plan_str = std::fs::read_to_string(&plan_path).unwrap();
    let plan: serde_json::Value = serde_json::from_str(&plan_str).unwrap();

    assert_eq!(plan["v"], 1);
    assert_eq!(plan["status"], 200);
    assert_eq!(plan["reconciled"], true);
    assert_eq!(plan["delta_count"], 2);

    let actions = plan["plan"].as_array().unwrap();
    assert_eq!(actions.len(), 2);

    // First action: REL_INIT (201)
    assert_eq!(actions[0]["code"], "REL_INIT");
    assert_eq!(actions[0]["op"], 201);
    assert_eq!(actions[0]["reason"], "DIFF_ABSENT");
    assert_eq!(actions[0]["ref"], "v1.0.0-canary.1");
    assert_eq!(
        actions[0]["notes_src"],
        ".evin/actions/ci/notes/v1.0.0-canary.1.md"
    );

    // Second action: ART_PUT (301)
    assert_eq!(actions[1]["code"], "ART_PUT");
    assert_eq!(actions[1]["op"], 301);
    assert_eq!(actions[1]["reason"], "DIFF_ABSENT");
    assert_eq!(actions[1]["dest"], "evin-v1.0.0-canary.1-linux.tar.gz");

    // 7. Verify release note slice was created
    let note_path = root.join(".evin/actions/ci/notes/v1.0.0-canary.1.md");
    assert!(note_path.exists(), "Note slice was not written");
    let note_content = std::fs::read_to_string(note_path).unwrap();
    assert!(note_content.contains("added ECP-1 delivery protocol"));
}
