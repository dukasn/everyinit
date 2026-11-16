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
use crate::v1::config::{ArchScripts, ArchTarget, PackageConfig, PackageTargets};

#[test]
fn formats_reject_each_others_keys() {
    let tar_with_package = evin_flow::parse_yaml(
        "path: out.tar.gz\ninclude: [x]\npackage: { name: app, summary: app, maintainer: A, license: MIT }\n",
    )
    .unwrap();
    assert_config_error(
        &tar_with_package,
        evin_locales::t!("archive-distro-fields-only-on-package-formats"),
    );
    let package_with_path = evin_flow::parse_yaml(
            "format: deb\npath: out.deb\noutput_dir: dist\narchitecture: x86_64\nrelease: ${bump.release}\npackage: { name: app, summary: app, maintainer: A, license: MIT, targets: { deb: {} } }\n",
        ).unwrap();
    assert_config_error(
        &package_with_path,
        evin_locales::t!("archive-tar-gz-fields-only-on-tar-gz"),
    );
    let configured_default = evin_flow::parse_yaml(
        "format: deb\noutput_dir: dist\narchitecture: x86_64\nlayout: flat\n",
    )
    .unwrap();
    assert_config_error(
        &configured_default,
        evin_locales::t!("archive-distro-fields-required"),
    );
}

#[test]
fn archive_configuration_failures_use_fluent_messages() {
    for (yaml, expected) in [
        (
            "format: zip\n",
            evin_locales::t!("unsupported-format", format = "zip"),
        ),
        (
            "include: [src/**]\n",
            evin_locales::t!("archive-path-must-not-be-empty"),
        ),
        (
            "path: out.tar.gz\n",
            evin_locales::t!("include-must-contain-at-least-one-pattern"),
        ),
        (
            "path: out.tar.gz\ninclude: [src/**]\ncompression_level: 10\n",
            evin_locales::t!("compression-level-invalid"),
        ),
        (
            "format: deb\n",
            evin_locales::t!("archive-distro-fields-required"),
        ),
    ] {
        let with = evin_flow::parse_yaml(yaml).unwrap();
        assert_config_error(&with, expected);
    }
}

fn assert_config_error(with: &evin_flow::SpannedValue, expected: String) {
    let error = validate_config(Some(with)).unwrap_err();
    assert!(
        !expected.starts_with("<missing locale key:"),
        "expected Fluent message to exist: {expected}"
    );
    assert_eq!(error.message, expected);
}

#[test]
fn package_release_reference_must_be_a_declared_need() {
    let with = evin_flow::parse_yaml("format: deb\noutput_dir: dist\narchitecture: x86_64\npackage: { name: app, summary: app, maintainer: A, license: MIT, targets: { deb: {} } }\nrelease: ${bump.release}\n").unwrap();
    let missing = evin_action_sdk::ActionValidationContext {
        task_name: "package",
        needs: &[],
    };
    let error = validate_task_config(&missing, Some(&with)).unwrap_err();
    assert_eq!(
        error.message,
        evin_locales::t!("archive-release-reference-missing-need", task = "bump")
    );
    let needs = vec!["bump".to_string()];
    let valid = evin_action_sdk::ActionValidationContext {
        task_name: "package",
        needs: &needs,
    };
    assert!(validate_task_config(&valid, Some(&with)).is_ok());
}

#[test]
fn package_validation_rejects_invalid_relations_and_backup_paths() {
    let invalid_relation = evin_flow::parse_yaml(
            "format: arch\noutput_dir: dist\narchitecture: x86_64\nrelease: ${bump.release}\npackage: { name: app, summary: app, maintainer: A, license: MIT, targets: { arch: { depends: [glibc, glibc] } } }\n",
        )
        .unwrap();
    let config: ArchiveConfig = evin_action_sdk::parse_config(Some(&invalid_relation)).unwrap();
    let error =
        validate_package(config.package.as_ref().unwrap(), PackageFormat::Arch).unwrap_err();
    let expected = evin_locales::t!(
        "archive-package-relation-duplicate",
        field = "depends",
        value = "glibc"
    );
    assert!(
        !expected.starts_with("<missing locale key:"),
        "expected Fluent message to exist: {expected}"
    );
    assert_eq!(error.code.key, "archive-package-relation-duplicate");
    assert_eq!(evin_locales::render_issue(&error.code), expected);

    let unsafe_backup = evin_flow::parse_yaml(
            "format: arch\noutput_dir: dist\narchitecture: x86_64\nrelease: ${bump.release}\npackage: { name: app, summary: app, maintainer: A, license: MIT, targets: { arch: { backup: ['../etc/app.conf'] } } }\n",
        )
        .unwrap();
    let config: ArchiveConfig = evin_action_sdk::parse_config(Some(&unsafe_backup)).unwrap();
    let error =
        validate_package(config.package.as_ref().unwrap(), PackageFormat::Arch).unwrap_err();
    let expected = evin_locales::t!("package-path-invalid", path = "../etc/app.conf");
    assert_eq!(error.code.key, "package-path-invalid");
    assert_eq!(evin_locales::render_issue(&error.code), expected);
}

#[test]
fn package_metadata_relations_and_scripts_use_fluent_errors() {
    let mut package = valid_package();
    package.summary = "two\nlines".into();
    assert_package_error(
        &package,
        "archive-package-metadata-invalid",
        evin_locales::t!("archive-package-metadata-invalid", field = "summary"),
    );

    let mut package = valid_package();
    package.name = "Bad Name".into();
    assert_package_error(
        &package,
        "archive-package-name-invalid",
        evin_locales::t!("archive-package-name-invalid"),
    );

    let mut package = valid_package();
    package.targets.arch = Some(ArchTarget {
        depends: vec![" ".into()],
        ..ArchTarget::default()
    });
    assert_package_error(
        &package,
        "archive-package-relation-invalid",
        evin_locales::t!("archive-package-relation-invalid", field = "depends"),
    );

    let mut package = valid_package();
    package.targets.arch = Some(ArchTarget {
        scripts: ArchScripts {
            install: Some(" ".into()),
        },
        ..ArchTarget::default()
    });
    assert_package_error(
        &package,
        "archive-package-script-path-empty",
        evin_locales::t!("archive-package-script-path-empty"),
    );
}

fn valid_package() -> PackageConfig {
    PackageConfig {
        name: "app".into(),
        summary: "An app".into(),
        description: None,
        maintainer: "A".into(),
        license: "MIT".into(),
        homepage: None,
        payload: Vec::new(),
        targets: PackageTargets::default(),
    }
}

fn assert_package_error(package: &PackageConfig, key: &str, expected: String) {
    let error = validate_package(package, PackageFormat::Arch).unwrap_err();
    assert_eq!(error.code.key, key);
    assert_eq!(evin_locales::render_issue(&error.code), expected);
}

#[test]
fn configuration_error_uses_fluent_message() {
    let with =
        evin_flow::parse_yaml("format: tar.gz\npath: out.tar.gz\ninclude: [x]\noutput_dir: dist\n")
            .unwrap();
    let config: ArchiveConfig = evin_action_sdk::parse_config(Some(&with)).unwrap();
    let error = config.validate().unwrap_err();
    assert_eq!(
        error.code.key,
        "archive-distro-fields-only-on-package-formats"
    );
    let expected = evin_locales::t!("archive-distro-fields-only-on-package-formats");
    assert!(
        !expected.starts_with("<missing locale key:"),
        "expected Fluent message to exist: {expected}"
    );
    assert_eq!(evin_locales::render_issue(&error.code), expected);
}
