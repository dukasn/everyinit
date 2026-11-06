// Copyright (C) 2026 Eduardo Marques Santos
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 of the License.

//! Evin's optional source-level YAML document header.

use crate::{ConfigError, Span};
use std::path::Path;

/// The current Evin configuration document marker.
pub const EVIN_HEADER_V1: &str = "#!evin/v1";

/// A recognized Evin document header. The header remains a YAML comment and
/// therefore never changes the document's deserialized configuration shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvinHeader {
    pub version: u8,
    pub span: Span,
}

/// Inspect the first physical line of a YAML source for an optional Evin
/// document header. A UTF-8 BOM is allowed before the marker.
pub fn inspect_evin_header(input: &str) -> Result<Option<EvinHeader>, ConfigError> {
    let (offset, input) = input
        .strip_prefix('\u{feff}')
        .map_or((0, input), |remaining| ("\u{feff}".len(), remaining));
    let line = input.split_once('\n').map_or(input, |(line, _)| line);
    let line = line.strip_suffix('\r').unwrap_or(line);

    if line == EVIN_HEADER_V1 {
        return Ok(Some(EvinHeader {
            version: 1,
            span: Span {
                line: 1,
                column: 1,
                offset,
                length: EVIN_HEADER_V1.len(),
            },
        }));
    }

    if line.starts_with("#!evin") {
        return Err(ConfigError {
            message: format!(
                "unsupported Evin document header '{line}'; expected {EVIN_HEADER_V1}"
            ),
            span: Some(Span {
                line: 1,
                column: 1,
                offset,
                length: line.len().max(1),
            }),
        });
    }

    Ok(None)
}

/// Validate the optional header and attach any finding to its source document.
#[allow(clippy::result_large_err)]
pub fn validate_evin_header(
    input: &str,
    path: Option<&Path>,
) -> Result<Option<EvinHeader>, evin_diagnostic::Diagnostic> {
    inspect_evin_header(input).map_err(|error| {
        let mut diagnostic = evin_locales::diagnostic(
            evin_diagnostic::IssueCode::new("issue-evin-header-invalid")
                .arg("error", error.message),
        );
        if let Some(path) = path
            && let Some(span) = error.span
        {
            diagnostic = diagnostic.with_file_span(
                path.display().to_string(),
                span.offset,
                span.length,
                None,
            );
        }
        diagnostic
    })
}

#[cfg(test)]
mod tests {
    use super::{EVIN_HEADER_V1, inspect_evin_header, validate_evin_header};
    use std::path::Path;

    #[test]
    fn recognizes_an_exact_v1_header() {
        let header = inspect_evin_header("#!evin/v1\nworkspace: {}\n")
            .unwrap()
            .unwrap();
        assert_eq!(header.version, 1);
        assert_eq!(header.span.offset, 0);
        assert_eq!(header.span.length, EVIN_HEADER_V1.len());
    }

    #[test]
    fn recognizes_bom_and_crlf_without_changing_the_marker_span() {
        let header = inspect_evin_header("\u{feff}#!evin/v1\r\nworkspace: {}\n")
            .unwrap()
            .unwrap();
        assert_eq!(header.span.offset, 3);
        assert_eq!(header.span.length, EVIN_HEADER_V1.len());
    }

    #[test]
    fn leaves_headerless_yaml_unmarked() {
        assert!(
            inspect_evin_header("# a normal comment\nworkspace: {}\n")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn rejects_an_unsupported_evin_looking_header_at_its_source_span() {
        let error = inspect_evin_header("#!evin/v2\nworkspace: {}\n").unwrap_err();
        let span = error.span.unwrap();
        assert_eq!(span.offset, 0);
        assert_eq!(span.length, "#!evin/v2".len());
        assert!(error.message.contains("#!evin/v2"));
    }

    #[test]
    fn validation_exposes_the_header_span_as_a_diagnostic() {
        let diagnostic =
            validate_evin_header("#!evin/v2\n", Some(Path::new("config.flow"))).unwrap_err();
        assert_eq!(diagnostic.code.key, "issue-evin-header-invalid");
        assert!(matches!(
            diagnostic.location,
            Some(evin_diagnostic::FileLocation::ByteSpan {
                offset: 0,
                length: 9,
                ..
            })
        ));
    }
}
