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

#[test]
fn formats_reject_each_others_keys() {
    let tar_with_package =
        evin_flow::parse_yaml("path: out.tar.gz\ninclude: [x]\npackage: { name: app }\n").unwrap();
    assert!(validate_config(Some(&tar_with_package)).is_err());
    let package_with_path = evin_flow::parse_yaml(
            "format: deb\npath: out.deb\noutput_dir: dist\narchitecture: x86_64\nrelease: ${bump.release}\npackage: { name: app, summary: app, maintainer: A, license: MIT, targets: { deb: {} } }\n",
        ).unwrap();
    assert!(validate_config(Some(&package_with_path)).is_err());
    let configured_default = evin_flow::parse_yaml(
        "format: deb\noutput_dir: dist\narchitecture: x86_64\nlayout: flat\n",
    )
    .unwrap();
    assert!(validate_config(Some(&configured_default)).is_err());
}

#[test]
fn package_release_reference_must_be_a_declared_need() {
    let with = evin_flow::parse_yaml("format: deb\noutput_dir: dist\narchitecture: x86_64\npackage: { name: app, summary: app, maintainer: A, license: MIT, targets: { deb: {} } }\nrelease: ${bump.release}\n").unwrap();
    let missing = evin_action_sdk::ActionValidationContext {
        task_name: "package",
        needs: &[],
    };
    assert!(validate_task_config(&missing, Some(&with)).is_err());
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
    assert!(validate_package(config.package.as_ref().unwrap(), PackageFormat::Arch).is_err());

    let unsafe_backup = evin_flow::parse_yaml(
            "format: arch\noutput_dir: dist\narchitecture: x86_64\nrelease: ${bump.release}\npackage: { name: app, summary: app, maintainer: A, license: MIT, targets: { arch: { backup: ['../etc/app.conf'] } } }\n",
        )
        .unwrap();
    let config: ArchiveConfig = evin_action_sdk::parse_config(Some(&unsafe_backup)).unwrap();
    assert!(validate_package(config.package.as_ref().unwrap(), PackageFormat::Arch).is_err());
}
