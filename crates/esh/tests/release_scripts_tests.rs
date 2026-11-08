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
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::tempdir;

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap()
        .to_path_buf()
}

async fn run_script(script: &str, cwd: &Path, args: &[&str]) -> (i32, String, String) {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(cwd).unwrap();
    let mut positional = vec![script.to_string()];
    positional.extend(args.iter().map(|arg| (*arg).to_string()));
    ctx.set_positional_args(positional);

    let code = engine
        .eval_file(
            &repository_root().join("scripts/release").join(script),
            &mut ctx,
        )
        .await
        .unwrap();
    (
        code,
        ctx.get_captured_stdout().unwrap(),
        ctx.get_captured_stderr().unwrap(),
    )
}

#[tokio::test]
async fn verify_changelog_requires_the_requested_generated_section() {
    let temp = tempdir().unwrap();
    fs::write(
        temp.path().join("CHANGELOG.md"),
        "# Changelog\n\n## [v1.2.3]\n",
    )
    .unwrap();

    let (code, output, error) = run_script(
        "verify-changelog.esh",
        temp.path(),
        &["CHANGELOG.md", "1.2.3"],
    )
    .await;
    assert_eq!(code, 0, "{error}");
    assert!(output.contains("Verified generated changelog section for v1.2.3"));

    let (code, _, error) = run_script(
        "verify-changelog.esh",
        temp.path(),
        &["CHANGELOG.md", "1.2.4"],
    )
    .await;
    assert_eq!(code, 1);
    assert!(error.contains("missing the generated section for v1.2.4"));
}

#[tokio::test]
async fn workspace_version_requires_matching_release_versions() {
    let temp = tempdir().unwrap();
    fs::create_dir_all(temp.path().join(".evin")).unwrap();
    fs::create_dir_all(temp.path().join("editors/vscode")).unwrap();
    fs::write(temp.path().join("Cargo.toml"), "version = \"1.2.3\"\n").unwrap();
    fs::write(
        temp.path().join(".evin/workspace.flow"),
        "workspace:\n  version: \"1.2.3\"\n",
    )
    .unwrap();
    fs::write(
        temp.path().join("editors/vscode/package.json"),
        "{\"version\": \"1.2.3\"}",
    )
    .unwrap();

    let (code, output, error) = run_script("workspace-version.esh", temp.path(), &[]).await;
    assert_eq!(code, 0, "{error}");
    assert_eq!(output, "1.2.3\n");
}

#[tokio::test]
async fn stage_candidate_artifacts_finds_archives_recursively() {
    let temp = tempdir().unwrap();
    let artifacts = temp.path().join("artifacts");
    fs::create_dir_all(artifacts.join("linux/evin")).unwrap();
    fs::create_dir_all(artifacts.join("linux/esh")).unwrap();
    fs::create_dir_all(temp.path().join("crates/evin/dist/github")).unwrap();
    fs::create_dir_all(temp.path().join("crates/esh/dist/github")).unwrap();
    fs::write(
        artifacts.join("linux/evin/evin-v1.2.3-x86_64-unknown-linux-musl.tar.gz"),
        "evin archive",
    )
    .unwrap();
    fs::write(
        artifacts.join("linux/esh/esh-v1.2.3-x86_64-unknown-linux-musl.tar.gz"),
        "esh archive",
    )
    .unwrap();

    let artifact_arg = artifacts.to_string_lossy().to_string();
    let (code, _, error) = run_script(
        "stage-candidate-artifacts.esh",
        temp.path(),
        &[&artifact_arg, "1.2.3"],
    )
    .await;
    assert_eq!(code, 0, "{error}");
    assert_eq!(
        fs::read_to_string(
            temp.path()
                .join("crates/evin/dist/github/evin-v1.2.3-x86_64-unknown-linux-musl.tar.gz"),
        )
        .unwrap(),
        "evin archive"
    );
    assert_eq!(
        fs::read_to_string(
            temp.path()
                .join("crates/esh/dist/github/esh-v1.2.3-x86_64-unknown-linux-musl.tar.gz"),
        )
        .unwrap(),
        "esh archive"
    );
}
