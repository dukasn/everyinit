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
    run_script_with_env(script, cwd, args, &[]).await
}

async fn run_script_with_env(
    script: &str,
    cwd: &Path,
    args: &[&str],
    env: &[(&str, &str)],
) -> (i32, String, String) {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(cwd).unwrap();
    for (key, value) in env {
        ctx.set_env(*key, *value);
    }
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
        "check/changelog.esh",
        temp.path(),
        &["CHANGELOG.md", "1.2.3"],
    )
    .await;
    assert_eq!(code, 0, "{error}");
    assert!(output.contains("Verified generated changelog section for v1.2.3"));

    let (code, _, error) = run_script(
        "check/changelog.esh",
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

    let (code, output, error) = run_script("check/workspace-version.esh", temp.path(), &[]).await;
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
        "candidate/stage-artifacts.esh",
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
        run_script("candidate/find-merged.esh", temp.path(), &[&before, &after]).await;
    assert_eq!(code, 0, "{error}");
    assert_eq!(
        output,
        format!(
            "{{\"found\":true,\"tag\":\"v1.2.3\",\"version\":\"1.2.3\",\"commit\":\"{after}\"}}\n"
        )
    );
}

#[tokio::test]
async fn validate_candidate_accepts_the_next_annotated_release_tag() {
    let temp = tempdir().unwrap();
    git(temp.path(), &["init", "--quiet"]);
    git(
        temp.path(),
        &["config", "user.email", "release@example.com"],
    );
    git(temp.path(), &["config", "user.name", "Release Test"]);
    fs::create_dir_all(temp.path().join(".evin")).unwrap();

    fs::write(temp.path().join("Cargo.toml"), "version = \"1.2.2\"\n").unwrap();
    fs::write(
        temp.path().join(".evin/workspace.flow"),
        "workspace:\n  version: \"1.2.2\"\n",
    )
    .unwrap();
    fs::write(
        temp.path().join("CHANGELOG.md"),
        "# Changelog\n\n## [v1.2.2]\n",
    )
    .unwrap();
    git(temp.path(), &["add", "."]);
    git(temp.path(), &["commit", "--quiet", "-m", "release 1.2.2"]);
    git(temp.path(), &["tag", "-a", "v1.2.2", "-m", "release 1.2.2"]);

    fs::write(temp.path().join("base.txt"), "base\n").unwrap();
    git(temp.path(), &["add", "base.txt"]);
    git(temp.path(), &["commit", "--quiet", "-m", "base"]);
    let base = git(temp.path(), &["rev-parse", "HEAD"]);

    fs::write(temp.path().join("Cargo.toml"), "version = \"1.2.3\"\n").unwrap();
    fs::write(
        temp.path().join(".evin/workspace.flow"),
        "workspace:\n  version: \"1.2.3\"\n",
    )
    .unwrap();
    fs::write(
        temp.path().join("CHANGELOG.md"),
        "# Changelog\n\n## [v1.2.3]\n",
    )
    .unwrap();
    git(temp.path(), &["add", "."]);
    git(temp.path(), &["commit", "--quiet", "-m", "release 1.2.3"]);
    git(temp.path(), &["tag", "-a", "v1.2.3", "-m", "release 1.2.3"]);
    let candidate = git(temp.path(), &["rev-parse", "HEAD"]);

    let (code, output, error) = run_script(
        "candidate/validate.esh",
        temp.path(),
        &["auto", "auto", &candidate, &base],
    )
    .await;
    assert_eq!(code, 0, "{error}");
    assert_eq!(
        output,
        format!(
            "{{\"tag\":\"v1.2.3\",\"version\":\"1.2.3\",\"commit\":\"{candidate}\",\"previous_tag\":\"v1.2.2\"}}\n"
        )
    );
}

#[tokio::test]
async fn seed_local_mirror_uploads_each_file_to_its_local_bucket() {
    let temp = tempdir().unwrap();
    let mirror = temp.path().join("mirror");
    fs::create_dir_all(mirror.join("evin/canary/v1.2.3")).unwrap();
    fs::create_dir_all(mirror.join("channels")).unwrap();
    fs::write(mirror.join("evin/canary/v1.2.3/release.json"), "release").unwrap();
    fs::write(mirror.join("channels/canary.json"), "channel").unwrap();

    let bin = temp.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let log = temp.path().join("corepack.log");
    let corepack = bin.join("corepack");
    fs::write(
        &corepack,
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$SEED_LOG\"\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&corepack, fs::Permissions::from_mode(0o755)).unwrap();
    }

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(temp.path()).unwrap();
    ctx.set_env(
        "PATH",
        format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
    );
    ctx.set_env("SEED_LOG", log.to_str().unwrap());
    let mirror_arg = mirror.to_str().unwrap();
    ctx.set_positional_args(vec!["seed-local.esh".to_string(), mirror_arg.to_string()]);

    let code = engine
        .eval_file(
            &repository_root().join("scripts/release/mirror/seed-local.esh"),
            &mut ctx,
        )
        .await
        .unwrap();
    assert_eq!(code, 0, "{}", ctx.get_captured_stderr().unwrap());
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "Seeded 2 objects into local R2 bucket everyinit-artifacts\n"
    );
    let uploads = fs::read_to_string(log).unwrap();
    assert!(uploads.contains("everyinit-artifacts/evin/canary/v1.2.3/release.json"));
    assert!(uploads.contains("everyinit-artifacts/channels/canary.json"));
    assert!(uploads.contains("--local --config infra/mirror/wrangler.toml"));
}

#[tokio::test]
async fn sign_packages_creates_a_sorted_ephemeral_manifest() {
    let temp = tempdir().unwrap();
    let packages = temp.path().join("packages");
    fs::create_dir_all(temp.path().join("infra/get")).unwrap();
    fs::create_dir(&packages).unwrap();
    fs::write(
        temp.path().join("infra/get/release-key.pub"),
        "placeholder\n",
    )
    .unwrap();
    fs::write(packages.join("everyinit-1.2.3.vsix"), b"vsix").unwrap();
    fs::write(
        packages.join("esh-v1.2.3-x86_64-unknown-linux-musl.tar.gz"),
        b"esh",
    )
    .unwrap();
    fs::write(
        packages.join("evin-v1.2.3-x86_64-unknown-linux-musl.tar.gz"),
        b"evin",
    )
    .unwrap();

    let (code, output, error) = run_script(
        "package/sign.esh",
        temp.path(),
        &[packages.to_str().unwrap(), "1.2.3"],
    )
    .await;
    assert_eq!(code, 0, "{error}");
    assert!(output.contains("for 3 artifacts"));
    let manifest = fs::read_to_string(packages.join("SHA256SUMS")).unwrap();
    let names = manifest
        .lines()
        .map(|line| line.split_whitespace().nth(1).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        [
            "esh-v1.2.3-x86_64-unknown-linux-musl.tar.gz",
            "everyinit-1.2.3.vsix",
            "evin-v1.2.3-x86_64-unknown-linux-musl.tar.gz",
        ]
    );
    assert_eq!(fs::read(packages.join("SHA256SUMS.sig")).unwrap().len(), 64);

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(temp.path()).unwrap();
    assert_eq!(
        engine
            .eval_str(
                "crypto --verify --pubkey packages/release-key.pub --sig packages/SHA256SUMS.sig packages/SHA256SUMS",
                &mut ctx,
            )
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn metadata_and_channel_manifest_publish_signed_release_pointers() {
    let temp = tempdir().unwrap();
    fs::create_dir_all(temp.path().join("infra/get/src")).unwrap();
    fs::create_dir_all(temp.path().join("archives")).unwrap();
    fs::write(
        temp.path()
            .join("archives/evin-v1.2.3-x86_64-unknown-linux-musl.tar.gz"),
        b"evin archive",
    )
    .unwrap();
    fs::write(
        temp.path()
            .join("archives/esh-v1.2.3-x86_64-unknown-linux-musl.tar.gz"),
        b"esh archive",
    )
    .unwrap();

    let engine = Engine::new();
    let mut key_context = ExecutionContext::new();
    key_context.set_cwd(temp.path()).unwrap();
    assert_eq!(
        engine
            .eval_str(
                "crypto --generate --private release-key.pem --public infra/get/release-key.pub",
                &mut key_context,
            )
            .await
            .unwrap(),
        0
    );
    fs::copy(
        temp.path().join("infra/get/release-key.pub"),
        temp.path().join("infra/get/src/installer.sh"),
    )
    .unwrap();

    let commit = "0123456789abcdef0123456789abcdef01234567";
    let key = temp.path().join("release-key.pem");
    let key = key.to_str().unwrap();
    for binary in ["evin", "esh"] {
        let (code, _, error) = run_script_with_env(
            "mirror/metadata.esh",
            temp.path(),
            &[
                binary,
                "canary",
                "1.2.3",
                "x86_64-unknown-linux-musl",
                "mirror",
                "archives",
            ],
            &[
                ("RELEASE_SIGNING_KEY_FILE", key),
                ("RELEASE_COMMIT", commit),
            ],
        )
        .await;
        assert_eq!(code, 0, "{error}");
    }

    let (code, output, error) = run_script_with_env(
        "mirror/channel-manifest.esh",
        temp.path(),
        &["canary", "1.2.3", "x86_64-unknown-linux-musl", "mirror"],
        &[("RELEASE_COMMIT", commit)],
    )
    .await;
    assert_eq!(code, 0, "{error}");
    assert!(output.contains("shared canary channel pointer"));
    let pointer = fs::read_to_string(temp.path().join("mirror/channels/canary.json")).unwrap();
    assert!(pointer.contains(commit));
    assert!(pointer.contains("https://dl.everyinit.dev/esh/canary/v1.2.3/release.json"));

    let mut verify_context = ExecutionContext::new();
    verify_context.set_cwd(temp.path()).unwrap();
    assert_eq!(
        engine
            .eval_str(
                "base64 --decode mirror/evin/canary/v1.2.3/SHA256SUMS.sig > signature\ncrypto --verify --pubkey infra/get/release-key.pub --sig signature mirror/evin/canary/v1.2.3/SHA256SUMS",
                &mut verify_context,
            )
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn publish_mirror_uploads_only_the_requested_release_objects() {
    let temp = tempdir().unwrap();
    let schema_directory = temp.path().join("schemas/v1");
    fs::create_dir_all(&schema_directory).unwrap();
    for name in ["policy", "environment"] {
        fs::copy(
            repository_root().join(format!("schemas/v1/{name}.schema.json")),
            schema_directory.join(format!("{name}.schema.json")),
        )
        .unwrap();
    }
    let mirror = temp.path().join("mirror");
    for binary in ["evin", "esh"] {
        let directory = mirror.join(format!("{binary}/canary/v1.2.3"));
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join(format!("{binary}-v1.2.3-x86_64-unknown-linux-musl.tar.gz")),
            b"archive",
        )
        .unwrap();
        fs::write(directory.join("SHA256SUMS"), "digest  archive\n").unwrap();
        fs::write(directory.join("SHA256SUMS.sig"), "signature\n").unwrap();
        fs::write(
            directory.join("release.json"),
            format!(
                "{{\"name\":\"{binary}\",\"version\":\"1.2.3\",\"channel\":\"canary\",\"git_commit\":\"0123456789abcdef0123456789abcdef01234567\"}}\n"
            ),
        )
        .unwrap();
    }
    fs::create_dir_all(mirror.join("channels")).unwrap();
    fs::write(
        mirror.join("channels/canary.json"),
        "{\"channel\":\"canary\",\"version\":\"1.2.3\",\"git_commit\":\"0123456789abcdef0123456789abcdef01234567\",\"binaries\":{\"evin\":{\"version\":\"1.2.3\",\"release_url\":\"https://dl.everyinit.dev/evin/canary/v1.2.3/release.json\"},\"esh\":{\"version\":\"1.2.3\",\"release_url\":\"https://dl.everyinit.dev/esh/canary/v1.2.3/release.json\"}}}\n",
    )
    .unwrap();

    let bin = temp.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let log = temp.path().join("corepack.log");
    let corepack = bin.join("corepack");
    fs::write(
        &corepack,
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$PUBLISH_LOG\"\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&corepack, fs::Permissions::from_mode(0o755)).unwrap();
    }

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(temp.path()).unwrap();
    ctx.set_env(
        "PATH",
        format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
    );
    ctx.set_env("PUBLISH_LOG", log.to_str().unwrap());
    ctx.set_positional_args(vec![
        "publish.esh".to_string(),
        "canary".to_string(),
        "1.2.3".to_string(),
        "objects".to_string(),
        "mirror".to_string(),
    ]);
    let code = engine
        .eval_file(
            &repository_root().join("scripts/release/mirror/publish.esh"),
            &mut ctx,
        )
        .await
        .unwrap();
    assert_eq!(code, 0, "{}", ctx.get_captured_stderr().unwrap());
    let uploads = fs::read_to_string(log).unwrap();
    assert_eq!(uploads.lines().count(), 10);
    assert!(uploads.contains("everyinit-artifacts/evin/canary/v1.2.3/release.json"));
    assert!(uploads.contains("everyinit-artifacts/esh/canary/v1.2.3/SHA256SUMS.sig"));
    assert!(uploads.contains("everyinit-artifacts/schema/v1/policy.schema.json"));
    assert!(uploads.contains("everyinit-artifacts/schema/v1/environment.schema.json"));
    assert!(uploads.lines().all(|line| line.contains("--remote")));
    assert!(!uploads.contains("channels/canary.json"));
}

#[tokio::test]
async fn verify_mirror_candidate_downloads_and_checks_signed_artifacts() {
    let temp = tempdir().unwrap();
    let remote = temp.path().join("remote");
    fs::create_dir_all(temp.path().join("infra/get")).unwrap();
    let key_pair = Ed25519KeyPair::from_pkcs8(
        Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
            .unwrap()
            .as_ref(),
    )
    .unwrap();
    fs::write(
        temp.path().join("infra/get/release-key.pub"),
        ed25519_public_key_pem(key_pair.public_key().as_ref()),
    )
    .unwrap();
    let commit = "0123456789abcdef0123456789abcdef01234567";
    for binary in ["evin", "esh"] {
        let directory = remote.join(format!("{binary}/canary/v1.2.3"));
        fs::create_dir_all(&directory).unwrap();
        let filename = format!("{binary}-v1.2.3-x86_64-unknown-linux-musl.tar.gz");
        let archive = format!("{binary} archive").into_bytes();
        let digest = format!("{:x}", Sha256::digest(&archive));
        let manifest = format!("{digest}  {filename}\n");
        fs::write(directory.join(&filename), archive).unwrap();
        fs::write(directory.join("SHA256SUMS"), &manifest).unwrap();
        fs::write(
            directory.join("SHA256SUMS.sig"),
            format!(
                "{}\n",
                BASE64.encode(key_pair.sign(manifest.as_bytes()).as_ref())
            ),
        )
        .unwrap();
        fs::write(
            directory.join("release.json"),
            format!(
                "{{\"name\":\"{binary}\",\"version\":\"1.2.3\",\"channel\":\"canary\",\"git_commit\":\"{commit}\",\"artifacts\":[{{\"filename\":\"{filename}\",\"sha256\":\"{digest}\"}}]}}\n"
            ),
        )
        .unwrap();
    }

    let base_url = format!("file://{}", remote.display());
    let (code, output, error) = run_script_with_env(
        "mirror/verify-candidate.esh",
        temp.path(),
        &["canary", "1.2.3", commit],
        &[("RELEASE_BASE_URL", &base_url)],
    )
    .await;
    assert_eq!(code, 0, "{error}");
    assert!(output.contains("Verified public canary artifacts"));
    assert!(
        temp.path()
            .join("crates/esh/dist/github/esh-v1.2.3-x86_64-unknown-linux-musl.tar.gz")
            .is_file()
    );
}

#[tokio::test]
async fn github_release_provider_snapshot_writes_the_reconciler_state() {
    let temp = tempdir().unwrap();
    let bin = temp.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let curl = bin.join("curl");
    fs::write(
        &curl,
        "#!/bin/sh\nout=\nurl=\nwhile test \"$#\" -gt 0; do\n  if test \"$1\" = -o; then out=$2; shift 2; continue; fi\n  url=$1\n  shift\ndone\ncase \"$url\" in\n  *page=1) printf '%s' '[{\"tag_name\":\"v1.2.3\",\"name\":\"Release 1.2.3\",\"draft\":false,\"prerelease\":true,\"body\":\"release notes\",\"published_at\":\"2026-01-02T03:04:05Z\",\"assets\":[{\"name\":\"evin.tar.gz\",\"size\":12,\"digest\":\"sha256:deadbeef\"}]}]' > \"$out\" ;;\n  *) printf '%s' '[]' > \"$out\" ;;\nesac\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&curl, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let provider_dir = temp.path().join("scripts/release/providers");
    fs::create_dir_all(&provider_dir).unwrap();
    fs::copy(
        repository_root().join("scripts/release/providers/github.esh"),
        provider_dir.join("github.esh"),
    )
    .unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(temp.path()).unwrap();
    ctx.set_env(
        "PATH",
        format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
    );
    ctx.set_env("GITHUB_REPOSITORY", "everyinit/test");
    ctx.set_env("GITHUB_TOKEN", "token");
    ctx.set_env("GITHUB_API_URL", "https://api.example.test");
    ctx.set_env("RELEASE_PROVIDER", "github");
    ctx.set_positional_args(vec!["release.esh".to_string(), "snapshot".to_string()]);
    let code = engine
        .eval_file(
            &repository_root().join("scripts/release/provider.esh"),
            &mut ctx,
        )
        .await
        .unwrap();
    assert_eq!(code, 0, "{}", ctx.get_captured_stderr().unwrap());
    let state = fs::read_to_string(
        temp.path()
            .join(".evin/actions/release-sync/remote-state.v1.json"),
    )
    .unwrap();
    assert!(state.contains("\"repository\": \"everyinit/test\""));
    assert!(state.contains("\"tag\": \"v1.2.3\""));
    assert!(state.contains("\"sha256\": \"deadbeef\""));
    assert!(state.contains("03fffb8510e0e948bc68ba8efb893d63695f7dd5ec998f6710d9635afee14b53"));
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
        "check/release-packages.esh",
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
