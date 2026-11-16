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
use std::collections::BTreeMap;

use crate::ArchiveAction;
use evin_action_sdk::Action;
use std::collections::HashMap;
use tempfile::TempDir;

fn spanned_value(value: serde_json::Value) -> evin_flow::SpannedValue {
    let span = evin_flow::Span::default();
    match value {
        serde_json::Value::Null => evin_flow::SpannedValue::Null(span),
        serde_json::Value::Bool(value) => evin_flow::SpannedValue::Bool(value, span),
        serde_json::Value::Number(value) => {
            if let Some(value) = value.as_i64() {
                evin_flow::SpannedValue::Integer(value, span)
            } else {
                evin_flow::SpannedValue::Float(value.as_f64().unwrap_or_default(), span)
            }
        }
        serde_json::Value::String(value) => evin_flow::SpannedValue::String(value, span),
        serde_json::Value::Array(values) => {
            evin_flow::SpannedValue::Sequence(values.into_iter().map(spanned_value).collect(), span)
        }
        serde_json::Value::Object(values) => evin_flow::SpannedValue::Mapping(
            values
                .into_iter()
                .map(|(key, value)| {
                    (
                        evin_flow::SpannedValue::String(key, span),
                        spanned_value(value),
                    )
                })
                .collect(),
            span,
        ),
    }
}

fn spanned_mapping(with: HashMap<String, serde_json::Value>) -> evin_flow::SpannedValue {
    evin_flow::SpannedValue::Mapping(
        with.into_iter()
            .map(|(key, value)| {
                (
                    evin_flow::SpannedValue::String(key, evin_flow::Span::default()),
                    spanned_value(value),
                )
            })
            .collect(),
        evin_flow::Span::default(),
    )
}

#[tokio::test]
async fn test_archive_creates_archive() {
    let temp_dir = TempDir::new().unwrap();
    let target_dir = temp_dir.path().join("target");
    let output_tar = temp_dir.path().join("out.tar.gz");

    std::fs::create_dir_all(&target_dir).unwrap();
    std::fs::write(target_dir.join("file1.txt"), "hello").unwrap();
    std::fs::write(target_dir.join("file2.txt"), "world").unwrap();

    let mut with = HashMap::new();
    with.insert(
        "path".to_string(),
        serde_json::Value::String(output_tar.to_string_lossy().to_string()),
    );
    with.insert(
        "include".to_string(),
        serde_json::Value::Array(vec![serde_json::Value::String("target/*.txt".to_string())]),
    );
    with.insert(
        "layout".to_string(),
        serde_json::Value::String("flat".to_string()),
    );

    let (log_tx, _) = tokio::sync::mpsc::unbounded_channel();
    let ctx = RunnerContext::new(
        None,
        "archive_test".to_string(),
        false,
        evin_action_sdk::PipelineMode::Normal,
        log_tx,
        vec![],
        Some(temp_dir.path().to_string_lossy().to_string()),
        std::collections::HashMap::from([("SOURCE_DATE_EPOCH".to_string(), "1".to_string())]),
        temp_dir.path().to_path_buf(),
        std::collections::HashMap::new(),
        std::sync::Arc::new(evin_action_sdk::git::GitProvider::new(
            temp_dir.path().to_path_buf(),
            evin_action_sdk::PipelineMode::Normal,
            evin_action_sdk::Logger::new(
                "test".to_string(),
                tokio::sync::mpsc::unbounded_channel().0,
                std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default()),
            ),
        )),
        std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default()),
    );

    let action = ArchiveAction;
    let res = action.execute(&ctx, Some(spanned_mapping(with))).await;

    assert!(res.is_ok(), "Archive execution failed: {:?}", res);
    assert!(output_tar.exists(), "Archive was not created!");
    let outputs = ctx.state.outputs.lock().unwrap();
    assert_eq!(
        outputs.get("path"),
        Some(&output_tar.to_string_lossy().to_string())
    );
    assert!(outputs.contains_key("size"));
    assert!(!outputs.contains_key("archive_path"));
    assert!(!outputs.contains_key("archive_size"));
}

#[tokio::test]
async fn test_archive_collision() {
    let temp_dir = TempDir::new().unwrap();
    let dir_a = temp_dir.path().join("a");
    let dir_b = temp_dir.path().join("b");
    let output_tar = temp_dir.path().join("out.tar.gz");

    std::fs::create_dir_all(&dir_a).unwrap();
    std::fs::create_dir_all(&dir_b).unwrap();
    std::fs::write(dir_a.join("file.txt"), "hello").unwrap();
    std::fs::write(dir_b.join("file.txt"), "world").unwrap();

    let mut with = HashMap::new();
    with.insert(
        "path".to_string(),
        serde_json::Value::String(output_tar.to_string_lossy().to_string()),
    );
    with.insert(
        "include".to_string(),
        serde_json::Value::Array(vec![
            serde_json::Value::String("a/*.txt".to_string()),
            serde_json::Value::String("b/*.txt".to_string()),
        ]),
    );
    with.insert(
        "layout".to_string(),
        serde_json::Value::String("flat".to_string()),
    );

    let (log_tx, _) = tokio::sync::mpsc::unbounded_channel();
    let ctx = RunnerContext::new(
        None,
        "archive_test".to_string(),
        false,
        evin_action_sdk::PipelineMode::Normal,
        log_tx,
        vec![],
        Some(temp_dir.path().to_string_lossy().to_string()),
        std::collections::HashMap::from([("SOURCE_DATE_EPOCH".to_string(), "1".to_string())]),
        temp_dir.path().to_path_buf(),
        std::collections::HashMap::new(),
        std::sync::Arc::new(evin_action_sdk::git::GitProvider::new(
            temp_dir.path().to_path_buf(),
            evin_action_sdk::PipelineMode::Normal,
            evin_action_sdk::Logger::new(
                "test".to_string(),
                tokio::sync::mpsc::unbounded_channel().0,
                std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default()),
            ),
        )),
        std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default()),
    );

    let action = ArchiveAction;
    let res = action.execute(&ctx, Some(spanned_mapping(with))).await;

    assert!(res.is_err());
    let err = res.unwrap_err();
    assert_eq!(err.code.key, "path-collision");
    assert_eq!(err.title, evin_locales::t!("path-collision-title"));
    assert_eq!(
        evin_locales::render_issue(&err.code),
        evin_locales::t!(
            "path-collision",
            destination = "file.txt",
            source1 = dir_a.join("file.txt").display().to_string(),
            source2 = dir_b.join("file.txt").display().to_string()
        )
    );
}

#[tokio::test]
async fn test_archive_exclude_globs() {
    let temp_dir = TempDir::new().unwrap();
    let target_dir = temp_dir.path().join("target");
    let output_tar = temp_dir.path().join("out.tar.gz");

    std::fs::create_dir_all(&target_dir).unwrap();
    std::fs::write(target_dir.join("file1.txt"), "hello").unwrap();
    std::fs::write(target_dir.join("file2.tmp"), "world").unwrap();

    let mut with = HashMap::new();
    with.insert(
        "path".to_string(),
        serde_json::Value::String(output_tar.to_string_lossy().to_string()),
    );
    with.insert(
        "include".to_string(),
        serde_json::Value::Array(vec![serde_json::Value::String("target/*".to_string())]),
    );
    with.insert(
        "exclude".to_string(),
        serde_json::Value::Array(vec![serde_json::Value::String("*.tmp".to_string())]),
    );
    with.insert(
        "layout".to_string(),
        serde_json::Value::String("flat".to_string()),
    );

    let (log_tx, _) = tokio::sync::mpsc::unbounded_channel();
    let ctx = RunnerContext::new(
        None,
        "archive_test".to_string(),
        false,
        evin_action_sdk::PipelineMode::Normal,
        log_tx,
        vec![],
        Some(temp_dir.path().to_string_lossy().to_string()),
        std::collections::HashMap::from([("SOURCE_DATE_EPOCH".to_string(), "1".to_string())]),
        temp_dir.path().to_path_buf(),
        std::collections::HashMap::new(),
        std::sync::Arc::new(evin_action_sdk::git::GitProvider::new(
            temp_dir.path().to_path_buf(),
            evin_action_sdk::PipelineMode::Normal,
            evin_action_sdk::Logger::new(
                "test".to_string(),
                tokio::sync::mpsc::unbounded_channel().0,
                std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default()),
            ),
        )),
        std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default()),
    );

    let action = ArchiveAction;
    let res = action.execute(&ctx, Some(spanned_mapping(with))).await;

    assert!(res.is_ok(), "Archive execution failed: {:?}", res);
}
#[tokio::test]
async fn test_archive_relative_layout_outside_cwd() {
    let temp_dir = TempDir::new().unwrap();
    let module_a = temp_dir.path().join("module_a");
    let module_b = temp_dir.path().join("module_b");
    let output_tar = temp_dir.path().join("out.tar.gz");

    std::fs::create_dir_all(&module_a).unwrap();
    std::fs::create_dir_all(&module_b).unwrap();
    std::fs::write(module_a.join("file.txt"), "hello").unwrap();
    std::fs::write(module_b.join("file.txt"), "world").unwrap();

    let mut with = HashMap::new();
    with.insert(
        "path".to_string(),
        serde_json::Value::String(output_tar.to_string_lossy().to_string()),
    );
    with.insert(
        "include".to_string(),
        serde_json::Value::Array(vec![serde_json::Value::String(
            "../module_b/*.txt".to_string(),
        )]),
    );
    with.insert(
        "layout".to_string(),
        serde_json::Value::String("relative".to_string()),
    );

    let (log_tx, _) = tokio::sync::mpsc::unbounded_channel();
    let ctx = RunnerContext::new(
        None,
        "archive_test".to_string(),
        false,
        evin_action_sdk::PipelineMode::Normal,
        log_tx,
        vec![],
        Some(module_a.to_string_lossy().to_string()), // CWD is module_a
        std::collections::HashMap::new(),
        temp_dir.path().to_path_buf(), // workspace_root is temp_dir
        std::collections::HashMap::new(),
        std::sync::Arc::new(evin_action_sdk::git::GitProvider::new(
            temp_dir.path().to_path_buf(),
            evin_action_sdk::PipelineMode::Normal,
            evin_action_sdk::Logger::new(
                "test".to_string(),
                tokio::sync::mpsc::unbounded_channel().0,
                std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default()),
            ),
        )),
        std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default()),
    );

    let action = ArchiveAction;
    let res = action.execute(&ctx, Some(spanned_mapping(with))).await;
    let error = res.unwrap_err();
    assert_eq!(error.code.key, "relative-path-escape");
    assert_eq!(
        evin_locales::render_issue(&error.code),
        evin_locales::t!(
            "relative-path-escape",
            path = module_b.join("file.txt").display().to_string()
        )
    );
}

#[tokio::test]
async fn test_archive_template_interpolation() {
    let temp_dir = TempDir::new().unwrap();
    let target_dir = temp_dir.path().join("target");
    let output_tar = temp_dir.path().join("dist/release-1.2.3.tar.gz");

    std::fs::create_dir_all(&target_dir).unwrap();
    std::fs::write(target_dir.join("file1.txt"), "hello").unwrap();

    let mut with = HashMap::new();
    with.insert(
        "path".to_string(),
        serde_json::Value::String(
            temp_dir
                .path()
                .join("dist/release-${bump.version}.tar.gz")
                .to_string_lossy()
                .to_string(),
        ),
    );
    with.insert(
        "include".to_string(),
        serde_json::Value::Array(vec![serde_json::Value::String("target/*.txt".to_string())]),
    );
    with.insert(
        "layout".to_string(),
        serde_json::Value::String("flat".to_string()),
    );
    with.insert(
        "prefix".to_string(),
        serde_json::Value::String("evin-${bump.version}".to_string()),
    );

    let mut bump_outputs = HashMap::new();
    bump_outputs.insert("version".to_string(), "1.2.3".to_string());
    let mut inputs = HashMap::new();
    inputs.insert("bump".to_string(), bump_outputs);

    let (log_tx, _) = tokio::sync::mpsc::unbounded_channel();
    let ctx = RunnerContext::new(
        None,
        "archive_test".to_string(),
        false,
        evin_action_sdk::PipelineMode::Normal,
        log_tx,
        vec![],
        Some(temp_dir.path().to_string_lossy().to_string()),
        std::collections::HashMap::from([("SOURCE_DATE_EPOCH".to_string(), "1".to_string())]),
        temp_dir.path().to_path_buf(),
        inputs,
        std::sync::Arc::new(evin_action_sdk::git::GitProvider::new(
            temp_dir.path().to_path_buf(),
            evin_action_sdk::PipelineMode::Normal,
            evin_action_sdk::Logger::new(
                "test".to_string(),
                tokio::sync::mpsc::unbounded_channel().0,
                std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default()),
            ),
        )),
        std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default()),
    );

    let action = ArchiveAction;
    let res = action.execute(&ctx, Some(spanned_mapping(with))).await;

    assert!(res.is_ok(), "Archive execution failed: {:?}", res);
    assert!(output_tar.exists(), "Interpolated Archive was not created!");
    let outputs = ctx.state.outputs.lock().unwrap();
    assert_eq!(
        outputs.get("path"),
        Some(&output_tar.to_string_lossy().to_string())
    );

    // Verify the prefix inside the tar
    use flate2::read::GzDecoder;
    use std::fs::File;
    use tar::Archive;

    let tar_gz = File::open(output_tar).unwrap();
    let tar = GzDecoder::new(tar_gz);
    let mut archive = Archive::new(tar);
    let mut entries = archive.entries().unwrap();
    let first_entry = entries.next().unwrap().unwrap();
    let path = String::from_utf8_lossy(&first_entry.path_bytes()).into_owned();

    assert!(
        !path.contains('\\'),
        "archive path was not portable: {path}"
    );
    assert!(
        path.contains("evin-1.2.3"),
        "Prefix was not interpolated, got: {}",
        path
    );
}

#[tokio::test]
async fn dry_run_missing_shell_artifact_is_unverified_not_failed() {
    let temp = TempDir::new().unwrap();
    let (log_tx, _) = tokio::sync::mpsc::unbounded_channel();
    let dry_run = evin_action_sdk::dry_run::DryRunState::default();
    let git = std::sync::Arc::new(evin_action_sdk::git::GitProvider::new_with_dry_run(
        temp.path().to_path_buf(),
        evin_action_sdk::PipelineMode::DryRun,
        evin_action_sdk::Logger::new(
            "package".to_string(),
            log_tx.clone(),
            std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default()),
        ),
        dry_run.clone(),
    ));
    let ctx = RunnerContext::new_with_dry_run(
        None,
        "package".to_string(),
        false,
        evin_action_sdk::PipelineMode::DryRun,
        log_tx,
        vec![],
        None,
        HashMap::new(),
        temp.path().to_path_buf(),
        [("build".to_string(), HashMap::new())]
            .into_iter()
            .collect(),
        ["build".to_string()].into_iter().collect(),
        git,
        std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default()),
        dry_run,
    );
    ctx.mark_unverified("Depends on skipped build");

    let with =
        evin_flow::parse_yaml("path: dist/app.tar.gz\ninclude: [target/release/app]\nlayout: flat")
            .unwrap();
    ArchiveAction.execute(&ctx, Some(with)).await.unwrap();

    let outputs = ctx.state.outputs.lock().unwrap();
    assert_eq!(
        outputs.get("path").map(String::as_str),
        Some("dist/app.tar.gz")
    );
    assert_eq!(outputs.get("size").map(String::as_str), Some("unknown"));
    assert!(!ctx.uncertainty().is_empty());
    assert!(!temp.path().join("dist/app.tar.gz").exists());
}

#[tokio::test]
async fn distro_packages_use_inline_config_and_derived_paths() {
    let temp = TempDir::new().unwrap();
    std::fs::create_dir_all(temp.path().join("bin")).unwrap();
    std::fs::write(temp.path().join("bin/example"), "hello\n").unwrap();
    std::fs::write(
        temp.path().join("bin/install"),
        "post_install() { echo installed; }\n",
    )
    .unwrap();
    let (log_tx, _) = tokio::sync::mpsc::unbounded_channel();
    let mut ctx = RunnerContext::new(
        None,
        "package".into(),
        false,
        evin_action_sdk::PipelineMode::Normal,
        log_tx,
        vec![],
        None,
        HashMap::from([("SOURCE_DATE_EPOCH".into(), "123".into())]),
        temp.path().to_path_buf(),
        HashMap::new(),
        std::sync::Arc::new(evin_action_sdk::git::GitProvider::new(
            temp.path().to_path_buf(),
            evin_action_sdk::PipelineMode::Normal,
            evin_action_sdk::Logger::new(
                "test".into(),
                tokio::sync::mpsc::unbounded_channel().0,
                std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default()),
            ),
        )),
        std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default()),
    );
    ctx.values =
        evin_action_sdk::Values::new(HashMap::from([("package_name".into(), "example".into())]));
    let release = evin_action_sdk::release::ReleaseArtifact {
        schema: 1,
        release: evin_action_sdk::release::ReleaseIdentity {
            version: "1.2.3".into(),
            tag: "v1.2.3".into(),
        },
        packages: std::collections::BTreeMap::from([(
            "example".into(),
            evin_action_sdk::release::ArtifactPackage {
                revisions: std::collections::BTreeMap::from([
                    (
                        "deb".into(),
                        evin_action_sdk::release::RevisionState {
                            version: "1.2.3".into(),
                            revision: 1,
                        },
                    ),
                    (
                        "apk".into(),
                        evin_action_sdk::release::RevisionState {
                            version: "1.2.3".into(),
                            revision: 1,
                        },
                    ),
                    (
                        "arch".into(),
                        evin_action_sdk::release::RevisionState {
                            version: "1.2.3".into(),
                            revision: 1,
                        },
                    ),
                ]),
            },
        )]),
    };
    ctx.state.inputs.insert(
        "bump".into(),
        HashMap::from([("release".into(), serde_json::to_string(&release).unwrap())]),
    );

    let package = "package:\n  name: ${package_name}\n  summary: Example package\n  maintainer: Example <example@example.test>\n  license: MIT\n  payload:\n    - source: bin/example\n      strip_prefix: bin\n      destination: /usr/bin\n      mode: 493\n  targets:\n    deb:\n      depends: [libc6]\n    apk:\n      depends: [so:libc.musl-x86_64.so.1]\n    arch:\n      depends: [glibc]\n      optdepends: ['bash: completion support']\n      provides: [example-cli]\n      conflicts: [example-old]\n      replaces: [example-old]\n      backup: [etc/example.conf]\n      scripts:\n        install: bin/install\n";
    let deb_with = evin_flow::parse_yaml(&format!(
        "format: deb\noutput_dir: dist\narchitecture: x86_64\nrelease: ${{bump.release}}\n{package}"
    ))
    .unwrap();
    ArchiveAction.execute(&ctx, Some(deb_with)).await.unwrap();
    let deb = std::fs::read(temp.path().join("dist/example_1.2.3-1_amd64.deb")).unwrap();
    assert!(deb.starts_with(b"!<arch>\n"));
    assert!(
        deb.windows(b"control.tar.gz".len())
            .any(|window| window == b"control.tar.gz")
    );
    assert!(
        deb.windows(b"data.tar.gz".len())
            .any(|window| window == b"data.tar.gz")
    );

    let apk_with = evin_flow::parse_yaml(&format!(
        "format: apk\noutput_dir: dist\narchitecture: x86_64\nrelease: ${{bump.release}}\n{package}"
    ))
    .unwrap();
    ArchiveAction.execute(&ctx, Some(apk_with)).await.unwrap();
    let apk = std::fs::read(temp.path().join("dist/example-1.2.3-r1.apk")).unwrap();
    assert!(apk.starts_with(&[0x1f, 0x8b]));
    assert!(apk.len() > 64);
    let arch_with = evin_flow::parse_yaml(&format!("format: arch\noutput_dir: dist\narchitecture: x86_64\nrelease: ${{bump.release}}\n{package}")).unwrap();
    ArchiveAction
        .execute(&ctx, Some(arch_with.clone()))
        .await
        .unwrap();
    let arch_path = temp.path().join("dist/example-1.2.3-1-x86_64.pkg.tar.zst");
    let bytes = std::fs::read(&arch_path).unwrap();
    let tar = zstandard::decode_all(bytes.as_slice()).unwrap();
    let mut archive = tar::Archive::new(std::io::Cursor::new(tar));
    let mut members = BTreeMap::new();
    for entry in archive.entries().unwrap() {
        let mut entry = entry.unwrap();
        let path = entry.path().unwrap().to_string_lossy().into_owned();
        let mut contents = Vec::new();
        std::io::Read::read_to_end(&mut entry, &mut contents).unwrap();
        members.insert(path, contents);
    }
    let names = members.keys().cloned().collect::<Vec<_>>();
    assert_eq!(names, {
        let mut sorted = names.clone();
        sorted.sort();
        sorted
    });
    let pkginfo = std::str::from_utf8(&members[".PKGINFO"]).unwrap();
    assert!(pkginfo.contains("pkgbase = example\n"));
    assert!(pkginfo.contains("xdata = pkgtype=pkg\n"));
    assert!(pkginfo.contains("pkgver = 1.2.3-1\n"));
    assert!(pkginfo.contains("optdepend = bash: completion support\n"));
    assert!(pkginfo.contains("provides = example-cli\n"));
    assert!(pkginfo.contains("conflict = example-old\n"));
    assert!(pkginfo.contains("replaces = example-old\n"));
    assert!(pkginfo.contains("backup = etc/example.conf\n"));
    assert!(
        std::str::from_utf8(&members[".MTREE"])
            .unwrap()
            .contains("./usr/bin/example type=file mode=755 time=123")
    );
    assert_eq!(members[".INSTALL"], b"post_install() { echo installed; }\n");
    ArchiveAction.execute(&ctx, Some(arch_with)).await.unwrap();
    assert_eq!(bytes, std::fs::read(arch_path).unwrap());
    assert_eq!(
        evin_action_sdk::packaging::LinuxArch::X86_64.arch(),
        "x86_64"
    );
}

mod config;
