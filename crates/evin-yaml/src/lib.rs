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

pub mod de;
pub mod de_enum;
pub mod header;
pub mod template;

use std::fmt::Display;
use yaml_rust2::parser::{Event, Parser};
use yaml_rust2::scanner::Marker;

pub use evin_diagnostic::{Span, Spanned};
pub use header::{EVIN_HEADER_V1, EvinHeader, inspect_evin_header, validate_evin_header};

fn marker_to_span(marker: Marker) -> Span {
    Span {
        line: marker.line(),
        column: marker.col() + 1,
        offset: marker.index(),
        length: 1,
    }
}

use serde::{Deserialize, Serialize};

/// The core AST for parsed YAML containing span information.
#[derive(Debug, Clone, Serialize)]
pub enum SpannedValue {
    Null(Span),
    Bool(bool, Span),
    Integer(i64, Span),
    Float(f64, Span),
    String(String, Span),
    Sequence(Vec<SpannedValue>, Span),
    Mapping(Vec<(SpannedValue, SpannedValue)>, Span),
}

impl<'de> Deserialize<'de> for SpannedValue {
    fn deserialize<D>(deserializer: D) -> Result<SpannedValue, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        struct SpannedValueVisitor;

        impl<'de> serde::de::Visitor<'de> for SpannedValueVisitor {
            type Value = SpannedValue;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a raw spanned value")
            }

            fn visit_unit<E>(self) -> Result<SpannedValue, E>
            where
                E: serde::de::Error,
            {
                crate::de::SPANNED_VALUE_EXCHANGE.with(|ex| {
                    ex.borrow_mut()
                        .take()
                        .ok_or_else(|| E::custom("missing spanned value from exchange"))
                })
            }
        }

        deserializer.deserialize_struct(
            crate::de::SPANNED_VALUE_MAGIC_STRUCT,
            &[],
            SpannedValueVisitor,
        )
    }
}

impl SpannedValue {
    pub fn span(&self) -> Span {
        match self {
            Self::Null(span) => *span,
            Self::Bool(_, span) => *span,
            Self::Integer(_, span) => *span,
            Self::Float(_, span) => *span,
            Self::String(_, span) => *span,
            Self::Sequence(_, span) => *span,
            Self::Mapping(_, span) => *span,
        }
    }
}

#[derive(Debug)]
pub struct ConfigError {
    pub message: String,
    pub span: Option<Span>,
}

#[derive(Debug, Clone, Default)]
pub struct ConfigReport {
    pub overrides: Vec<String>,
    pub defaults: Vec<String>,
}

impl Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(span) = self.span {
            write!(f, "{} at line {}:{}", self.message, span.line, span.column)
        } else {
            write!(f, "{}", self.message)
        }
    }
}

impl std::error::Error for ConfigError {}

impl ConfigError {
    pub fn map_message(self, map: impl FnOnce(String) -> String) -> Self {
        Self {
            message: map(self.message),
            span: self.span,
        }
    }

    pub fn into_diagnostic(
        self,
        _title: impl Into<String>,
        path: Option<String>,
    ) -> evin_diagnostic::Diagnostic {
        let mut diag = evin_locales::diagnostic(
            evin_diagnostic::IssueCode::new("issue-yaml-parse-failed").arg("error", self.message),
        );
        if let Some(span) = self.span
            && let Some(p) = path
        {
            if span.length > 1 {
                diag.location = Some(evin_diagnostic::FileLocation::ByteSpan {
                    path: p,
                    offset: span.offset,
                    length: span.length,
                    label: None,
                });
            } else {
                diag = diag.with_file_location(p, Some(span.line), Some(span.column));
            }
        }
        diag
    }
}

pub fn parse_yaml(input: &str) -> Result<SpannedValue, ConfigError> {
    let mut parser = Parser::new(input.chars());
    let mut stack = Vec::new();

    loop {
        let (event, marker) = parser.next_token().map_err(|e| ConfigError {
            message: format!("Parse error: {:?}", e),
            span: None,
        })?;

        let span = marker_to_span(marker);

        match event {
            Event::StreamStart | Event::DocumentStart => {}
            Event::StreamEnd => break,
            Event::DocumentEnd => {
                if stack.len() == 1 {
                    return Ok(stack.pop().unwrap());
                }
            }
            Event::Scalar(value, _style, _anchor, _tag) => {
                let mut span = span;
                span.length = value.len().max(1);
                // Determine true type of scalar
                let v = if value == "null" || value == "~" {
                    SpannedValue::Null(span)
                } else if value == "true" {
                    SpannedValue::Bool(true, span)
                } else if value == "false" {
                    SpannedValue::Bool(false, span)
                } else if let Ok(i) = value.parse::<i64>() {
                    SpannedValue::Integer(i, span)
                } else if let Ok(f) = value.parse::<f64>() {
                    SpannedValue::Float(f, span)
                } else {
                    SpannedValue::String(value, span)
                };

                push_value(&mut stack, v);
            }
            Event::SequenceStart(_idx, _anchor) => {
                stack.push(SpannedValue::Sequence(Vec::new(), span));
            }
            Event::SequenceEnd => {
                let seq = stack.pop().unwrap();
                push_value(&mut stack, seq);
            }
            Event::MappingStart(_idx, _anchor) => {
                stack.push(SpannedValue::Mapping(Vec::new(), span));
            }
            Event::MappingEnd => {
                // In stack, we should have a Mapping waiting, but push_value will have pushed alternating keys and values.
                // Wait, push_value implementation for Mapping below needs to be careful.
                let map = stack.pop().unwrap();
                push_value(&mut stack, map);
            }
            _ => {}
        }
    }

    if stack.len() == 1 {
        Ok(stack.pop().unwrap())
    } else {
        Err(ConfigError {
            message: "Unexpected end of input".into(),
            span: None,
        })
    }
}

#[allow(clippy::result_large_err)]
pub trait YamlDocument: serde::de::DeserializeOwned {
    fn from_str(
        content: &str,
        file_path: Option<&std::path::Path>,
    ) -> Result<Self, evin_diagnostic::Diagnostic> {
        validate_evin_header(content, file_path)?;
        let ast = parse_yaml(content).map_err(|e| {
            e.into_diagnostic(
                "YAML Syntax Error",
                file_path.map(|p| p.display().to_string()),
            )
        })?;

        crate::de::from_spanned_value(ast).map_err(|e| {
            e.into_diagnostic(
                "Configuration Error",
                file_path.map(|p| p.display().to_string()),
            )
        })
    }

    fn from_file(file_path: &std::path::Path) -> Result<Self, evin_diagnostic::Diagnostic> {
        let content = std::fs::read_to_string(file_path).map_err(|e| {
            evin_locales::diagnostic(
                evin_diagnostic::IssueCode::new("issue-file-parse-failed")
                    .arg("file_path", file_path.display().to_string())
                    .arg("error", e.to_string()),
            )
        })?;
        Self::from_str(&content, Some(file_path))
    }
}

fn push_value(stack: &mut Vec<SpannedValue>, value: SpannedValue) {
    if stack.is_empty() {
        stack.push(value);
        return;
    }
    let last_idx = stack.len() - 1;
    match &mut stack[last_idx] {
        SpannedValue::Sequence(seq, _) => {
            seq.push(value);
        }
        SpannedValue::Mapping(map, _) => {
            // A mapping needs pairs.
            // If the last pair is missing a value, add it.
            // If the last pair is complete or map is empty, add a new key with Null value temporarily.
            if map.is_empty() {
                map.push((value, SpannedValue::Null(Span::default())));
            } else {
                let last_pair_idx = map.len() - 1;
                // We use Null(default) as a placeholder for missing value.
                if let SpannedValue::Null(span) = map[last_pair_idx].1
                    && span.offset == 0
                    && span.line == 1
                {
                    map[last_pair_idx].1 = value;
                    return;
                }
                map.push((value, SpannedValue::Null(Span::default())));
            }
        }
        _ => {
            // Document root
            stack.push(value);
        }
    }
}
