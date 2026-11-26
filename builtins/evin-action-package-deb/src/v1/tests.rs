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

use super::action::deb_architecture;
use super::*;
use evin_action_sdk::{Action, ActionValidationContext, packaging::LinuxArch};
use std::collections::{BTreeMap, HashMap};
use tempfile::TempDir;

const BASE: &str = "output_dir: dist\narchitecture: x86_64\nrelease: ${bump.release}\npackage:\n  name: demo\n  summary: Demo\n  maintainer: Maintainer\n  payload:\n    - source: target/demo\n      strip_prefix: target\n      destination: /usr/bin\n";

#[test]
fn deb_action_accepts_its_schema_and_rejects_alpine_fields() {
    let action = DebPackageAction;
    let valid = evin_flow::parse_yaml(BASE).unwrap();
    action.validate_config(Some(&valid)).unwrap();

    let foreign = evin_flow::parse_yaml(&format!("{BASE}license: MIT\n")).unwrap();
    assert!(action.validate_config(Some(&foreign)).is_err());
    assert_eq!(action.package_release_contract().unwrap().target, "deb");
}

#[test]
fn deb_release_must_be_a_declared_task_dependency() {
    let action = DebPackageAction;
    let valid = evin_flow::parse_yaml(BASE).unwrap();
    let no_needs = ActionValidationContext {
        task_name: "package",
        needs: &[],
    };
    assert!(
        action
            .validate_task_config(&no_needs, Some(&valid))
            .is_err()
    );
    let needs = vec!["bump".to_string()];
    let declared = ActionValidationContext {
        task_name: "package",
        needs: &needs,
    };
    action
        .validate_task_config(&declared, Some(&valid))
        .unwrap();
}

#[test]
fn deb_architecture_names_are_owned_by_the_deb_action() {
    assert_eq!(deb_architecture(LinuxArch::X86_64), "amd64");
    assert_eq!(deb_architecture(LinuxArch::Noarch), "all");
}

#[tokio::test]
async fn deb_action_publishes_expected_path_size_and_container_signature() {
    let temp = TempDir::new().unwrap();
    std::fs::create_dir_all(temp.path().join("target")).unwrap();
    std::fs::write(temp.path().join("target/demo"), b"payload").unwrap();
    let release = release_artifact("deb");
    let (log_tx, _) = tokio::sync::mpsc::unbounded_channel();
    let secrets = std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default());
    let logger =
        evin_action_sdk::Logger::new("package-deb-test".into(), log_tx.clone(), secrets.clone());
    let git = std::sync::Arc::new(evin_action_sdk::git::GitProvider::new(
        temp.path().to_path_buf(),
        evin_action_sdk::PipelineMode::Normal,
        logger,
    ));
    let ctx = evin_action_sdk::RunnerContextBuilder::new(
        "package-deb-test".into(),
        log_tx,
        temp.path().to_path_buf(),
        git,
        secrets,
    )
    .env(HashMap::from([("SOURCE_DATE_EPOCH".into(), "123".into())]))
    .inputs(HashMap::from([(
        "bump".into(),
        HashMap::from([("release".into(), serde_json::to_string(&release).unwrap())]),
    )]))
    .build();
    let with = evin_flow::parse_yaml(
        "output_dir: dist\narchitecture: x86_64\nrelease: ${bump.release}\npackage:\n  name: demo\n  summary: Demo\n  maintainer: Maintainer\n  payload:\n    - source: target/demo\n      strip_prefix: target\n      destination: /usr/bin\n",
    )
    .unwrap();

    DebPackageAction.execute(&ctx, Some(with)).await.unwrap();

    let path = "dist/demo_1.2.3-1_amd64.deb";
    let outputs = ctx.state.outputs.lock().unwrap();
    assert_eq!(outputs.get("path").map(String::as_str), Some(path));
    assert!(
        outputs
            .get("size")
            .and_then(|size| size.parse::<u64>().ok())
            .is_some_and(|size| size > 0)
    );
    let bytes = std::fs::read(temp.path().join(path)).unwrap();
    assert!(bytes.starts_with(b"!<arch>\n"));
}

fn release_artifact(target: &str) -> evin_action_sdk::release::ReleaseArtifact {
    use evin_action_sdk::release::{ArtifactPackage, ReleaseIdentity, RevisionState};
    evin_action_sdk::release::ReleaseArtifact {
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
    }
}
