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

use esh::{Engine, ExecutionContext};
use serde_yaml::Value;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::sync::atomic::{AtomicUsize, Ordering};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn flow(path: &str) -> Value {
    serde_yaml::from_str(&fs::read_to_string(root().join(path)).unwrap()).unwrap()
}

fn workflow_step_run(name: &str) -> String {
    flow(".github/workflows/release.yml")["jobs"]["release"]["steps"]
        .as_sequence()
        .unwrap()
        .iter()
        .find(|step| step["name"].as_str() == Some(name))
        .unwrap()["run"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn step(name: &str) -> String {
    let run = workflow_step_run(name);
    let trimmed = run.trim();
    if let Some(task) = trimmed
        .strip_prefix("cargo evin-dev run --verbose release:")
        .or_else(|| trimmed.strip_prefix("cargo evin-dev run release:"))
    {
        let (flow_path, task_name) = match task.split_once(':') {
            Some((namespace, task_name)) => (format!(".evin/release/{namespace}.flow"), task_name),
            None => (".evin/release.flow".to_owned(), task),
        };
        let task_run = flow(&flow_path)["tasks"][task_name]["steps"][0]["run"]
            .as_str()
            .unwrap()
            .to_owned();
        if let Some(script_path) = task_run
            .trim()
            .strip_prefix("cargo run --locked -p esh -- ")
        {
            let script_path = script_path.split_whitespace().next().unwrap();
            return fs::read_to_string(root().join(script_path)).unwrap();
        }
        return task_run;
    }
    run
}

#[test]
fn automation_only_publishes_merged_proposals_after_ci() {
    let workflow = flow(".github/workflows/release.yml");
    assert!(workflow["on"]["push"].is_null());
    assert_eq!(
        workflow["on"]["workflow_run"]["workflows"][0].as_str(),
        Some("CI")
    );
    assert!(!workflow["on"]["workflow_dispatch"].is_null());
    assert_eq!(
        workflow["concurrency"]["cancel-in-progress"].as_bool(),
        Some(false)
    );
    assert!(workflow["permissions"].as_mapping().unwrap().is_empty());
    assert_eq!(
        workflow["jobs"]["release"]["env"]["GH_TOKEN"].as_str(),
        Some("${{ secrets.RELEASE_BOT_TOKEN }}")
    );
    assert_eq!(
        workflow["jobs"]["release"]["permissions"]["id-token"].as_str(),
        Some("write")
    );
    assert_eq!(
        workflow["jobs"]["release"]["permissions"]["packages"].as_str(),
        Some("write")
    );
    let steps = workflow["jobs"]["release"]["steps"].as_sequence().unwrap();
    let checkout = steps
        .iter()
        .find(|s| s["uses"].as_str() == Some("actions/checkout@v7"))
        .unwrap();
    assert_eq!(
        checkout["with"]["token"].as_str(),
        Some("${{ secrets.RELEASE_BOT_TOKEN }}")
    );
    let publisher_config = workflow_step_run("Load the current release publisher configuration");
    assert!(publisher_config.contains(".evin/release.flow"));
    let pos = |name| {
        steps
            .iter()
            .position(|s| s["name"].as_str() == Some(name))
            .unwrap()
    };
    assert!(pos("Require release bot token") < pos("Require exact successful main push CI"));
    assert!(pos("Require exact successful main push CI") < pos("Identify merged release proposal"));
    assert!(
        pos("Identify merged release proposal") < pos("Extract merged release notes with Evin")
    );
    assert!(
        pos("Extract merged release notes with Evin")
            < pos("Build and verify release archives with Evin")
    );
    assert!(
        pos("Build and verify release archives with Evin")
            < pos("Tag and publish the merged release")
    );
    assert!(
        pos("Tag and publish the merged release")
            < pos("Publish the npm package to both registries")
    );
    assert!(
        pos("Publish the npm package to both registries")
            < pos("Require merged releases to be published before proposing")
    );
    assert!(
        pos("Require merged releases to be published before proposing")
            < pos("Maintain the release pull request")
    );
    let publication = &steps[pos("Tag and publish the merged release")];
    assert_eq!(
        publication["if"].as_str(),
        Some("steps.proposal.outputs.merged == '1'")
    );
    for (step_name, expected_task) in [
        (
            "Require exact successful main push CI",
            "release:ci:require",
        ),
        ("Identify merged release proposal", "release:ci:proposal"),
        ("Determine release version", "release:ci:state"),
        ("Extract merged release notes with Evin", "release:notes"),
        (
            "Build and verify release archives with Evin",
            "release:archives-verify",
        ),
        ("Tag and publish the merged release", "release:pub:merged"),
        (
            "Publish the npm package to both registries",
            "release:pub:registries",
        ),
        (
            "Require merged releases to be published before proposing",
            "release:ci:audit",
        ),
        ("Maintain the release pull request", "release:ci:reconcile"),
    ] {
        assert_eq!(
            workflow_step_run(step_name),
            format!("cargo evin-dev run --verbose {expected_task}")
        );
    }
    let identity = step("Identify merged release proposal");
    for value in [
        ".merge_commit_sha == env.RELEASE_COMMIT",
        ".head.repo.full_name == env.GITHUB_REPOSITORY",
        ".merged_at != null",
    ] {
        assert!(identity.contains(value));
    }
    let proposal = fs::read_to_string(root().join(".evin/release/proposal.flow")).unwrap();
    let artifacts = fs::read_to_string(root().join(".evin/release/artifacts.flow")).unwrap();
    let tasks = flow(".evin/release/proposal.flow");
    let bump = &tasks["tasks"]["bump"]["steps"][0]["with"];
    assert_eq!(bump["initial_version"].as_str(), Some("0.1.0"));
    assert!(bump["cargo_pre_1_0"].is_null());
    assert_eq!(
        flow(".evin/gitrules.flow")["releases"]["versioning"]["strategy"].as_str(),
        Some("cargo")
    );
    let propose = &tasks["tasks"]["propose"];
    let git = &propose["steps"]
        .as_sequence()
        .unwrap()
        .iter()
        .find(|s| s["uses"].as_str() == Some("evin/git-release@v1"))
        .unwrap()["with"];
    assert!(git["tag"].is_null(), "never tag an unmerged proposal");
    assert_eq!(git["enabled"].as_str(), Some("${release:bump.changed}"));
    assert!(proposal.contains("evin/review-request@v1"));
    assert!(artifacts.contains("evin/archive@v1"));
    let notes = &flow(".evin/release/artifacts.flow")["tasks"]["notes"]["steps"][0]["with"];
    assert_eq!(notes["section"].as_str(), Some("${env.RELEASE_TAG}"));
    assert_eq!(
        notes["notes_path"].as_str(),
        Some("target/release-notes.md")
    );
    let publish = step("Tag and publish the merged release");
    assert!(publish.contains("--notes-file target/release-notes.md"));
    assert!(!publish.contains("--generate-notes"));
    let yaml = fs::read_to_string(root().join(".github/workflows/release.yml")).unwrap();
    assert!(!yaml.contains("github-script"));
    assert!(!yaml.contains("node "));
    assert!(yaml.contains("NPM_TOKEN: ${{ secrets.NPM_TOKEN }}"));
    assert!(yaml.contains("NODE_AUTH_TOKEN: ${{ github.token }}"));
    let npm_publisher = step("Publish the npm package to both registries");
    assert!(npm_publisher.contains("//registry.npmjs.org/:_authToken=${NPM_TOKEN}"));
    assert!(npm_publisher.contains("NPM_CONFIG_USERCONFIG=\"$npmrc\""));
    let ci = flow(".github/workflows/ci.yml");
    assert!(
        ci["on"]["pull_request"]["branches"]
            .as_sequence()
            .unwrap()
            .iter()
            .any(|branch| branch.as_str() == Some("main"))
    );
    assert_eq!(
        ci["jobs"]["validation"]["if"].as_str(),
        Some("github.event_name != 'schedule'")
    );
}

#[test]
fn release_package_producers_wait_for_archive_directory_cleanup() {
    let release = flow(".evin/release/artifacts.flow");
    let tasks = &release["tasks"];
    let sign_needs = tasks["sign"]["needs"].as_sequence().unwrap();

    for producer in ["archives", "native", "vscode"] {
        assert!(
            tasks[producer]["needs"]
                .as_sequence()
                .is_some_and(|needs| needs
                    .iter()
                    .any(|dependency| dependency.as_str() == Some("release:stage"))),
            "{producer} must wait until the archive directory is cleaned"
        );
        assert!(
            sign_needs
                .iter()
                .any(|dependency| dependency.as_str() == Some(&format!("release:{producer}"))),
            "signing must wait for {producer}"
        );
    }
}

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Repo {
    dir: PathBuf,
    work: PathBuf,
    sha: String,
}
impl Drop for Repo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}
impl Repo {
    fn git(&self, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(args)
            .current_dir(&self.work)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap().trim().to_owned()
    }
    fn new() -> Self {
        let dir = root().join("target").join(format!(
            "release-workflow-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(dir.join("bin")).unwrap();
        fs::create_dir_all(dir.join("work")).unwrap();
        let work = dir.join("work");
        fs::create_dir_all(work.join("target")).unwrap();
        fs::write(
            work.join("target/release-notes.md"),
            "### Fixes\n\n- reviewed fix\n",
        )
        .unwrap();
        let mut repo = Self {
            dir,
            work,
            sha: String::new(),
        };
        repo.git(&["init", "-q"]);
        repo.git(&["config", "user.name", "Test"]);
        repo.git(&["config", "user.email", "test@example.invalid"]);
        fs::write(repo.work.join("file"), "source").unwrap();
        fs::write(
            repo.work.join("Cargo.toml"),
            "[workspace.package]\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        repo.git(&["add", "file", "Cargo.toml"]);
        repo.git(&["commit", "-qm", "feat: source"]);
        repo.git(&["branch", "-M", "main"]);
        repo.sha = repo.git(&["rev-parse", "HEAD"]);
        let remote = repo.dir.join("remote.git");
        let out = Command::new("git")
            .args(["init", "--bare", "-q"])
            .arg(&remote)
            .output()
            .unwrap();
        assert!(out.status.success());
        repo.git(&["remote", "add", "origin", remote.to_str().unwrap()]);
        repo.git(&["push", "-q", "origin", "HEAD:main"]);
        let gh = repo.dir.join("bin/gh");
        fs::write(
            &gh,
            r##"#!/bin/sh
set -eu
if [ "$1" = api ]; then
  if [ -f "$GH_TEST_DIR/api-failure" ]; then exit 1; fi
  case "$*" in
    *actions/workflows*) cat "$GH_TEST_DIR/runs" ;;
    *actions/runs*) cat "$GH_TEST_DIR/jobs" ;;
    *commits*) jq -c '.[]' "$GH_TEST_DIR/pulls" ;;
    *pulls*) jq -c '.[]' "$GH_TEST_DIR/pulls" ;;
    *releases*) jq -c '.[]' "$GH_TEST_DIR/releases" ;;
    *) exit 2 ;;
  esac
elif [ "$1 $2" = "pr list" ]; then
  echo 1
elif [ "$1 $2" = "pr close" ]; then
  printf '%s\n' "$*" >> "$GH_TEST_DIR/calls"
elif [ "$1 $2" = "release create" ]; then
  printf '%s\n' "$*" >> "$GH_TEST_DIR/calls"
  while [ "$#" -gt 0 ]; do
    if [ "$1" = "--notes-file" ]; then
      cp "$2" "$GH_TEST_DIR/published-notes"
      exit 0
    fi
    shift
  done
  echo "Missing notes file" >&2
  exit 1
else
  printf '%s\n' "$*" >> "$GH_TEST_DIR/calls"
fi
"##,
        )
        .unwrap();
        fs::set_permissions(gh, fs::Permissions::from_mode(0o755)).unwrap();
        repo
    }
    fn run(&self, script: &str) -> std::process::Output {
        let path = format!(
            "{}:{}",
            self.dir.join("bin").display(),
            std::env::var("PATH").unwrap()
        );
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let engine = Engine::new();
            let mut ctx = ExecutionContext::new();
            ctx.enable_capture();
            ctx.set_cwd(&self.work).unwrap();
            ctx.set_env("PATH", path);
            ctx.set_env("GH_TEST_DIR", self.dir.to_string_lossy().as_ref());
            ctx.set_env("GITHUB_REPOSITORY", "owner/repo");
            ctx.set_env("RELEASE_COMMIT", &self.sha);
            ctx.set_env("RELEASE_BRANCH", "automation/release");
            ctx.set_env("RELEASE_TAG", "v0.1.0");
            ctx.set_env("RELEASE_TARGET", "x86_64-unknown-linux-musl");
            ctx.set_env(
                "GITHUB_OUTPUT",
                self.dir.join("output").to_string_lossy().as_ref(),
            );
            ctx.set_env(
                "GITHUB_ENV",
                self.dir.join("env").to_string_lossy().as_ref(),
            );
            let code = match engine.eval_str(script, &mut ctx).await {
                Ok(code) => code,
                Err(err) => {
                    let mut stderr = ctx.get_captured_stderr_bytes().unwrap_or_default();
                    stderr.extend_from_slice(format!("{err}\n").as_bytes());
                    return std::process::Output {
                        status: ExitStatus::from_raw(1 << 8),
                        stdout: ctx.get_captured_stdout_bytes().unwrap_or_default(),
                        stderr,
                    };
                }
            };
            std::process::Output {
                status: ExitStatus::from_raw((code & 0xff) << 8),
                stdout: ctx.get_captured_stdout_bytes().unwrap_or_default(),
                stderr: ctx.get_captured_stderr_bytes().unwrap_or_default(),
            }
        })
    }
}

#[test]
fn cli_version_bumper_uses_the_configured_git_rules_strategy() {
    for (strategy, expected) in [("cargo", "0.1.1\n"), ("semver", "0.2.0\n")] {
        let repo = Repo::new();
        fs::create_dir_all(repo.work.join(".evin")).unwrap();
        fs::write(
            repo.work.join(".evin/workspace.flow"),
            r#"#!evin/v1
workspace:
  name: versioning-test
  version: "0.1.0"
  rules: .evin/gitrules.flow
stages: [Release]
tasks:
  bump:
    stage: Release
    steps:
      - uses: evin/version-bumper@v1
        with:
          files:
            - path: VERSION
              match: '(?m)^${previous_version}$'
              write: '${version}'
"#,
        )
        .unwrap();
        fs::write(
            repo.work.join(".evin/gitrules.flow"),
            format!("#!evin/v1\nreleases:\n  versioning:\n    strategy: {strategy}\n"),
        )
        .unwrap();
        fs::write(repo.work.join("VERSION"), "0.1.0\n").unwrap();
        repo.git(&["add", ".evin", "VERSION"]);
        repo.git(&["commit", "-qm", "chore: configure versioning"]);
        repo.git(&["tag", "v0.1.0"]);
        repo.git(&["commit", "--allow-empty", "-qm", "feat: add capability"]);

        let output = Command::new(env!("CARGO_BIN_EXE_evin"))
            .current_dir(&repo.work)
            .args(["run", "bump"])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{strategy}: {} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            fs::read_to_string(repo.work.join("VERSION")).unwrap(),
            expected,
            "{strategy}"
        );
    }
}

#[test]
fn exact_sha_ci_gate_rejects_wrong_and_incomplete_runs() {
    let repo = Repo::new();
    let jobs = r#"{"name":"Linux validation (quality, tests, E2E, distribution)","status":"completed","conclusion":"success"}
{"name":"CI gate","status":"completed","conclusion":"success"}
"#;
    fs::write(repo.dir.join("jobs"), jobs).unwrap();
    let good = format!(
        r#"{{"id":1,"head_sha":"{}","head_branch":"main","event":"push","status":"completed","conclusion":"success"}}"#,
        repo.sha
    );
    for (name, runs, job_data, expected) in [
        ("success", good.clone(), jobs.to_owned(), true),
        ("missing", String::new(), jobs.to_owned(), false),
        (
            "wrong-sha",
            good.replace(&repo.sha, "wrong"),
            jobs.to_owned(),
            false,
        ),
        (
            "wrong-branch",
            good.replace("main", "release"),
            jobs.to_owned(),
            false,
        ),
        (
            "wrong-event",
            good.replace("push", "pull_request"),
            jobs.to_owned(),
            false,
        ),
        (
            "pending",
            good.replace("completed", "in_progress"),
            jobs.to_owned(),
            false,
        ),
        (
            "newer-failure",
            format!(
                "{good}\n{}",
                good.replace("\"id\":1", "\"id\":2")
                    .replace("success", "failure")
            ),
            jobs.to_owned(),
            false,
        ),
        (
            "missing-job",
            good.clone(),
            jobs.lines().next().unwrap().to_owned(),
            false,
        ),
        (
            "failed-job",
            good.clone(),
            jobs.replace("success", "failure"),
            false,
        ),
    ] {
        fs::write(repo.dir.join("runs"), runs).unwrap();
        fs::write(repo.dir.join("jobs"), job_data).unwrap();
        let output = repo.run(&step("Require exact successful main push CI"));
        assert_eq!(
            output.status.success(),
            expected,
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    fs::write(repo.dir.join("api-failure"), "").unwrap();
    assert!(
        !repo
            .run(&step("Require exact successful main push CI"))
            .status
            .success()
    );
}

#[test]
fn publication_requires_the_actual_same_repository_merge_commit() {
    let repo = Repo::new();
    let good = format!(
        r#"[{{"base":{{"ref":"main"}},"head":{{"ref":"automation/release","repo":{{"full_name":"owner/repo"}}}},"merged_at":"2026-01-01","merge_commit_sha":"{}"}}]"#,
        repo.sha
    );
    for (name, pulls, expected) in [
        ("merged", good.clone(), "merged=1"),
        ("ordinary", "[]".to_owned(), "merged=0"),
        ("fork", good.replace("owner/repo", "fork/repo"), "merged=0"),
        (
            "other-head",
            good.replace("automation/release", "other"),
            "merged=0",
        ),
        (
            "different-merge",
            good.replace(&repo.sha, "other"),
            "merged=0",
        ),
        (
            "not-merged",
            good.replace("\"2026-01-01\"", "null"),
            "merged=0",
        ),
    ] {
        fs::write(repo.dir.join("pulls"), pulls).unwrap();
        fs::write(repo.dir.join("output"), "").unwrap();
        let output = repo.run(&step("Identify merged release proposal"));
        assert!(
            output.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            fs::read_to_string(repo.dir.join("output")).unwrap().trim(),
            expected,
            "{name}"
        );
    }
}

#[test]
fn all_merge_methods_publish_main_sha_after_the_proposal_branch_is_deleted() {
    for method in ["merge", "squash", "rebase"] {
        let mut repo = Repo::new();
        repo.git(&["switch", "-qc", "automation/release"]);
        fs::write(repo.work.join("file"), "released source").unwrap();
        repo.git(&["add", "file"]);
        repo.git(&["commit", "-qm", "chore(release): 0.1.0"]);
        let proposal = repo.git(&["rev-parse", "HEAD"]);
        repo.git(&["switch", "-q", "main"]);
        repo.git(&["commit", "--allow-empty", "-qm", "fix: advance main"]);
        match method {
            "merge" => {
                repo.git(&[
                    "merge",
                    "--no-ff",
                    "-qm",
                    "merge release",
                    "automation/release",
                ]);
            }
            "squash" => {
                repo.git(&["merge", "--squash", "-q", "automation/release"]);
                repo.git(&["commit", "-qm", "chore(release): 0.1.0"]);
            }
            "rebase" => {
                repo.git(&["switch", "-q", "automation/release"]);
                repo.git(&["rebase", "-q", "main"]);
                repo.git(&["switch", "-q", "main"]);
                repo.git(&["merge", "--ff-only", "-q", "automation/release"]);
            }
            _ => unreachable!(),
        }
        repo.sha = repo.git(&["rev-parse", "HEAD"]);
        assert_ne!(repo.sha, proposal, "{method} must publish the main result");
        repo.git(&["push", "-q", "origin", "HEAD:main"]);
        repo.git(&["branch", "-D", "automation/release"]);
        let pull = format!(
            r#"[{{"base":{{"ref":"main"}},"head":{{"ref":"automation/release","sha":"{proposal}","repo":{{"full_name":"owner/repo"}}}},"merged_at":"2026-01-01","merge_commit_sha":"{}"}}]"#,
            repo.sha
        );
        fs::write(repo.dir.join("pulls"), pull).unwrap();
        fs::write(repo.dir.join("releases"), "[]").unwrap();
        let identity = repo.run(&step("Identify merged release proposal"));
        assert!(
            identity.status.success(),
            "{method}: {}",
            String::from_utf8_lossy(&identity.stderr)
        );
        assert_eq!(
            fs::read_to_string(repo.dir.join("output")).unwrap().trim(),
            "merged=1"
        );
        let publication = repo.run(&step("Tag and publish the merged release"));
        assert!(
            publication.status.success(),
            "{method}: {}",
            String::from_utf8_lossy(&publication.stderr)
        );
        assert_eq!(repo.git(&["rev-parse", "v0.1.0^{}"]), repo.sha, "{method}");
    }
}

#[test]
fn proposal_waits_for_merged_release_tag_and_complete_publication() {
    let mut repo = Repo::new();
    let script = step("Require merged releases to be published before proposing");
    let pull = format!(
        r#"[{{"base":{{"ref":"main"}},"head":{{"ref":"automation/release","repo":{{"full_name":"owner/repo"}}}},"merged_at":"2026-01-01","merge_commit_sha":"{}"}}]"#,
        repo.sha
    );
    fs::write(repo.dir.join("pulls"), "[]").unwrap();
    fs::write(repo.dir.join("releases"), "[]").unwrap();
    let initial = repo.run(&script);
    assert!(
        initial.status.success(),
        "initial release may be proposed: stdout={} stderr={}",
        String::from_utf8_lossy(&initial.stdout),
        String::from_utf8_lossy(&initial.stderr)
    );

    fs::write(repo.dir.join("pulls"), pull).unwrap();
    let pending = repo.run(&script);
    assert!(
        !pending.status.success(),
        "untagged merge must block proposals"
    );
    assert!(String::from_utf8_lossy(&pending.stderr).contains("not tagged correctly"));

    repo.git(&["tag", "-a", "v0.1.0", "-m", "release"]);
    repo.git(&["push", "-q", "origin", "refs/tags/v0.1.0"]);
    let missing_release = repo.run(&script);
    assert!(
        !missing_release.status.success(),
        "tag alone must not unblock proposals"
    );
    assert!(String::from_utf8_lossy(&missing_release.stderr).contains("not fully published"));
    let complete = r#"[{"tag_name":"v0.1.0","draft":false,"assets":[{"name":"evin-v0.1.0-x86_64-unknown-linux-musl.tar.gz"},{"name":"esh-v0.1.0-x86_64-unknown-linux-musl.tar.gz"}]}]"#;
    fs::write(
        repo.dir.join("releases"),
        complete.replace("\"draft\":false", "\"draft\":true"),
    )
    .unwrap();
    assert!(
        !repo.run(&script).status.success(),
        "draft release must block proposals"
    );
    fs::write(
        repo.dir.join("releases"),
        r#"[{"tag_name":"v0.1.0","draft":false,"assets":[{"name":"evin-v0.1.0-x86_64-unknown-linux-musl.tar.gz"}]}]"#,
    )
    .unwrap();
    assert!(
        !repo.run(&script).status.success(),
        "partial release must block proposals"
    );
    fs::write(repo.dir.join("releases"), complete).unwrap();
    assert!(
        repo.run(&script).status.success(),
        "published merge permits proposals"
    );

    repo.git(&["commit", "--allow-empty", "-qm", "fix: later"]);
    repo.git(&["push", "-q", "origin", "HEAD:main"]);
    fs::write(repo.dir.join("releases"), "[]").unwrap();
    assert!(
        repo.run(&script).status.success(),
        "stale runs skip proposal reconciliation"
    );
    repo.sha = repo.git(&["rev-parse", "HEAD"]);
    assert!(
        !repo.run(&script).status.success(),
        "a later main CI must still wait for the merged release"
    );
}

#[test]
fn proposal_is_closed_if_main_advances_during_reconciliation() {
    let repo = Repo::new();
    let cargo = repo.dir.join("bin/cargo");
    fs::write(
        &cargo,
        r#"#!/bin/sh
set -eu
if [ -f "$GH_TEST_DIR/advance-main" ]; then
  git commit --allow-empty -qm 'fix: concurrent main push'
  git push -q origin HEAD:main
fi
"#,
    )
    .unwrap();
    fs::set_permissions(cargo, fs::Permissions::from_mode(0o755)).unwrap();
    let script = step("Maintain the release pull request");
    assert!(repo.run(&script).status.success());
    fs::write(repo.dir.join("advance-main"), "").unwrap();
    let raced = repo.run(&script);
    assert!(!raced.status.success());
    assert!(
        String::from_utf8_lossy(&raced.stderr).contains("Main advanced during proposal"),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&raced.stdout),
        String::from_utf8_lossy(&raced.stderr)
    );
    let calls = fs::read_to_string(repo.dir.join("calls")).unwrap();
    assert!(calls.contains("pr close 1 --comment"));
}

#[test]
fn tag_and_publication_are_idempotent_and_reject_partial_or_conflicting_state() {
    let repo = Repo::new();
    let script = step("Tag and publish the merged release");
    fs::write(repo.dir.join("releases"), "[]").unwrap();
    let result = repo.run(&script);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(repo.git(&["rev-parse", "v0.1.0^{}"]), repo.sha);
    let calls = fs::read_to_string(repo.dir.join("calls")).unwrap();
    assert!(calls.contains("release create v0.1.0 --verify-tag --draft"));
    assert!(calls.contains("release edit v0.1.0 --draft=false"));
    assert_eq!(
        fs::read_to_string(repo.dir.join("published-notes")).unwrap(),
        "### Fixes\n\n- reviewed fix\n"
    );
    fs::write(repo.dir.join("calls"), "").unwrap();
    let complete = r#"[{"tag_name":"v0.1.0","draft":false,"assets":[{"name":"evin-v0.1.0-x86_64-unknown-linux-musl.tar.gz"},{"name":"esh-v0.1.0-x86_64-unknown-linux-musl.tar.gz"}]}]"#;
    fs::write(repo.dir.join("releases"), complete).unwrap();
    assert!(repo.run(&script).status.success());
    assert_eq!(fs::read_to_string(repo.dir.join("calls")).unwrap(), "");
    fs::write(
        repo.dir.join("releases"),
        complete.replace("\"draft\":false", "\"draft\":true"),
    )
    .unwrap();
    assert!(!repo.run(&script).status.success());
    fs::write(repo.dir.join("releases"), "[]").unwrap();
    repo.git(&["tag", "-d", "v0.1.0"]);
    repo.git(&["tag", "v0.1.0"]);
    assert!(
        !repo.run(&script).status.success(),
        "lightweight tag must be rejected"
    );
    repo.git(&["tag", "-d", "v0.1.0"]);
    repo.git(&["commit", "--allow-empty", "-qm", "fix: later"]);
    repo.git(&["tag", "-a", "v0.1.0", "-m", "wrong commit"]);
    assert!(
        !repo.run(&script).status.success(),
        "tag pointing to another commit must be rejected"
    );
}

#[test]
fn release_version_resolution_requires_matching_stable_workspace_version() {
    let repo = Repo::new();
    let script = step("Determine release version");
    fs::create_dir_all(repo.work.join(".evin")).unwrap();
    fs::create_dir_all(repo.work.join("editors/vscode")).unwrap();
    fs::write(
        repo.work.join(".evin/workspace.flow"),
        "workspace:\n  version: \"0.1.0\"\n",
    )
    .unwrap();
    fs::write(
        repo.work.join("editors/vscode/package.json"),
        "{\"version\": \"0.1.0\"}\n",
    )
    .unwrap();
    fs::write(repo.dir.join("env"), "").unwrap();
    fs::write(repo.dir.join("output"), "").unwrap();

    let ok = repo.run(&script);
    assert!(
        ok.status.success(),
        "{}",
        String::from_utf8_lossy(&ok.stderr)
    );
    assert_eq!(
        fs::read_to_string(repo.dir.join("env")).unwrap(),
        "RELEASE_VERSION=0.1.0\nRELEASE_TAG=v0.1.0\n"
    );
    assert_eq!(
        fs::read_to_string(repo.dir.join("output")).unwrap(),
        "version=0.1.0\ntag=v0.1.0\n"
    );

    fs::write(
        repo.work.join(".evin/workspace.flow"),
        "workspace:\n  version: \"0.2.0\"\n",
    )
    .unwrap();
    let mismatch = repo.run(&script);
    assert!(
        !mismatch.status.success(),
        "mismatched workspace version must be rejected"
    );

    fs::write(
        repo.work.join("Cargo.toml"),
        "[workspace.package]\nversion = \"0.1.0-alpha.1\"\n",
    )
    .unwrap();
    fs::write(
        repo.work.join(".evin/workspace.flow"),
        "workspace:\n  version: \"0.1.0-alpha.1\"\n",
    )
    .unwrap();
    fs::write(
        repo.work.join("editors/vscode/package.json"),
        "{\"version\": \"0.1.0-alpha.1\"}\n",
    )
    .unwrap();
    let prerelease = repo.run(&script);
    assert!(
        !prerelease.status.success(),
        "prerelease version must be rejected by stable release resolution"
    );
    assert!(String::from_utf8_lossy(&prerelease.stderr).contains("Invalid stable release version"));
}
