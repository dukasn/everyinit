// Copyright (C) 2026 Eduardo Marques Santos
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License, version 3.

use super::*;
use crate::v1::config::{ArchConfig, DebConfig};

#[test]
fn archive_schema_accepts_only_archive_options() {
    for yaml in [
        "include: [src/**]\n",
        "path: out.tar.gz\ninclude: [src/**]\nformat: tar.gz\n",
        "path: out.tar.gz\ninclude: [src/**]\noutput_dir: dist\n",
    ] {
        let with = evin_flow::parse_yaml(yaml).unwrap();
        assert!(validate_archive_config(Some(&with)).is_err(), "{yaml}");
    }

    let valid = evin_flow::parse_yaml("path: out.tar.gz\ninclude: [src/**]\n").unwrap();
    assert!(validate_archive_config(Some(&valid)).is_ok());
}

#[test]
fn archive_validation_failures_use_their_fluent_messages() {
    for (yaml, expected) in [
        (
            "path: ''\ninclude: [src/**]\n",
            evin_locales::t!("archive-path-must-not-be-empty"),
        ),
        (
            "path: out.tar.gz\ninclude: []\n",
            evin_locales::t!("include-must-contain-at-least-one-pattern"),
        ),
        (
            "path: out.tar.gz\ninclude: [' ']\n",
            evin_locales::t!("include-patterns-must-not-be-empty"),
        ),
        (
            "path: out.tar.gz\ninclude: [src/**]\nexclude: [' ']\n",
            evin_locales::t!("exclude-patterns-must-not-be-empty"),
        ),
        (
            "path: out.tar.gz\ninclude: [src/**]\nprefix: ' '\n",
            evin_locales::t!("prefix-must-not-be-empty-when-provided"),
        ),
        (
            "path: out.tar.gz\ninclude: [src/**]\ncompression_level: 10\n",
            evin_locales::t!("compression-level-invalid"),
        ),
    ] {
        let with = evin_flow::parse_yaml(yaml).unwrap();
        assert_eq!(
            validate_archive_config(Some(&with)).unwrap_err().message,
            expected
        );
    }
}

#[test]
fn package_schemas_reject_fields_owned_by_other_formats() {
    let base = "output_dir: dist\narchitecture: x86_64\nrelease: ${bump.release}\npackage: { name: app, summary: app, maintainer: A }\n";
    let deb_with_apk_license = evin_flow::parse_yaml(&format!("{base}license: MIT\n")).unwrap();
    assert!(validate_deb_config(Some(&deb_with_apk_license)).is_err());

    let apk_with_deb_description =
        evin_flow::parse_yaml(&format!("{base}license: MIT\ndescription: Long text\n")).unwrap();
    assert!(validate_apk_config(Some(&apk_with_deb_description)).is_err());

    let arch_with_archive_path =
        evin_flow::parse_yaml(&format!("{base}license: MIT\npath: out.pkg\n")).unwrap();
    assert!(validate_arch_config(Some(&arch_with_archive_path)).is_err());
}

#[test]
fn package_schemas_require_release_output_and_format_metadata() {
    let missing_package =
        evin_flow::parse_yaml("output_dir: dist\narchitecture: x86_64\nrelease: ${bump.release}\n")
            .unwrap();
    assert!(validate_deb_config(Some(&missing_package)).is_err());

    let missing_license = evin_flow::parse_yaml(
        "output_dir: dist\narchitecture: x86_64\nrelease: ${bump.release}\npackage: { name: app, summary: app, maintainer: A }\n",
    )
    .unwrap();
    assert!(validate_apk_config(Some(&missing_license)).is_err());

    let empty_output = evin_flow::parse_yaml(
        "output_dir: ''\narchitecture: x86_64\nrelease: ${bump.release}\npackage: { name: app, summary: app, maintainer: A }\n",
    )
    .unwrap();
    assert_eq!(
        validate_deb_config(Some(&empty_output))
            .unwrap_err()
            .message,
        evin_locales::t!("package-output-and-release-required")
    );
}

#[test]
fn package_release_reference_must_be_a_declared_need() {
    let with = evin_flow::parse_yaml(
        "output_dir: dist\narchitecture: x86_64\nrelease: ${bump.release}\npackage: { name: app, summary: app, maintainer: A }\n",
    )
    .unwrap();
    let missing = evin_action_sdk::ActionValidationContext {
        task_name: "package",
        needs: &[],
    };
    let error = validate_deb_task_config(&missing, Some(&with)).unwrap_err();
    assert_eq!(
        error.message,
        evin_locales::t!("archive-release-reference-missing-need", task = "bump")
    );

    let needs = vec!["bump".to_string()];
    let valid = evin_action_sdk::ActionValidationContext {
        task_name: "package",
        needs: &needs,
    };
    assert!(validate_deb_task_config(&valid, Some(&with)).is_ok());
}

#[test]
fn invalid_package_release_reference_uses_its_fluent_message() {
    let error = crate::v1::config::validate_release_reference("release.output", &[]).unwrap_err();
    assert_eq!(
        evin_locales::render_issue(&error),
        evin_locales::t!("archive-release-reference-format-invalid")
    );
}

#[test]
fn format_specific_metadata_is_validated_only_by_its_owner() {
    let invalid_relation = evin_flow::parse_yaml(
        "output_dir: dist\narchitecture: x86_64\nrelease: ${bump.release}\nlicense: MIT\npackage: { name: app, summary: app, maintainer: A }\ndepends: [glibc, glibc]\n",
    )
    .unwrap();
    let config: ArchConfig = evin_action_sdk::parse_config(Some(&invalid_relation)).unwrap();
    let error = validate_arch(&config).unwrap_err();
    assert_eq!(error.code.key, "archive-package-relation-duplicate");
    assert_eq!(
        evin_locales::render_issue(&error.code),
        evin_locales::t!(
            "archive-package-relation-duplicate",
            field = "depends",
            value = "glibc"
        )
    );

    let invalid_relation = evin_flow::parse_yaml(
        "output_dir: dist\narchitecture: x86_64\nrelease: ${bump.release}\nlicense: MIT\npackage: { name: app, summary: app, maintainer: A }\ndepends: [' ']\n",
    )
    .unwrap();
    let config: ArchConfig = evin_action_sdk::parse_config(Some(&invalid_relation)).unwrap();
    let error = validate_arch(&config).unwrap_err();
    assert_eq!(error.code.key, "archive-package-relation-invalid");
    assert_eq!(
        evin_locales::render_issue(&error.code),
        evin_locales::t!("archive-package-relation-invalid", field = "depends")
    );

    let unsafe_backup = evin_flow::parse_yaml(
        "output_dir: dist\narchitecture: x86_64\nrelease: ${bump.release}\nlicense: MIT\npackage: { name: app, summary: app, maintainer: A }\nbackup: ['../etc/app.conf']\n",
    )
    .unwrap();
    let config: ArchConfig = evin_action_sdk::parse_config(Some(&unsafe_backup)).unwrap();
    let error = validate_arch(&config).unwrap_err();
    assert_eq!(error.code.key, "package-path-invalid");
    assert_eq!(
        evin_locales::render_issue(&error.code),
        evin_locales::t!("package-path-invalid", path = "../etc/app.conf")
    );
}

#[test]
fn package_metadata_and_script_paths_use_fluent_errors() {
    let bad_name = evin_flow::parse_yaml(
        "output_dir: dist\narchitecture: x86_64\nrelease: ${bump.release}\nlicense: MIT\npackage: { name: 'Bad Name', summary: app, maintainer: A }\n",
    )
    .unwrap();
    let config: ArchConfig = evin_action_sdk::parse_config(Some(&bad_name)).unwrap();
    let error = validate_arch(&config).unwrap_err();
    assert_eq!(error.code.key, "archive-package-name-invalid");
    assert_eq!(
        evin_locales::render_issue(&error.code),
        evin_locales::t!("archive-package-name-invalid")
    );

    let bad_script = evin_flow::parse_yaml(
        "output_dir: dist\narchitecture: x86_64\nrelease: ${bump.release}\nlicense: MIT\npackage: { name: app, summary: app, maintainer: A }\nscripts: { install: ' ' }\n",
    )
    .unwrap();
    let config: ArchConfig = evin_action_sdk::parse_config(Some(&bad_script)).unwrap();
    let error = validate_arch(&config).unwrap_err();
    assert_eq!(error.code.key, "archive-package-script-path-empty");
    assert_eq!(
        evin_locales::render_issue(&error.code),
        evin_locales::t!("archive-package-script-path-empty")
    );

    let bad_description = evin_flow::parse_yaml(
        "output_dir: dist\narchitecture: x86_64\nrelease: ${bump.release}\npackage: { name: app, summary: app, maintainer: A }\ndescription: \"two\\nlines\"\n",
    )
    .unwrap();
    let config: DebConfig = evin_action_sdk::parse_config(Some(&bad_description)).unwrap();
    let error = validate_deb(&config).unwrap_err();
    assert_eq!(error.code.key, "archive-package-metadata-invalid");
    assert_eq!(
        evin_locales::render_issue(&error.code),
        evin_locales::t!("archive-package-metadata-invalid", field = "description")
    );
}

#[test]
fn missing_package_script_uses_its_fluent_diagnostic() {
    let temp = tempfile::TempDir::new().unwrap();
    let ctx = super::context_with_env(temp.path(), std::collections::HashMap::new());
    let path = "missing-install-script".to_string();
    let error =
        crate::v1::packages::load_scripts(&ctx, vec![("install", Some(&path))]).unwrap_err();
    assert_eq!(error.code.key, "archive-package-script-read-failed");
    let details = &error.code.args[0].1;
    assert_eq!(
        evin_locales::render_issue(&error.code),
        evin_locales::t!("archive-package-script-read-failed", error = details)
    );
}

#[test]
fn architecture_and_package_schema_are_strict() {
    let unsupported = evin_flow::parse_yaml(
        "output_dir: dist\narchitecture: amd64\nrelease: ${bump.release}\npackage: { name: app, summary: app, maintainer: A }\n",
    )
    .unwrap();
    assert!(validate_deb_config(Some(&unsupported)).is_err());

    let package_with_target_union = evin_flow::parse_yaml(
        "output_dir: dist\narchitecture: x86_64\nrelease: ${bump.release}\npackage: { name: app, summary: app, maintainer: A, targets: { deb: {} } }\n",
    )
    .unwrap();
    assert!(validate_deb_config(Some(&package_with_target_union)).is_err());
}
