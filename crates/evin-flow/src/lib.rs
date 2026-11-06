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
pub mod schema;
pub mod template;

use std::fmt::Display;
use yaml_rust2::parser::{Event, Parser};
use yaml_rust2::scanner::{Marker, TScalarStyle};

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

fn scalar_source_length(input: &str, start: usize, value: &str, style: TScalarStyle) -> usize {
    let quote = match style {
        TScalarStyle::SingleQuoted => '\'',
        TScalarStyle::DoubleQuoted => '"',
        _ => return value.chars().count().max(1),
    };
    let mut source = input.chars().skip(start).peekable();
    if source.next() != Some(quote) {
        return value.chars().count().max(1);
    }
    let mut length = 1;
    let mut escaped = false;
    while let Some(character) = source.next() {
        length += 1;
        if quote == '"' && !escaped && character == '\\' {
            escaped = true;
            continue;
        }
        if character == quote && !escaped {
            if quote == '\'' && source.peek() == Some(&'\'') {
                source.next();
                length += 1;
                continue;
            }
            return length;
        }
        escaped = false;
    }
    value.chars().count().max(1)
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

pub fn parse_flow(input: &str) -> Result<SpannedValue, ConfigError> {
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
            Event::Scalar(value, style, _anchor, _tag) => {
                let mut span = span;
                span.length = scalar_source_length(input, span.offset, &value, style);
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
                let mut seq = stack.pop().unwrap();
                if let SpannedValue::Sequence(_, start) = &mut seq {
                    start.length = span.offset.saturating_sub(start.offset).max(1);
                }
                push_value(&mut stack, seq);
            }
            Event::MappingStart(_idx, _anchor) => {
                stack.push(SpannedValue::Mapping(Vec::new(), span));
            }
            Event::MappingEnd => {
                let mut map = stack.pop().unwrap();
                if let SpannedValue::Mapping(_, start) = &mut map {
                    start.length = span.offset.saturating_sub(start.offset).max(1);
                }
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

pub use parse_flow as parse_yaml;

#[allow(clippy::result_large_err)]
pub trait FlowDocument: serde::de::DeserializeOwned {
    fn from_str(
        content: &str,
        file_path: Option<&std::path::Path>,
    ) -> Result<Self, evin_diagnostic::Diagnostic> {
        validate_evin_header(content, file_path)?;
        let ast = parse_flow(content).map_err(|e| {
            e.into_diagnostic(
                "Flow Syntax Error",
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

pub use FlowDocument as YamlDocument;

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

/// Return the byte index within `line` where a YAML comment starts, if any.
///
/// The scan is line-local: a quote only opens a quoted scalar when it is the
/// first character of a scalar (after indentation, `- `, `: `, `[`, `{`, or
/// `,`). Apostrophes inside plain scalars such as `Don't` are therefore
/// ordinary characters and cannot hide later comments.
pub fn comment_start_in_line(line: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    let mut index = 0;
    // `true` when the next non-blank character would begin a new scalar.
    let mut at_scalar_start = true;
    while index < bytes.len() {
        let byte = bytes[index];
        match byte {
            b' ' | b'\t' => {
                index += 1;
                continue;
            }
            b'#' if index == 0 || matches!(bytes[index - 1], b' ' | b'\t' | b':') => {
                return Some(index);
            }
            b'\'' | b'"' if at_scalar_start => {
                // Skip the quoted scalar. An unterminated quote continues on
                // a following line, so nothing after it on this line can be a
                // comment.
                let quote = byte;
                index += 1;
                loop {
                    let &current = bytes.get(index)?;
                    index += 1;
                    if quote == b'"' && current == b'\\' {
                        index += 1;
                    } else if current == quote {
                        if quote == b'\'' && bytes.get(index) == Some(&b'\'') {
                            index += 1;
                        } else {
                            break;
                        }
                    }
                }
                at_scalar_start = false;
                continue;
            }
            b'[' | b'{' | b',' => at_scalar_start = true,
            b'-' | b':' | b'?'
                if bytes
                    .get(index + 1)
                    .is_none_or(|next| matches!(next, b' ' | b'\t')) =>
            {
                at_scalar_start = true
            }
            _ => at_scalar_start = false,
        }
        index += 1;
    }
    None
}

/// Returns true if the given byte `offset` in `input` falls inside a YAML
/// comment, including the indentation of a comment-only line.
///
/// This is a lexical check on the offset's own line. Callers holding a parsed
/// AST should additionally treat offsets inside scalar spans as non-comments,
/// which covers `#` lines inside multi-line quoted and block scalars.
pub fn is_comment_at(input: &str, offset: usize) -> bool {
    if offset > input.len() || !input.is_char_boundary(offset) {
        return false;
    }
    let line_start = input[..offset].rfind('\n').map_or(0, |index| index + 1);
    let line_end = input[offset..]
        .find('\n')
        .map_or(input.len(), |index| offset + index);
    let line = input[line_start..line_end].trim_end_matches('\r');
    let Some(start) = comment_start_in_line(line) else {
        return false;
    };
    let comment_only = line[..start].trim().is_empty();
    let start = if comment_only { 0 } else { start };
    offset >= line_start + start
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_comment_at() {
        let text = "#!evin/v1\nversion: 1\ncommits:\n  # The conventional commit format\n  message_format: \"{type} # not comment\"\n  policies:\n    require_body: false # inline comment\n";
        // Header
        assert!(is_comment_at(text, 0));
        assert!(is_comment_at(text, 5));
        assert!(is_comment_at(text, 9));

        // Code: version: 1
        let v_offset = text.find("version").unwrap();
        assert!(!is_comment_at(text, v_offset));
        let one_offset = text.find("version: 1").unwrap() + "version: ".len();
        assert!(!is_comment_at(text, one_offset));

        // Full line comment (including indentation)
        let comment_line = text.find("  # The").unwrap();
        assert!(is_comment_at(text, comment_line)); // spaces before '#'
        assert!(is_comment_at(text, comment_line + 2)); // '#'
        assert!(is_comment_at(text, comment_line + 5)); // 'The'

        // String containing '#'
        let inside_str = text.find("# not comment").unwrap();
        assert!(!is_comment_at(text, inside_str));

        // Inline comment
        let req_offset = text.find("require_body").unwrap();
        assert!(!is_comment_at(text, req_offset));
        let false_offset = text.find("false").unwrap();
        assert!(!is_comment_at(text, false_offset));
        let inline_hash = text.find("# inline comment").unwrap();
        assert!(is_comment_at(text, inline_hash));
        assert!(is_comment_at(text, inline_hash + 5));
    }

    #[test]
    fn apostrophes_in_plain_scalars_do_not_hide_later_comments() {
        let text = "name: Don't panic\nnote: it's \"fine\n  # after\nlist: [a, 'b # c', \"d\"] # tail\nhash: a#b # real\nquoted: 'it''s # in' # out\n";
        let at = |needle: &str| text.find(needle).unwrap();
        assert!(is_comment_at(text, at("# after")));
        assert!(is_comment_at(text, at("# after") - 2)); // indentation
        assert!(!is_comment_at(text, at("Don't")));
        assert!(!is_comment_at(text, at("# c")));
        assert!(is_comment_at(text, at("# tail")));
        assert!(!is_comment_at(text, at("a#b") + 1));
        assert!(is_comment_at(text, at("# real")));
        assert!(!is_comment_at(text, at("# in")));
        assert!(is_comment_at(text, at("# out")));
        assert!(is_comment_at(text, at("# out") + "# out".len())); // line end
    }
}
