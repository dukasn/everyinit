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

macro_rules! assert_fluent_error {
    ($config:expr, $key:expr, $expected:expr) => {{
        let error = $config.validate().unwrap_err();
        assert_eq!(error.code.key, $key);
        assert_eq!(evin_locales::render_issue(&error.code), $expected);
    }};
}

#[test]
fn removed_cargo_option_is_rejected() {
    assert!(VersionBumperConfig::from_with(None).is_ok());
    for value in ["true", "false"] {
        let with = evin_flow::parse_yaml(&format!("cargo_pre_1_0: {value}\n")).unwrap();
        assert!(VersionBumperConfig::from_with(Some(with)).is_err());
    }
}

#[test]
fn initial_version_error_uses_fluent_message() {
    let config = VersionBumperConfig {
        initial_version: "invalid".to_string(),
        ..VersionBumperConfig::default()
    };
    let error = config.validate().unwrap_err();
    let detail = semver::Version::parse("invalid").unwrap_err().to_string();
    let expected = evin_locales::t!("initial-version-invalid", error = detail);
    assert!(
        !expected.starts_with("<missing locale key:"),
        "expected Fluent message to exist: {expected}"
    );
    assert_eq!(evin_locales::render_issue(&error.code), expected);
}

#[test]
fn config_error_conversion_preserves_fluent_rendering() {
    let with = evin_flow::parse_yaml("initial_version: invalid\n").unwrap();
    let detail = semver::Version::parse("invalid").unwrap_err().to_string();

    let error = VersionBumperConfig::from_with(Some(with)).unwrap_err();
    assert_eq!(
        error.message,
        evin_locales::t!("initial-version-invalid", error = detail)
    );
}

#[test]
fn configuration_validation_errors_use_fluent_messages() {
    assert_fluent_error!(
        VersionBumperConfig {
            stable: true,
            prerelease: Some("rc".into()),
            ..VersionBumperConfig::default()
        },
        "stable-and-prerelease-conflict",
        evin_locales::t!("stable-and-prerelease-conflict")
    );
    assert_fluent_error!(
        VersionBumperConfig {
            stable: true,
            channel: Some("canary".into()),
            ..VersionBumperConfig::default()
        },
        "stable-and-channel-conflict",
        evin_locales::t!("stable-and-channel-conflict")
    );
    assert_fluent_error!(
        VersionBumperConfig {
            prerelease: Some("rc".into()),
            channel: Some("canary".into()),
            ..VersionBumperConfig::default()
        },
        "prerelease-and-channel-conflict",
        evin_locales::t!("prerelease-and-channel-conflict")
    );
    assert_fluent_error!(
        VersionBumperConfig {
            prerelease: Some("  ".into()),
            ..VersionBumperConfig::default()
        },
        "prerelease-must-not-be-empty",
        evin_locales::t!("prerelease-must-not-be-empty")
    );

    let prerelease = VersionBumperConfig {
        prerelease: Some("bad tag".into()),
        ..VersionBumperConfig::default()
    };
    let prerelease_error = semver::Prerelease::new("bad tag.1")
        .unwrap_err()
        .to_string();
    assert_fluent_error!(
        prerelease,
        "prerelease-invalid",
        evin_locales::t!("prerelease-invalid", error = prerelease_error)
    );
    assert_fluent_error!(
        VersionBumperConfig {
            git: GitConfig {
                tag: true,
                ..GitConfig::default()
            },
            ..VersionBumperConfig::default()
        },
        "git-tag-requires-git-commit",
        evin_locales::t!("git-tag-requires-git-commit")
    );
    assert_fluent_error!(
        VersionBumperConfig {
            git: GitConfig {
                defer: true,
                ..GitConfig::default()
            },
            ..VersionBumperConfig::default()
        },
        "git-defer-requires-commit-and-tag",
        evin_locales::t!("git-defer-requires-commit-and-tag")
    );
    assert_fluent_error!(
        VersionBumperConfig {
            read_only: true,
            files: vec![FileConfig::Auto("Cargo.toml".into())],
            ..VersionBumperConfig::default()
        },
        "read-only-conflicts-with-mutations",
        evin_locales::t!("read-only-conflicts-with-mutations")
    );

    let fixed_version_error = semver::Version::parse("invalid").unwrap_err().to_string();
    assert_fluent_error!(
        VersionBumperConfig {
            fixed_version: Some("invalid".into()),
            ..VersionBumperConfig::default()
        },
        "fixed-version-invalid",
        evin_locales::t!("fixed-version-invalid", error = fixed_version_error)
    );
    assert_fluent_error!(
        VersionBumperConfig {
            fixed_version: Some("1.2.3".into()),
            ..VersionBumperConfig::default()
        },
        "fixed-version-requires-read-only",
        evin_locales::t!("fixed-version-requires-read-only")
    );
    assert_fluent_error!(
        VersionBumperConfig {
            git: GitConfig {
                message: Some("  ".into()),
                ..GitConfig::default()
            },
            ..VersionBumperConfig::default()
        },
        "git-message-must-not-be-empty",
        evin_locales::t!("git-message-must-not-be-empty")
    );
    assert_fluent_error!(
        VersionBumperConfig {
            git: GitConfig {
                include: vec!["  ".into()],
                ..GitConfig::default()
            },
            ..VersionBumperConfig::default()
        },
        "git-include-paths-must-not-be-empty",
        evin_locales::t!("git-include-paths-must-not-be-empty")
    );

    assert_fluent_error!(
        VersionBumperConfig {
            files: vec![FileConfig::Auto("  ".into())],
            ..VersionBumperConfig::default()
        },
        "file-path-must-not-be-empty",
        evin_locales::t!("file-path-must-not-be-empty")
    );
    assert_fluent_error!(
        VersionBumperConfig {
            files: vec![FileConfig::Auto("release.txt".into())],
            ..VersionBumperConfig::default()
        },
        "automatic-version-detection-unsupported",
        evin_locales::t!(
            "automatic-version-detection-unsupported",
            path = "release.txt"
        )
    );
    let error = VersionBumperConfig {
        files: vec![FileConfig::Custom(CustomFileConfig {
            path: "release.txt".into(),
            match_pattern: "[".into(),
            write: "${version}".into(),
        })],
        ..VersionBumperConfig::default()
    }
    .validate()
    .unwrap_err();
    assert_eq!(error.code.key, "custom-file-match-invalid");
    let detail = &error.code.args[0].1;
    assert_eq!(
        evin_locales::render_issue(&error.code),
        evin_locales::t!("custom-file-match-invalid", error = detail)
    );
    for (config, key, expected) in [
        (
            CustomFileConfig {
                path: " ".into(),
                match_pattern: "v${previous_version}".into(),
                write: "${version}".into(),
            },
            "custom-file-path-must-not-be-empty",
            evin_locales::t!("custom-file-path-must-not-be-empty"),
        ),
        (
            CustomFileConfig {
                path: "release.txt".into(),
                match_pattern: " ".into(),
                write: "${version}".into(),
            },
            "custom-file-match-must-not-be-empty",
            evin_locales::t!("custom-file-match-must-not-be-empty"),
        ),
        (
            CustomFileConfig {
                path: "release.txt".into(),
                match_pattern: "v${previous_version}".into(),
                write: " ".into(),
            },
            "custom-file-write-must-not-be-empty",
            evin_locales::t!("custom-file-write-must-not-be-empty"),
        ),
    ] {
        assert_fluent_error!(
            VersionBumperConfig {
                files: vec![FileConfig::Custom(config)],
                ..VersionBumperConfig::default()
            },
            key,
            expected
        );
    }
}

#[test]
fn invalid_template_syntax_uses_fluent_diagnostic() {
    let error = super::super::config::validate_tokens("${version", &["version"]).unwrap_err();
    assert_eq!(error.code.key, "version-bumper-template-syntax-invalid");
    let expected = evin_locales::t!(
        "version-bumper-template-syntax-invalid",
        error = evin_flow::template::pipeline::validate_template_syntax_with_self(
            "${version",
            &["version"]
        )
        .unwrap_err()
        .to_string()
    );
    assert_eq!(evin_locales::render_issue(&error.code), expected);
}

#[test]
fn unsupported_template_variable_uses_fluent_diagnostic() {
    let error = super::super::config::validate_tokens("${bump.release}", &["version"]).unwrap_err();
    assert_eq!(error.code.key, "issue-version-bumper-unsupported-template");
    assert_eq!(
        evin_locales::render_issue(&error.code),
        evin_locales::t!(
            "issue-version-bumper-unsupported-template",
            template = "bump.release"
        )
    );
}
