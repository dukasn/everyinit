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

use evin_action_sdk::{
    Logger, PipelineMode, RunnerContext, git::GitProvider, secrets::SecretsProvider,
};
use std::path::Path;
use std::sync::Arc;

fn git(root: &Path, args: &[&str]) {
    assert!(
        std::process::Command::new("git")
            .args(args)
            .current_dir(root)
            .status()
            .unwrap()
            .success()
    );
}

fn context(root: &Path) -> RunnerContext {
    let secrets = Arc::new(SecretsProvider::default());
    let logger = Logger::new(
        "changelog".into(),
        tokio::sync::mpsc::unbounded_channel().0,
        secrets.clone(),
    );
    let provider = Arc::new(GitProvider::new(
        root.to_path_buf(),
        PipelineMode::Normal,
        logger,
    ));
    let mut ctx = RunnerContext::new(
        None,
        "changelog".into(),
        false,
        PipelineMode::Normal,
        tokio::sync::mpsc::unbounded_channel().0,
        vec![],
        None,
        HashMap::new(),
        root.to_path_buf(),
        HashMap::new(),
        provider,
        secrets,
    );
    ctx.policy = Arc::new(evin_repository_policy::RepositoryPolicy::default());
    ctx
}

fn repository(with_pending: bool) -> tempfile::TempDir {
    let temp = tempfile::TempDir::new().unwrap();
    let root = temp.path();
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "Test"][..],
        &["config", "user.email", "test@example.test"][..],
        &["config", "commit.gpgSign", "false"][..],
        &["config", "tag.gpgSign", "false"][..],
    ] {
        git(root, args);
    }
    std::fs::write(root.join("file"), "one\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "feat: initial"]);
    git(root, &["tag", "-a", "v1.0.0", "-m", "v1.0.0"]);
    if with_pending {
        std::fs::write(root.join("file"), "two\n").unwrap();
        git(root, &["commit", "-aqm", "fix(core): repair the parser"]);
    }
    temp
}

#[tokio::test]
async fn blank_section_resolution_uses_fluent_diagnostic() {
    let temp = tempfile::tempdir().unwrap();
    let mut ctx = context(temp.path());
    ctx.env = evin_action_sdk::Environment::new(HashMap::from([(
        "EMPTY_SECTION".to_string(),
        String::new(),
    )]));
    let with =
        evin_flow::parse_yaml("section: '${env.EMPTY_SECTION}'\nnotes_path: notes.md\n").unwrap();

    let error = execute(&ctx, Some(with)).await.unwrap_err();
    assert_eq!(error.code.key, "changelog-section-template-invalid");
    let detail = &error.code.args[0].1;
    assert_eq!(
        evin_locales::render_issue(&error.code),
        evin_locales::t!("changelog-section-template-invalid", error = detail)
    );
}

#[tokio::test]
async fn version_template_resolution_failure_uses_fluent_diagnostic() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path());
    let with = evin_flow::parse_yaml("version: '${missing}'\n").unwrap();

    let error = execute(&ctx, Some(with)).await.unwrap_err();
    assert_eq!(error.code.key, "changelog-template-resolution-failed");
    let detail = &error.code.args[0].1;
    assert_eq!(
        evin_locales::render_issue(&error.code),
        evin_locales::t!("changelog-template-resolution-failed", error = detail)
    );
}

#[tokio::test]
async fn changelog_read_failure_uses_fluent_diagnostic() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path());
    let with =
        evin_flow::parse_yaml("path: missing.md\nsection: v1.0.0\nnotes_path: notes.md\n").unwrap();

    let error = execute(&ctx, Some(with)).await.unwrap_err();
    assert_eq!(error.code.key, "changelog-file-read-failed");
    let path = &error.code.args[0].1;
    let detail = &error.code.args[1].1;
    assert_eq!(
        evin_locales::render_issue(&error.code),
        evin_locales::t!("changelog-file-read-failed", path = path, error = detail)
    );
}

#[tokio::test]
async fn notes_write_failure_uses_fluent_diagnostic() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("CHANGELOG.md"), "## [v1.0.0]\n- notes\n").unwrap();
    std::fs::write(temp.path().join("blocked"), "file").unwrap();
    let ctx = context(temp.path());
    let with = evin_flow::parse_yaml("section: v1.0.0\nnotes_path: blocked/notes.md\n").unwrap();

    let error = execute(&ctx, Some(with)).await.unwrap_err();
    assert_eq!(error.code.key, "file-write-failed");
    let path = &error.code.args[0].1;
    let detail = &error.code.args[1].1;
    assert_eq!(
        evin_locales::render_issue(&error.code),
        evin_locales::t!("file-write-failed", path = path, error = detail)
    );
}

#[tokio::test]
async fn git_execution_failure_uses_fluent_diagnostic() {
    let temp = tempfile::tempdir().unwrap();
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "Test"][..],
        &["config", "user.email", "test@example.test"][..],
    ] {
        git(temp.path(), args);
    }
    std::fs::write(temp.path().join(".git/config"), "[broken\n").unwrap();
    let ctx = context(temp.path());

    let error = execute(&ctx, None).await.unwrap_err();
    assert_eq!(error.code.key, "git-execution-failed");
    let detail = &error.code.args[0].1;
    assert_eq!(
        evin_locales::render_issue(&error.code),
        evin_locales::t!("git-execution-failed", error = detail)
    );
}

#[tokio::test]
async fn notes_path_receives_only_the_pending_section_body() {
    let temp = repository(true);
    let ctx = context(temp.path());
    let with =
        evin_flow::parse_yaml("version: 1.0.1\nshow_hashes: false\nnotes_path: target/notes.md\n")
            .unwrap();
    execute(&ctx, Some(with)).await.unwrap();

    let notes = std::fs::read_to_string(temp.path().join("target/notes.md")).unwrap();
    assert_eq!(notes, "### Bug Fixes\n\n- **core:** repair the parser\n");
    let changelog = std::fs::read_to_string(temp.path().join("CHANGELOG.md")).unwrap();
    assert!(changelog.contains("## [v1.0.1]"));
    assert!(changelog.contains(&notes));
    assert_eq!(
        ctx.state.outputs.lock().unwrap()["notes_path"],
        "target/notes.md"
    );
}

#[tokio::test]
async fn notes_path_is_not_written_without_a_pending_section() {
    let temp = repository(false);
    let ctx = context(temp.path());
    let with = evin_flow::parse_yaml("notes_path: notes.md\n").unwrap();
    execute(&ctx, Some(with)).await.unwrap();

    assert!(!temp.path().join("notes.md").exists());
    assert!(!ctx.state.outputs.lock().unwrap().contains_key("notes_path"));
}

#[tokio::test]
async fn release_commit_filter_and_chore_section_follow_git_rules() {
    let temp = repository(false);
    for subject in [
        "chore(release): 1.0.1",
        "chore(release): v1.0.2",
        "chore(ship): 1.0.3",
        "chore(sdk): maintain the toolkit",
        "fix(core): repair the parser",
    ] {
        git(temp.path(), &["commit", "--allow-empty", "-qm", subject]);
    }
    let mut ctx = context(temp.path());
    let mut rules = evin_repository_policy::RepositoryPolicy::default();
    let releases = rules.releases.as_mut().unwrap();
    releases
        .change_types
        .get_mut("chore")
        .unwrap()
        .changelog_section = Some("Maintenance".to_string());
    ctx.policy = Arc::new(rules);
    let with = evin_flow::parse_yaml("version: 1.0.1\nnotes_path: target/notes.md\n").unwrap();
    execute(&ctx, Some(with)).await.unwrap();

    let notes = std::fs::read_to_string(temp.path().join("target/notes.md")).unwrap();
    assert!(notes.contains("### Maintenance"));
    assert!(notes.contains("**sdk:** maintain the toolkit"));
    assert!(notes.contains("**ship:** 1.0.3"));
    assert!(notes.contains("**core:** repair the parser"));
    assert!(!notes.contains("**release:** 1.0.1"));
    assert!(!notes.contains("**release:** v1.0.2"));

    Arc::make_mut(&mut ctx.policy)
        .releases
        .as_mut()
        .unwrap()
        .commit_message = Some("chore(ship): {version}".to_string());
    let with = evin_flow::parse_yaml("version: 1.0.1\nnotes_path: target/notes.md\n").unwrap();
    execute(&ctx, Some(with)).await.unwrap();
    let custom_notes = std::fs::read_to_string(temp.path().join("target/notes.md")).unwrap();
    assert!(!custom_notes.contains("**ship:** 1.0.3"));
    assert!(custom_notes.contains("**release:** 1.0.1"));
}

#[tokio::test]
async fn exports_existing_section_without_regenerating_changelog() {
    let temp = repository(true);
    let changelog =
        "# Changelog\n\n## [v1.0.1]\n\n### Bug Fixes\n\n- reviewed fix\n\n## [v1.0.0]\n\n- older\n";
    std::fs::write(temp.path().join("CHANGELOG.md"), changelog).unwrap();
    let ctx = context(temp.path());
    let with = evin_flow::parse_yaml("section: v1.0.1\nnotes_path: target/notes.md\n").unwrap();
    execute(&ctx, Some(with)).await.unwrap();
    assert_eq!(
        std::fs::read_to_string(temp.path().join("target/notes.md")).unwrap(),
        "### Bug Fixes\n\n- reviewed fix\n"
    );
    assert_eq!(
        std::fs::read_to_string(temp.path().join("CHANGELOG.md")).unwrap(),
        changelog
    );
}

#[tokio::test]
async fn export_rejects_missing_empty_duplicate_and_same_path() {
    let temp = repository(false);
    let path = temp.path().join("CHANGELOG.md");
    for changelog in [
        "## [v1.0.0]\n- older\n",
        "## [v1.0.1]\n\n## [v1.0.0]\n- older\n",
        "## [v1.0.1]\n- first\n## [v1.0.1]\n- duplicate\n",
    ] {
        std::fs::write(&path, changelog).unwrap();
        let ctx = context(temp.path());
        let with = evin_flow::parse_yaml("section: v1.0.1\nnotes_path: notes.md\n").unwrap();
        let error = execute(&ctx, Some(with)).await.unwrap_err();
        if changelog == "## [v1.0.0]\n- older\n"
            || changelog == "## [v1.0.1]\n\n## [v1.0.0]\n- older\n"
        {
            assert_eq!(error.code.key, "changelog-section-missing");
            let expected = evin_locales::t!("changelog-section-missing", section = "v1.0.1");
            assert!(!expected.starts_with("<missing locale key:"));
            assert_eq!(evin_locales::render_issue(&error.code), expected);
        } else if changelog == "## [v1.0.1]\n- first\n## [v1.0.1]\n- duplicate\n" {
            let detail = "Duplicate changelog sections for v1.0.1";
            let expected = evin_locales::t!("changelog-section-invalid", error = detail);
            assert_eq!(error.code.key, "changelog-section-invalid",);
            assert_eq!(evin_locales::render_issue(&error.code), expected);
        }
        assert!(!temp.path().join("notes.md").exists());
    }
    std::fs::write(&path, "## [v1.0.1]\n- keep\n").unwrap();
    let ctx = context(temp.path());
    let with = evin_flow::parse_yaml("section: v1.0.1\nnotes_path: ./CHANGELOG.md\n").unwrap();
    let error = execute(&ctx, Some(with)).await.unwrap_err();
    assert_eq!(error.code.key, "changelog-section-paths-must-differ");
    assert_eq!(
        evin_locales::render_issue(&error.code),
        evin_locales::t!("changelog-section-paths-must-differ")
    );
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        "## [v1.0.1]\n- keep\n"
    );

    let error =
        validate_config(Some(&evin_flow::parse_yaml("section: v1.0.1\n").unwrap())).unwrap_err();
    assert_eq!(
        error.message,
        evin_locales::t!("section-must-be-non-empty-and-requires-notes-path")
    );
}

mod config;
