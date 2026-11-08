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

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use esh::{Engine, ExecutionContext};
use ring::{
    rand::SystemRandom,
    signature::{Ed25519KeyPair, KeyPair},
};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
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

fn git(cwd: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

#[tokio::test]
async fn find_merged_candidate_reports_a_single_new_release_tag() {
    let temp = tempdir().unwrap();
    git(temp.path(), &["init", "--quiet"]);
    git(
        temp.path(),
        &["config", "user.email", "release@example.com"],
    );
    git(temp.path(), &["config", "user.name", "Release Test"]);

    fs::write(temp.path().join("release.txt"), "before\n").unwrap();
    git(temp.path(), &["add", "release.txt"]);
    git(temp.path(), &["commit", "--quiet", "-m", "before release"]);
    let before = git(temp.path(), &["rev-parse", "HEAD"]);

    fs::write(temp.path().join("release.txt"), "candidate\n").unwrap();
    git(
        temp.path(),
        &["commit", "--quiet", "-am", "release candidate"],
    );
    git(temp.path(), &["tag", "v1.2.3"]);
    let after = git(temp.path(), &["rev-parse", "HEAD"]);

    let (code, output, error) =
        run_script("find-merged-candidate.esh", temp.path(), &[&before, &after]).await;
    assert_eq!(code, 0, "{error}");
    assert_eq!(
        output,
        format!(
            "{{\"found\":true,\"tag\":\"v1.2.3\",\"version\":\"1.2.3\",\"commit\":\"{after}\"}}\n"
        )
    );
}

const RELEASE_VERSION: &str = "0.2.2";
const RELEASE_TARGET: &str = "x86_64-unknown-linux-musl";

fn release_artifacts() -> Vec<(String, Vec<u8>)> {
    vec![
        (
            format!("evin-v{RELEASE_VERSION}-{RELEASE_TARGET}.tar.gz"),
            vec![0x1f, 0x8b],
        ),
        (
            format!("esh-v{RELEASE_VERSION}-{RELEASE_TARGET}.tar.gz"),
            vec![0x1f, 0x8b],
        ),
        (
            format!("evin_{RELEASE_VERSION}-2_amd64.deb"),
            b"!<arch>\n".to_vec(),
        ),
        (
            format!("esh_{RELEASE_VERSION}-3_amd64.deb"),
            b"!<arch>\n".to_vec(),
        ),
        (
            format!("evin-{RELEASE_VERSION}-4-x86_64.pkg.tar.zst"),
            vec![0x28, 0xb5, 0x2f, 0xfd],
        ),
        (
            format!("esh-{RELEASE_VERSION}-5-x86_64.pkg.tar.zst"),
            vec![0x28, 0xb5, 0x2f, 0xfd],
        ),
        (
            format!("everyinit-{RELEASE_VERSION}.vsix"),
            vec![0x50, 0x4b, 0x03, 0x04],
        ),
    ]
}

fn ed25519_public_key_pem(public_key: &[u8]) -> String {
    let mut subject_public_key_info = vec![
        0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
    ];
    subject_public_key_info.extend_from_slice(public_key);
    format!(
        "-----BEGIN PUBLIC KEY-----\n{}\n-----END PUBLIC KEY-----\n",
        BASE64.encode(subject_public_key_info)
    )
}

fn release_fixture(root: &Path, artifacts: &[(String, Vec<u8>)]) -> PathBuf {
    let directory = root.join("target/release-archives");
    fs::create_dir_all(&directory).unwrap();

    let manifest = artifacts
        .iter()
        .map(|(name, content)| format!("{:x}  {name}\n", Sha256::digest(content)))
        .collect::<String>();
    let key_pair = Ed25519KeyPair::from_pkcs8(
        Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
            .unwrap()
            .as_ref(),
    )
    .unwrap();
    let public_key = ed25519_public_key_pem(key_pair.public_key().as_ref());

    for (name, content) in artifacts {
        fs::write(directory.join(name), content).unwrap();
    }
    fs::write(directory.join("SHA256SUMS"), &manifest).unwrap();
    fs::write(
        directory.join("SHA256SUMS.sig"),
        key_pair.sign(manifest.as_bytes()).as_ref(),
    )
    .unwrap();
    fs::write(directory.join("release-key.pub"), &public_key).unwrap();
    fs::create_dir_all(root.join("infra/get")).unwrap();
    fs::write(root.join("infra/get/release-key.pub"), public_key).unwrap();
    directory
}

async fn verify_release_packages(root: &Path, directory: &Path) -> (i32, String, String) {
    let directory = directory.to_str().unwrap();
    run_script(
        "verify-release-packages.esh",
        root,
        &[directory, RELEASE_VERSION, RELEASE_TARGET],
    )
    .await
}

#[tokio::test]
async fn verify_release_packages_accepts_signed_packages_with_independent_revisions() {
    let temp = tempdir().unwrap();
    let directory = release_fixture(temp.path(), &release_artifacts());

    let (code, output, error) = verify_release_packages(temp.path(), &directory).await;
    assert_eq!(code, 0, "{error}");
    assert!(output.contains("Verified all 10 release artifacts"));
}

#[tokio::test]
async fn verify_release_packages_accepts_revision_one_packages() {
    let temp = tempdir().unwrap();
    let artifacts = release_artifacts()
        .into_iter()
        .map(|(name, content)| {
            (
                name.replace("-2_amd64.deb", "-1_amd64.deb")
                    .replace("-3_amd64.deb", "-1_amd64.deb")
                    .replace("-4-x86_64.pkg.tar.zst", "-1-x86_64.pkg.tar.zst")
                    .replace("-5-x86_64.pkg.tar.zst", "-1-x86_64.pkg.tar.zst"),
                content,
            )
        })
        .collect::<Vec<_>>();
    let directory = release_fixture(temp.path(), &artifacts);

    let (code, _, error) = verify_release_packages(temp.path(), &directory).await;
    assert_eq!(code, 0, "{error}");
}

#[tokio::test]
async fn verify_release_packages_rejects_ambiguous_package_revisions() {
    let temp = tempdir().unwrap();
    let directory = release_fixture(temp.path(), &release_artifacts());
    fs::write(
        directory.join(format!("evin_{RELEASE_VERSION}-1_amd64.deb")),
        b"!<arch>\n",
    )
    .unwrap();

    let (code, output, error) = verify_release_packages(temp.path(), &directory).await;
    assert_ne!(code, 0);
    assert!(
        error.contains("expected exactly one evin deb package"),
        "{output}{error}"
    );
}

#[tokio::test]
async fn verify_release_packages_rejects_missing_or_invalid_package_revisions() {
    let temp = tempdir().unwrap();
    let directory = release_fixture(temp.path(), &release_artifacts());
    fs::remove_file(directory.join(format!("evin_{RELEASE_VERSION}-2_amd64.deb"))).unwrap();
    fs::write(
        directory.join(format!("evin_{RELEASE_VERSION}-0_amd64.deb")),
        b"!<arch>\n",
    )
    .unwrap();

    let (code, output, error) = verify_release_packages(temp.path(), &directory).await;
    assert_ne!(code, 0);
    assert!(
        error.contains("evin deb package must use a positive revision"),
        "{output}{error}"
    );
}

#[tokio::test]
async fn verify_release_packages_checks_resolved_package_checksums() {
    let temp = tempdir().unwrap();
    let directory = release_fixture(temp.path(), &release_artifacts());
    fs::write(
        directory.join(format!("evin_{RELEASE_VERSION}-2_amd64.deb")),
        b"!<arch>\nx",
    )
    .unwrap();

    let (code, output, error) = verify_release_packages(temp.path(), &directory).await;
    assert_ne!(code, 0);
    assert!(output.contains("evin_0.2.2-2_amd64.deb: FAILED"), "{error}");
}

#[tokio::test]
async fn verify_release_packages_rejects_damaged_signatures() {
    let temp = tempdir().unwrap();
    let directory = release_fixture(temp.path(), &release_artifacts());
    fs::write(directory.join("SHA256SUMS.sig"), [0_u8; 64]).unwrap();

    let (code, _, error) = verify_release_packages(temp.path(), &directory).await;
    assert_ne!(code, 0);
    assert!(error.contains("signature verification failed"));
}
