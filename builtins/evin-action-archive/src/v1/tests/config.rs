// Copyright (C) 2026 Eduardo Marques Santos
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License, version 3.

use super::*;

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
