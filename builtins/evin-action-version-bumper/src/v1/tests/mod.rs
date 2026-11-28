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
use evin_repository_policy::{PackageReleasePolicy, PackageRevisionPolicy};
use std::collections::BTreeMap;
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
    context_with_mode(root, PipelineMode::Normal)
}

fn context_with_mode(root: &Path, mode: PipelineMode) -> RunnerContext {
    let secrets = Arc::new(SecretsProvider::default());
    let logger = Logger::new(
        "bump".into(),
        tokio::sync::mpsc::unbounded_channel().0,
        secrets.clone(),
    );
    let provider = Arc::new(GitProvider::new(root.to_path_buf(), mode, logger));
    RunnerContext::new(
        None,
        "bump".into(),
        false,
        mode,
        tokio::sync::mpsc::unbounded_channel().0,
        vec![],
        None,
        HashMap::new(),
        root.to_path_buf(),
        HashMap::new(),
        provider,
        secrets,
    )
}

#[tokio::test]
async fn workspace_name_channel_variable_errors_use_fluent_messages() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path());

    let failure = resolve_channel_var("workspace.name", &ctx)
        .await
        .unwrap_err();
    assert_eq!(
        failure.message.key,
        "version-bumper-workspace-name-resolution-failed"
    );
    assert_eq!(
        evin_locales::render_issue(&failure.message),
        evin_locales::t!("version-bumper-workspace-name-resolution-failed")
    );
    let hint = failure.hint.as_ref().unwrap();
    assert_eq!(hint.key, "version-bumper-workspace-name-requirement");
    assert_eq!(
        evin_locales::render_issue(hint),
        evin_locales::t!("version-bumper-workspace-name-requirement")
    );

    let failure = resolve_channel_var("not.a.real.variable", &ctx)
        .await
        .unwrap_err();
    assert_eq!(
        failure.message.key,
        "version-bumper-channel-variable-unknown"
    );
    assert_eq!(
        evin_locales::render_issue(&failure.message),
        evin_locales::t!(
            "version-bumper-channel-variable-unknown",
            name = "not.a.real.variable"
        )
    );
    assert!(failure.hint.is_none());
    assert_eq!(
        evin_locales::t!("version-bumper-channel-variable-location-hint"),
        "missing variable here"
    );

    let env_name = format!("EVIN_TEST_UNSET_{}", std::process::id());
    let failure = resolve_channel_var(&format!("env.{env_name}"), &ctx)
        .await
        .unwrap_err();
    assert_eq!(
        failure.message.key,
        "version-bumper-environment-variable-missing"
    );
    assert_eq!(
        evin_locales::render_issue(&failure.message),
        evin_locales::t!("version-bumper-environment-variable-missing")
    );
    let hint = failure.hint.as_ref().unwrap();
    assert_eq!(hint.key, "version-bumper-environment-variable-required");
    assert_eq!(
        evin_locales::render_issue(hint),
        evin_locales::t!(
            "version-bumper-environment-variable-required",
            name = env_name
        )
    );
}

#[tokio::test]
async fn git_channel_variable_failures_use_fluent_messages_and_hints() {
    let temp = tempfile::tempdir().unwrap();
    init_repo(temp.path());
    std::fs::write(temp.path().join(".git/config"), "[broken\n").unwrap();
    let ctx = context(temp.path());

    for (variable, message_key, message, hint_key, hint_message) in [
        (
            "git.branch",
            "version-bumper-git-branch-resolution-failed",
            evin_locales::t!("version-bumper-git-branch-resolution-failed"),
            "version-bumper-git-branch-requirement",
            evin_locales::t!("version-bumper-git-branch-requirement"),
        ),
        (
            "git.short_sha",
            "version-bumper-git-short-sha-resolution-failed",
            evin_locales::t!("version-bumper-git-short-sha-resolution-failed"),
            "version-bumper-git-short-sha-requirement",
            evin_locales::t!("version-bumper-git-short-sha-requirement"),
        ),
    ] {
        let failure = resolve_channel_var(variable, &ctx).await.unwrap_err();
        assert_eq!(failure.message.key, message_key);
        assert_eq!(evin_locales::render_issue(&failure.message), message);
        let hint = failure.hint.as_ref().unwrap();
        assert_eq!(hint.key, hint_key);
        assert_eq!(evin_locales::render_issue(hint), hint_message);
    }
}

#[tokio::test]
async fn undefined_channel_errors_use_fluent_diagnostic() {
    let temp = tempfile::tempdir().unwrap();
    init_repo(temp.path());
    std::fs::write(temp.path().join("version.txt"), "0.1.0\n").unwrap();
    git(temp.path(), &["add", "version.txt"]);
    git(temp.path(), &["commit", "-m", "feat: initialize project"]);
    let ctx = context(temp.path());
    let config = evin_flow::parse_yaml("channel: unknown-channel\n").unwrap();

    let error = execute(&ctx, Some(config)).await.unwrap_err();
    assert_eq!(error.code.key, "version-bumper-channel-undefined");
    assert_eq!(
        evin_locales::render_issue(&error.code),
        evin_locales::t!(
            "version-bumper-channel-undefined",
            channel = "unknown-channel"
        )
    );
}

#[tokio::test]
async fn package_release_preconditions_use_fluent_diagnostics() {
    let temp = tempfile::tempdir().unwrap();
    let mut ctx = context(temp.path());
    let mut rules = evin_repository_policy::RepositoryPolicy::default();
    rules.releases.as_mut().unwrap().packages = BTreeMap::from([(
        "app".into(),
        PackageReleasePolicy {
            watch: vec![],
            targets: vec!["deb".into()],
            revision: PackageRevisionPolicy::default(),
        },
    )]);
    ctx.policy = Arc::new(rules);
    let error = execute(
        &ctx,
        Some(evin_flow::parse_yaml("initial_version: 1.0.0\n").unwrap()),
    )
    .await
    .unwrap_err();
    let expected = evin_locales::t!(
        "version-bumper-package-policy-invalid",
        error = "invalid package release policy: package 'app' has no watched paths"
    );
    assert_eq!(error.code.key, "version-bumper-package-policy-invalid");
    assert_eq!(evin_locales::render_issue(&error.code), expected);
    assert_eq!(error.title, expected);

    let temp = tempfile::tempdir().unwrap();
    let mut ctx = context(temp.path());
    ctx.policy = Arc::new(package_release_rules());
    let with =
        evin_flow::parse_yaml("initial_version: 1.0.0\ngit:\n  commit: false\n  tag: false\n")
            .unwrap();
    let error = execute(&ctx, Some(with)).await.unwrap_err();
    assert_eq!(
        error.code.key,
        "version-bumper-package-release-requires-git"
    );
    let expected = evin_locales::t!("version-bumper-package-release-requires-git");
    assert_eq!(evin_locales::render_issue(&error.code), expected);
    assert_eq!(error.title, expected);
}

#[tokio::test]
async fn package_release_rejects_staged_and_watched_worktree_changes_with_fluent_messages() {
    for staged in [true, false] {
        let temp = tempfile::tempdir().unwrap();
        init_repo(temp.path());
        std::fs::create_dir_all(temp.path().join("src")).unwrap();
        std::fs::write(temp.path().join("src/app"), "pending\n").unwrap();
        if staged {
            git(temp.path(), &["add", "src/app"]);
        }

        let mut ctx = context(temp.path());
        ctx.policy = Arc::new(package_release_rules());
        let with =
            evin_flow::parse_yaml("initial_version: 1.0.0\ngit:\n  commit: true\n  tag: true\n")
                .unwrap();
        let error = execute(&ctx, Some(with)).await.unwrap_err();
        if staged {
            let expected = evin_locales::t!("version-bumper-package-release-index-not-empty");
            assert_eq!(
                error.code.key,
                "version-bumper-package-release-index-not-empty"
            );
            assert_eq!(evin_locales::render_issue(&error.code), expected);
            assert_eq!(error.title, expected);
        } else {
            let expected = evin_locales::t!(
                "version-bumper-package-release-worktree-dirty",
                paths = "src/app"
            );
            assert_eq!(
                error.code.key,
                "version-bumper-package-release-worktree-dirty"
            );
            assert_eq!(evin_locales::render_issue(&error.code), expected);
            assert_eq!(error.title, expected);
        }
    }
}

fn package_release_rules() -> evin_repository_policy::RepositoryPolicy {
    let mut rules = evin_repository_policy::RepositoryPolicy::default();
    rules.releases.as_mut().unwrap().packages = BTreeMap::from([(
        "app".into(),
        PackageReleasePolicy {
            watch: vec!["src/**".into()],
            targets: vec!["deb".into()],
            revision: PackageRevisionPolicy::default(),
        },
    )]);
    rules
}

fn init_repo(root: &Path) {
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "Test"][..],
        &["config", "user.email", "test@example.test"][..],
        &["config", "commit.gpgSign", "false"][..],
        &["config", "tag.gpgSign", "false"][..],
    ] {
        git(root, args);
    }
}

#[test]
fn file_and_release_operation_diagnostics_use_fluent_messages() {
    let cases = [
        (
            file_path_error(
                "version-bumper-file-read-failed",
                "Cargo.toml",
                "permission denied",
            ),
            "version-bumper-file-read-failed",
            evin_locales::t!(
                "version-bumper-file-read-failed",
                path = "Cargo.toml",
                error = "permission denied"
            ),
            evin_locales::t!("version-bumper-file-read-failed-title"),
        ),
        (
            file_template_error(
                "package.json",
                evin_locales::t!("version-bumper-file-match-field"),
                "unknown variable",
            ),
            "version-bumper-file-template-resolution-failed",
            evin_locales::t!(
                "version-bumper-file-template-resolution-failed",
                path = "package.json",
                field = evin_locales::t!("version-bumper-file-match-field"),
                error = "unknown variable"
            ),
            evin_locales::t!("version-bumper-file-template-resolution-failed-title"),
        ),
        (
            fixed_version_template_error("unknown variable"),
            "version-bumper-fixed-version-template-resolution-failed",
            evin_locales::t!(
                "version-bumper-fixed-version-template-resolution-failed",
                error = "unknown variable"
            ),
            evin_locales::t!("version-bumper-fixed-version-template-resolution-failed-title"),
        ),
        (
            git_message_template_error("unknown variable"),
            "version-bumper-git-message-template-failed",
            evin_locales::t!(
                "version-bumper-git-message-template-failed",
                error = "unknown variable"
            ),
            evin_locales::t!("version-bumper-git-message-template-failed-title"),
        ),
        (
            release_state_error(
                evin_locales::t!("version-bumper-release-operation-reading-lock"),
                "permission denied",
            ),
            "version-bumper-release-state-operation-failed",
            evin_locales::t!(
                "version-bumper-release-state-operation-failed",
                operation = evin_locales::t!("version-bumper-release-operation-reading-lock"),
                error = "permission denied"
            ),
            evin_locales::t!("version-bumper-release-state-operation-failed-title"),
        ),
    ];

    for (diagnostic, key, expected, expected_title) in cases {
        assert_eq!(diagnostic.code.key, key);
        assert_eq!(evin_locales::render_issue(&diagnostic.code), expected);
        assert_eq!(diagnostic.title, expected_title);
    }
}

#[tokio::test]
async fn first_release_accepts_existing_initial_version_when_deferred() {
    let temp = tempfile::TempDir::new().unwrap();
    init_repo(temp.path());
    std::fs::write(temp.path().join("VERSION"), "0.1.0\n").unwrap();
    std::fs::write(
        temp.path().join("Cargo.toml"),
        "[package]\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    git(temp.path(), &["add", "."]);
    git(temp.path(), &["commit", "-qm", "feat: initial"]);
    let mut ctx = context(temp.path());
    ctx.policy = Arc::new(evin_repository_policy::RepositoryPolicy::default());
    let with = evin_flow::parse_yaml(
            "initial_version: 0.1.0\nfiles:\n  - Cargo.toml\n  - path: VERSION\n    match: '(?m)^${previous_version}$'\n    write: '${version}'\ngit:\n  commit: true\n  tag: true\n  defer: true\n  message: 'release: ${version}'\n",
        )
        .unwrap();
    execute(&ctx, Some(with)).await.unwrap();
    {
        let outputs = ctx.state.outputs.lock().unwrap();
        assert_eq!(outputs["version"], "0.1.0");
        assert_eq!(outputs["tag"], "v0.1.0");
        assert_eq!(outputs["changed"], "true");
    }
    assert!(!ctx.git().has_staged_changes().await.unwrap());
}

#[tokio::test]
async fn first_release_without_staged_changes_cannot_commit() {
    let temp = tempfile::TempDir::new().unwrap();
    init_repo(temp.path());
    std::fs::write(temp.path().join("VERSION"), "0.1.0\n").unwrap();
    git(temp.path(), &["add", "."]);
    git(temp.path(), &["commit", "-qm", "feat: initial"]);
    let mut ctx = context(temp.path());
    ctx.policy = Arc::new(evin_repository_policy::RepositoryPolicy::default());
    let with = evin_flow::parse_yaml(
            "files:\n  - path: VERSION\n    match: '(?m)^${previous_version}$'\n    write: '${version}'\ngit:\n  commit: true\n  tag: true\n  message: 'release: ${version}'\n",
        )
        .unwrap();
    let error = execute(&ctx, Some(with)).await.unwrap_err();
    assert_eq!(error.code.key, "no-staged-changes");
    let expected = evin_locales::t!("no-staged-changes");
    assert!(!expected.starts_with("<missing locale key:"));
    assert_eq!(evin_locales::render_issue(&error.code), expected);
    assert!(!ctx.git().has_tag("v0.1.0").await.unwrap());
}

#[tokio::test]
async fn first_release_still_replaces_zero_previous_version_and_can_defer() {
    let temp = tempfile::TempDir::new().unwrap();
    init_repo(temp.path());
    std::fs::write(temp.path().join("VERSION"), "0.0.0\n").unwrap();
    git(temp.path(), &["add", "."]);
    git(temp.path(), &["commit", "-qm", "feat: initial"]);
    let mut ctx = context(temp.path());
    ctx.policy = Arc::new(evin_repository_policy::RepositoryPolicy::default());
    let with = evin_flow::parse_yaml(
            "files:\n  - path: VERSION\n    match: '(?m)^${previous_version}$'\n    write: '${version}'\ngit:\n  commit: true\n  tag: true\n  defer: true\n  message: 'release: ${version}'\n",
        )
        .unwrap();
    execute(&ctx, Some(with)).await.unwrap();
    assert_eq!(
        std::fs::read_to_string(temp.path().join("VERSION")).unwrap(),
        "0.1.0\n"
    );
    assert!(ctx.git().has_staged_changes().await.unwrap());
}

#[tokio::test]
async fn first_release_with_no_edits_can_defer() {
    let temp = tempfile::TempDir::new().unwrap();
    init_repo(temp.path());
    std::fs::write(temp.path().join("VERSION"), "0.1.0\n").unwrap();
    git(temp.path(), &["add", "."]);
    git(temp.path(), &["commit", "-qm", "feat: initial"]);
    let mut ctx = context(temp.path());
    ctx.policy = Arc::new(evin_repository_policy::RepositoryPolicy::default());
    let with = evin_flow::parse_yaml(
            "files:\n  - path: VERSION\n    match: '(?m)^${previous_version}$'\n    write: '${version}'\ngit:\n  commit: true\n  tag: true\n  defer: true\n  message: 'release: ${version}'\n",
        )
        .unwrap();
    execute(&ctx, Some(with)).await.unwrap();
    assert_eq!(ctx.state.outputs.lock().unwrap()["changed"], "true");
    assert!(!ctx.git().has_staged_changes().await.unwrap());
}

#[tokio::test]
async fn tagged_pre_one_releases_use_commit_impacts() {
    use evin_repository_policy::VersioningStrategy::{Cargo, Semver};
    for (strategy, subject, expected) in [
        (Cargo, "fix: correct an issue", "0.1.1"),
        (Cargo, "feat: add a capability", "0.1.1"),
        (Cargo, "feat!: break compatibility", "0.2.0"),
        (Semver, "feat: add a capability", "0.2.0"),
        (Semver, "feat!: break compatibility", "1.0.0"),
    ] {
        let temp = tempfile::TempDir::new().unwrap();
        init_repo(temp.path());
        std::fs::write(temp.path().join("VERSION"), "0.1.0\n").unwrap();
        git(temp.path(), &["add", "."]);
        git(temp.path(), &["commit", "-qm", "feat: initial"]);
        git(temp.path(), &["tag", "v0.1.0"]);
        git(temp.path(), &["commit", "--allow-empty", "-qm", subject]);
        let mut ctx = context(temp.path());
        let mut rules = evin_repository_policy::RepositoryPolicy::default();
        rules.releases.as_mut().unwrap().versioning.strategy = strategy;
        ctx.policy = Arc::new(rules);
        let with = evin_flow::parse_yaml(
                "files:\n  - path: VERSION\n    match: '(?m)^${previous_version}$'\n    write: '${version}'\n"
            ).unwrap();
        execute(&ctx, Some(with)).await.unwrap();
        assert_eq!(ctx.state.outputs.lock().unwrap()["version"], expected);
        assert_eq!(
            std::fs::read_to_string(temp.path().join("VERSION")).unwrap(),
            format!("{expected}\n")
        );
    }
}

#[tokio::test]
async fn proposal_recalculation_uses_last_tag_when_main_adds_a_breaking_change() {
    let temp = tempfile::TempDir::new().unwrap();
    init_repo(temp.path());
    std::fs::write(temp.path().join("VERSION"), "0.1.0\r\n").unwrap();
    git(temp.path(), &["add", "VERSION"]);
    git(temp.path(), &["commit", "-qm", "feat: initial"]);
    git(temp.path(), &["branch", "-M", "main"]);
    git(temp.path(), &["tag", "v0.1.0"]);
    git(
        temp.path(),
        &["commit", "--allow-empty", "-qm", "fix: first change"],
    );
    let rules = evin_repository_policy::parse_str(
        r#"{"$schema":"https://dl.everyinit.dev/schema/v1/policy","releases":{"versioning":{"strategy":"cargo"}}}"#,
        "json",
        None,
    )
    .unwrap();
    let config = "files:\n  - path: VERSION\n    match: '(?m)^${previous_version}$'\n    write: '${version}'\n";
    git(temp.path(), &["switch", "-qc", "automation/release"]);
    let mut first = context(temp.path());
    first.policy = Arc::new(rules.clone());
    execute(&first, Some(evin_flow::parse_yaml(config).unwrap()))
        .await
        .unwrap();
    assert_eq!(first.state.outputs.lock().unwrap()["version"], "0.1.1");
    git(temp.path(), &["add", "VERSION"]);
    git(temp.path(), &["commit", "-qm", "chore(release): 0.1.1"]);

    git(temp.path(), &["switch", "-q", "main"]);
    git(
        temp.path(),
        &[
            "commit",
            "--allow-empty",
            "-qm",
            "feat!: break compatibility",
        ],
    );
    git(
        temp.path(),
        &["switch", "--force-create", "automation/release", "main"],
    );
    let mut updated = context(temp.path());
    updated.policy = Arc::new(rules);
    execute(&updated, Some(evin_flow::parse_yaml(config).unwrap()))
        .await
        .unwrap();
    assert_eq!(updated.state.outputs.lock().unwrap()["version"], "0.2.0");
    assert_eq!(
        std::fs::read_to_string(temp.path().join("VERSION")).unwrap(),
        "0.2.0\r\n"
    );
}

#[tokio::test]
async fn unmatched_replacements_fail_with_and_without_existing_tag() {
    for tagged in [false, true] {
        for automatic in [false, true] {
            let temp = tempfile::TempDir::new().unwrap();
            init_repo(temp.path());
            let (filename, files) = if automatic {
                ("Cargo.toml", "files:\n  - Cargo.toml\n")
            } else {
                (
                    "VERSION",
                    "files:\n  - path: VERSION\n    match: '(?m)^${previous_version}$'\n    write: '${version}'\n",
                )
            };
            std::fs::write(temp.path().join(filename), "unrelated\n").unwrap();
            git(temp.path(), &["add", "."]);
            git(temp.path(), &["commit", "-qm", "feat: initial"]);
            if tagged {
                git(temp.path(), &["tag", "v0.1.0"]);
                git(
                    temp.path(),
                    &["commit", "--allow-empty", "-qm", "feat: next"],
                );
            }
            let mut ctx = context(temp.path());
            ctx.policy = Arc::new(evin_repository_policy::RepositoryPolicy::default());
            let error = execute(&ctx, Some(evin_flow::parse_yaml(files).unwrap()))
                .await
                .unwrap_err();
            let expected = evin_locales::t!("version-bumper-replacement-missing", path = filename);
            assert!(!expected.starts_with("<missing locale key:"));
            assert_eq!(evin_locales::render_issue(&error.code), expected);
            assert_eq!(
                std::fs::read_to_string(temp.path().join(filename)).unwrap(),
                "unrelated\n"
            );
        }
    }
}

#[tokio::test]
async fn later_mismatch_does_not_write_earlier_replacement() {
    let temp = tempfile::TempDir::new().unwrap();
    init_repo(temp.path());
    std::fs::write(temp.path().join("Cargo.toml"), "version = \"0.0.0\"\n").unwrap();
    std::fs::write(temp.path().join("VERSION"), "unrelated\n").unwrap();
    git(temp.path(), &["add", "."]);
    git(temp.path(), &["commit", "-qm", "feat: initial"]);
    let mut ctx = context(temp.path());
    ctx.policy = Arc::new(evin_repository_policy::RepositoryPolicy::default());
    let with = evin_flow::parse_yaml(
            "files:\n  - Cargo.toml\n  - path: VERSION\n    match: '(?m)^${previous_version}$'\n    write: '${version}'\n",
        )
        .unwrap();
    let error = execute(&ctx, Some(with)).await.unwrap_err();
    assert_eq!(error.code.key, "version-bumper-replacement-missing");
    assert_eq!(
        evin_locales::render_issue(&error.code),
        evin_locales::t!("version-bumper-replacement-missing", path = "VERSION")
    );
    assert_eq!(
        std::fs::read_to_string(temp.path().join("Cargo.toml")).unwrap(),
        "version = \"0.0.0\"\n"
    );
    assert!(!ctx.git().has_staged_changes().await.unwrap());
}

#[tokio::test]
async fn repeated_file_replacements_are_validated_in_order() {
    let temp = tempfile::TempDir::new().unwrap();
    init_repo(temp.path());
    std::fs::write(temp.path().join("VERSION"), "old\n").unwrap();
    git(temp.path(), &["add", "."]);
    git(temp.path(), &["commit", "-qm", "feat: initial"]);
    let mut ctx = context(temp.path());
    ctx.policy = Arc::new(evin_repository_policy::RepositoryPolicy::default());
    let with = evin_flow::parse_yaml(
            "files:\n  - path: VERSION\n    match: old\n    write: intermediate\n  - path: ./VERSION\n    match: intermediate\n    write: '${version}'\n",
        )
        .unwrap();
    execute(&ctx, Some(with)).await.unwrap();
    assert_eq!(
        std::fs::read_to_string(temp.path().join("VERSION")).unwrap(),
        "0.1.0\n"
    );
}

#[tokio::test]
async fn tagged_release_does_not_fall_back_to_new_version() {
    let temp = tempfile::TempDir::new().unwrap();
    init_repo(temp.path());
    std::fs::write(temp.path().join("VERSION"), "0.1.1\n").unwrap();
    git(temp.path(), &["add", "."]);
    git(temp.path(), &["commit", "-qm", "feat: initial"]);
    git(temp.path(), &["tag", "v0.1.0"]);
    git(
        temp.path(),
        &["commit", "--allow-empty", "-qm", "feat: next"],
    );
    let mut ctx = context(temp.path());
    let mut rules = evin_repository_policy::RepositoryPolicy::default();
    rules.releases.as_mut().unwrap().versioning.strategy =
        evin_repository_policy::VersioningStrategy::Cargo;
    ctx.policy = Arc::new(rules);
    let with = evin_flow::parse_yaml(
            "files:\n  - path: VERSION\n    match: '(?m)^${previous_version}$'\n    write: '${version}'\n",
        )
        .unwrap();
    let error = execute(&ctx, Some(with)).await.unwrap_err();
    assert_eq!(error.code.key, "version-bumper-replacement-missing");
    assert_eq!(
        evin_locales::render_issue(&error.code),
        evin_locales::t!("version-bumper-replacement-missing", path = "VERSION")
    );
}

#[tokio::test]
async fn calculated_existing_tag_fails_before_edits() {
    let temp = tempfile::TempDir::new().unwrap();
    init_repo(temp.path());
    std::fs::write(temp.path().join("VERSION"), "0.1.0\n").unwrap();
    git(temp.path(), &["add", "."]);
    git(temp.path(), &["commit", "-qm", "feat: initial"]);
    git(temp.path(), &["tag", "v0.1.1"]);
    git(
        temp.path(),
        &["commit", "--allow-empty", "-qm", "chore: previous release"],
    );
    git(temp.path(), &["tag", "v0.1.0"]);
    git(
        temp.path(),
        &["commit", "--allow-empty", "-qm", "feat: next"],
    );
    let mut ctx = context(temp.path());
    let mut rules = evin_repository_policy::RepositoryPolicy::default();
    rules.releases.as_mut().unwrap().versioning.strategy =
        evin_repository_policy::VersioningStrategy::Cargo;
    ctx.policy = Arc::new(rules);
    let with = evin_flow::parse_yaml(
            "files:\n  - path: VERSION\n    match: '(?m)^${previous_version}$'\n    write: '${version}'\n",
        )
        .unwrap();
    let error = execute(&ctx, Some(with)).await.unwrap_err();
    assert_eq!(error.code.key, "calculated-tag-already-exists");
    assert_eq!(
        evin_locales::render_issue(&error.code),
        evin_locales::t!(
            "calculated-tag-already-exists",
            version = "0.1.1",
            tag = "v0.1.1"
        )
    );
    assert_eq!(
        std::fs::read_to_string(temp.path().join("VERSION")).unwrap(),
        "0.1.0\n"
    );
}

#[tokio::test]
async fn package_only_release_commits_lock_with_trailer_and_no_tag() {
    let temp = tempfile::TempDir::new().unwrap();
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "Test"][..],
        &["config", "user.email", "test@example.test"][..],
        &["config", "commit.gpgSign", "false"][..],
        &["config", "tag.gpgSign", "false"][..],
    ] {
        git(temp.path(), args);
    }
    std::fs::create_dir(temp.path().join("src")).unwrap();
    std::fs::write(temp.path().join("src/app"), "one\n").unwrap();
    git(temp.path(), &["add", "."]);
    git(temp.path(), &["commit", "-qm", "feat: initial"]);
    git(temp.path(), &["tag", "-a", "v1.0.0", "-m", "v1.0.0"]);
    std::fs::write(temp.path().join("src/app"), "two\n").unwrap();
    git(temp.path(), &["add", "src/app"]);
    git(temp.path(), &["commit", "-qm", "ci: rebuild package"]);

    let mut ctx = context(temp.path());
    let mut rules = evin_repository_policy::RepositoryPolicy::default();
    rules.releases.as_mut().unwrap().packages = BTreeMap::from([(
        "app".into(),
        PackageReleasePolicy {
            watch: vec!["src/**".into()],
            targets: vec!["deb".into()],
            revision: PackageRevisionPolicy::default(),
        },
    )]);
    ctx.policy = Arc::new(rules);
    let with = evin_flow::parse_yaml("initial_version: 1.0.0\ngit:\n  commit: true\n  tag: true\n")
        .unwrap();
    execute(&ctx, Some(with)).await.unwrap();

    let subject = std::process::Command::new("git")
        .args(["log", "-1", "--format=%s%n%b"])
        .current_dir(temp.path())
        .output()
        .unwrap();
    let message = String::from_utf8_lossy(&subject.stdout);
    assert!(message.contains(evin_action_sdk::release::PACKAGE_RELEASE_SUBJECT));
    assert!(message.contains(evin_action_sdk::release::PACKAGE_RELEASE_TRAILER));
    let tags = std::process::Command::new("git")
        .args(["tag", "--list"])
        .current_dir(temp.path())
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&tags.stdout).trim(), "v1.0.0");
    assert!(
        temp.path()
            .join(evin_action_sdk::release::RELEASE_LOCK_PATH)
            .exists()
    );
    assert!(ctx.state.outputs.lock().unwrap().contains_key("release"));
    assert_eq!(ctx.state.outputs.lock().unwrap()["changed"], "false");
}

#[tokio::test]
async fn reports_changed_false_without_qualifying_commits() {
    let temp = tempfile::TempDir::new().unwrap();
    init_repo(temp.path());
    std::fs::write(temp.path().join("file"), "one\n").unwrap();
    git(temp.path(), &["add", "."]);
    git(temp.path(), &["commit", "-qm", "feat: initial"]);
    git(temp.path(), &["tag", "-a", "v1.0.0", "-m", "v1.0.0"]);

    let mut ctx = context(temp.path());
    ctx.policy = Arc::new(evin_repository_policy::RepositoryPolicy::default());
    let with = evin_flow::parse_yaml("initial_version: 1.0.0\n").unwrap();
    execute(&ctx, Some(with)).await.unwrap();
    let outputs = ctx.state.outputs.lock().unwrap();
    assert_eq!(outputs["changed"], "false");
    assert_eq!(outputs["version"], "1.0.0");
}

#[tokio::test]
async fn require_change_failure_uses_fluent_message() {
    let temp = tempfile::TempDir::new().unwrap();
    init_repo(temp.path());
    std::fs::write(temp.path().join("file"), "one\n").unwrap();
    git(temp.path(), &["add", "."]);
    git(temp.path(), &["commit", "-qm", "feat: initial"]);
    git(temp.path(), &["tag", "-a", "v1.0.0", "-m", "v1.0.0"]);

    let ctx = context(temp.path());
    let with = evin_flow::parse_yaml("initial_version: 1.0.0\nrequire_change: true\n").unwrap();
    let error = execute(&ctx, Some(with)).await.unwrap_err();
    let expected = evin_locales::t!("no-version-change");
    assert_eq!(error.code.key, "no-version-change");
    assert_eq!(evin_locales::render_issue(&error.code), expected);
    assert_eq!(error.title, evin_locales::t!("no-version-change-title"));
}

#[tokio::test]
async fn unsafe_version_file_path_uses_fluent_message() {
    let temp = tempfile::TempDir::new().unwrap();
    init_repo(temp.path());
    std::fs::write(temp.path().join("file"), "initial\n").unwrap();
    git(temp.path(), &["add", "."]);
    git(temp.path(), &["commit", "-qm", "feat: initial"]);
    let ctx = context(temp.path());
    let path = "../Cargo.toml";
    let detail = ctx.fs.get_safe_physical_path(path).unwrap_err().to_string();
    let with = evin_flow::parse_yaml(&format!("files: ['{path}']\n")).unwrap();

    let error = execute(&ctx, Some(with)).await.unwrap_err();
    let expected = evin_locales::t!(
        "version-bumper-file-path-unusable",
        path = path,
        error = detail
    );
    assert_eq!(error.code.key, "version-bumper-file-path-unusable");
    assert_eq!(evin_locales::render_issue(&error.code), expected);
    assert_eq!(
        error.title,
        evin_locales::t!("version-bumper-file-path-unusable-title")
    );
}

#[tokio::test]
async fn dry_run_mutation_bypass_uses_fluent_uncertainty() {
    let temp = tempfile::TempDir::new().unwrap();
    init_repo(temp.path());
    let manifest = "[package]\nname = \"sample\"\nversion = \"0.1.0\"\n";
    std::fs::write(temp.path().join("Cargo.toml"), manifest).unwrap();
    git(temp.path(), &["add", "Cargo.toml"]);
    git(temp.path(), &["commit", "-qm", "feat: initial"]);
    let head_before = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(temp.path())
        .output()
        .unwrap()
        .stdout;
    let ctx = context_with_mode(temp.path(), PipelineMode::DryRun);
    let with =
        evin_flow::parse_yaml("files: [Cargo.toml]\ngit:\n  commit: true\n  tag: true\n").unwrap();

    execute(&ctx, Some(with)).await.unwrap();

    assert_eq!(
        ctx.uncertainty(),
        [evin_locales::t!("version-bumper-dry-run-skipped-mutations")]
    );
    assert_eq!(
        std::fs::read_to_string(temp.path().join("Cargo.toml")).unwrap(),
        manifest
    );
    let head_after = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(temp.path())
        .output()
        .unwrap()
        .stdout;
    assert_eq!(head_before, head_after);
}

#[tokio::test]
async fn reports_changed_true_for_a_qualifying_commit() {
    let temp = tempfile::TempDir::new().unwrap();
    init_repo(temp.path());
    std::fs::write(temp.path().join("file"), "one\n").unwrap();
    git(temp.path(), &["add", "."]);
    git(temp.path(), &["commit", "-qm", "feat: initial"]);
    git(temp.path(), &["tag", "-a", "v1.0.0", "-m", "v1.0.0"]);
    std::fs::write(temp.path().join("file"), "two\n").unwrap();
    git(temp.path(), &["commit", "-aqm", "feat: add a capability"]);

    let mut ctx = context(temp.path());
    ctx.policy = Arc::new(evin_repository_policy::RepositoryPolicy::default());
    let with = evin_flow::parse_yaml("initial_version: 1.0.0\n").unwrap();
    execute(&ctx, Some(with)).await.unwrap();
    let outputs = ctx.state.outputs.lock().unwrap();
    assert_eq!(outputs["changed"], "true");
    assert_eq!(outputs["version"], "1.1.0");
}

mod config;
