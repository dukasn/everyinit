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

use super::action::apk_architecture;
use super::*;
use evin_action_sdk::{Action, packaging::LinuxArch};
use std::collections::{BTreeMap, HashMap};
use tempfile::TempDir;

const BASE: &str = "output_dir: dist\narchitecture: x86_64\nrelease: ${bump.release}\npackage:\n  name: demo\n  summary: Demo\n  maintainer: Maintainer\n  payload:\n    - source: target/demo\n      strip_prefix: target\n      destination: /usr/bin\nlicense: MIT\n";

#[test]
fn apk_action_accepts_alpine_fields_and_rejects_debian_fields() {
    let action = ApkPackageAction;
    let valid = evin_flow::parse_yaml(BASE).unwrap();
    action.validate_config(Some(&valid)).unwrap();

    let foreign =
        evin_flow::parse_yaml(&format!("{BASE}description: Debian description\n")).unwrap();
    assert!(action.validate_config(Some(&foreign)).is_err());
    assert_eq!(action.package_release_contract().unwrap().target, "apk");
}

#[test]
fn apk_architecture_names_are_owned_by_the_apk_action() {
    assert_eq!(apk_architecture(LinuxArch::X86_64), "x86_64");
    assert_eq!(apk_architecture(LinuxArch::Noarch), "noarch");
}

#[tokio::test]
async fn apk_action_publishes_expected_path_size_and_gzip_signature() {
    let temp = TempDir::new().unwrap();
    std::fs::create_dir_all(temp.path().join("target")).unwrap();
    std::fs::write(temp.path().join("target/demo"), b"payload").unwrap();
    let ctx = context(temp.path(), "apk");
    let with = evin_flow::parse_yaml(
        "output_dir: dist\narchitecture: x86_64\nrelease: ${bump.release}\npackage:\n  name: demo\n  summary: Demo\n  maintainer: Maintainer\n  payload:\n    - source: target/demo\n      strip_prefix: target\n      destination: /usr/bin\nlicense: MIT\n",
    )
    .unwrap();

    ApkPackageAction.execute(&ctx, Some(with)).await.unwrap();

    let path = "dist/demo-1.2.3-r1.apk";
    let outputs = ctx.state.outputs.lock().unwrap();
    assert_eq!(outputs.get("path").map(String::as_str), Some(path));
    assert!(
        outputs
            .get("size")
            .and_then(|size| size.parse::<u64>().ok())
            .is_some_and(|size| size > 0)
    );
    let bytes = std::fs::read(temp.path().join(path)).unwrap();
    assert!(bytes.starts_with(&[0x1f, 0x8b]));
    assert!(bytes.len() > 64);
}

fn context(root: &std::path::Path, target: &str) -> evin_action_sdk::RunnerContext {
    use evin_action_sdk::release::{ArtifactPackage, ReleaseIdentity, RevisionState};
    let release = evin_action_sdk::release::ReleaseArtifact {
        schema: 1,
        release: ReleaseIdentity {
            version: "1.2.3".into(),
            tag: "v1.2.3".into(),
        },
        packages: BTreeMap::from([(
            "demo".into(),
            ArtifactPackage {
                revisions: BTreeMap::from([(
                    target.into(),
                    RevisionState {
                        version: "1.2.3".into(),
                        revision: 1,
                    },
                )]),
            },
        )]),
    };
    let (tx, _) = tokio::sync::mpsc::unbounded_channel();
    let secrets = std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default());
    let logger =
        evin_action_sdk::Logger::new("package-apk-test".into(), tx.clone(), secrets.clone());
    let git = std::sync::Arc::new(evin_action_sdk::git::GitProvider::new(
        root.to_path_buf(),
        evin_action_sdk::PipelineMode::Normal,
        logger,
    ));
    evin_action_sdk::RunnerContextBuilder::new(
        "package-apk-test".into(),
        tx,
        root.to_path_buf(),
        git,
        secrets,
    )
    .env(HashMap::from([("SOURCE_DATE_EPOCH".into(), "123".into())]))
    .inputs(HashMap::from([(
        "bump".into(),
        HashMap::from([("release".into(), serde_json::to_string(&release).unwrap())]),
    )]))
    .build()
}
