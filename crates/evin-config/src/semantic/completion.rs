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

fn namespace_path_candidate(source: &dyn WorkspaceSource, path: &Path) -> bool {
    let supported_extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| matches!(extension, "flow" | "yaml" | "yml"));
    supported_extension
        && source
            .read_to_string(path)
            .ok()
            .is_some_and(|text| evin_yaml::validate_evin_header(&text, Some(path)).is_ok())
}

fn template_component_context(
    document: &WorkspaceDocument,
    offset: usize,
) -> Option<TemplateComponentContext> {
    let text = document.text.as_str();
    let cursor = offset.min(text.len());
    let open = text[..cursor].rfind("${")?;
    let body_start = open + 2;
    if text[body_start..cursor].contains('}') {
        return None;
    }
    let body_end = text[body_start..]
        .find('}')
        .map(|distance| body_start + distance)
        .unwrap_or_else(|| {
            text[body_start..]
                .find(|character: char| character.is_whitespace() || matches!(character, '\'' | '"' | ',' | ']'))
                .map(|distance| body_start + distance)
                .unwrap_or(text.len())
        });
    if cursor > body_end {
        return None;
    }
    let before_cursor = &text[body_start..cursor];
    let (kind, qualifier, token_start, prefix) = if let Some(env_prefix) = before_cursor.strip_prefix("env.") {
        (
            TemplateComponentKind::Environment,
            "env.".to_owned(),
            body_start + 4,
            env_prefix.rsplit('.').next().unwrap_or_default().to_owned(),
        )
    } else if let Some(dot) = before_cursor.rfind('.') {
        let qualifier = before_cursor[..=dot].to_owned();
        (
            TemplateComponentKind::TaskOutput,
            qualifier,
            body_start + dot + 1,
            before_cursor[dot + 1..].to_owned(),
        )
    } else {
        (
            TemplateComponentKind::Unqualified,
            String::new(),
            body_start,
            before_cursor.to_owned(),
        )
    };
    let mut token_end = cursor;
    while token_end < body_end
        && !matches!(text.as_bytes()[token_end], b'.' | b'}' | b',' | b']' | b'\'' | b'"')
        && !text.as_bytes()[token_end].is_ascii_whitespace()
    {
        token_end += 1;
    }
    Some(TemplateComponentContext {
        kind,
        qualifier,
        prefix,
        replacement: SourceRange {
            path: document.path.clone(),
            start: token_start,
            end: token_end,
        },
    })
}

fn source_slice<'a>(
    documents: &'a std::collections::BTreeMap<std::path::PathBuf, WorkspaceDocument>,
    path: &Path,
    range: &SourceRange,
) -> &'a str {
    documents
        .get(path)
        .and_then(|document| document.text.get(range.start..range.end))
        .unwrap_or_default()
}

fn declared_name_in_scope(symbols: &[SemanticSymbol], scope: &str, short: &str) -> bool {
    symbols.iter().any(|symbol| {
        symbol.kind == SemanticKind::Task && symbol.name == format!("{scope}:{short}")
    })
}

fn completion_word_range(
    document: &WorkspaceDocument,
    offset: usize,
    mapping_key: bool,
) -> SourceRange {
    let text = document.text.as_str();
    let offset = offset.min(text.len());
    let mut start = offset;
    let mut end = offset;
    let delimiter = |byte: u8| {
        byte.is_ascii_whitespace()
            || matches!(byte, b',' | b'[' | b']' | b'{' | b'}' | b'\'' | b'"')
            || (mapping_key && byte == b':')
    };
    while start > 0 && !delimiter(text.as_bytes()[start - 1]) {
        start -= 1;
    }
    while end < text.len() && !delimiter(text.as_bytes()[end]) {
        end += 1;
    }
    SourceRange {
        path: document.path.clone(),
        start,
        end,
    }
}

fn mapping_value_precedes_cursor(text: &str, offset: usize) -> bool {
    let line_start = text[..offset.min(text.len())]
        .rfind('\n')
        .map_or(0, |index| index + 1);
    let before = &text[line_start..offset.min(text.len())];
    let mut quoted = None;
    let mut escaped = false;
    for character in before.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        if character == '\\' && quoted == Some('"') {
            escaped = true;
            continue;
        }
        match (quoted, character) {
            (Some(current), value) if current == value => quoted = None,
            (None, '\'' | '"') => quoted = Some(character),
            (None, ':') => return true,
            _ => {}
        }
    }
    false
}

fn mapping_key_has_colon(text: &str, offset: usize) -> bool {
    text.get(offset..)
        .unwrap_or_default()
        .lines()
        .next()
        .unwrap_or_default()
        .trim_start()
        .starts_with(':')
}

fn action_field_candidates(
    descriptor: &ActionDescriptor,
    invocation: &ActionInvocation,
    path: &Path,
    offset: usize,
) -> Vec<CompletionCandidate> {
    let containing = invocation
        .fields
        .iter()
        .filter(|(_, range)| range.contains_inclusive(path, offset))
        .max_by_key(|(field, _)| field.matches('.').count())
        .map(|(field, _)| field.as_str());
    let prefix = containing
        .map(|field| {
            let nested = format!("{field}.");
            if descriptor
                .fields
                .iter()
                .any(|candidate| candidate.path.starts_with(&nested))
            {
                nested
            } else {
                field
                    .rsplit_once('.')
                    .map(|(parent, _)| format!("{parent}."))
                    .unwrap_or_default()
            }
        })
        .unwrap_or_default();
    descriptor
        .fields
        .iter()
        .filter(|field| condition_matches(field, invocation.format.as_deref()))
        .filter(|field| {
            field
                .path
                .strip_prefix(&prefix)
                .is_some_and(|rest| !rest.contains('.'))
        })
        .filter(|field| {
            !invocation
                .fields
                .iter()
                .any(|(existing, _)| existing == field.path)
        })
        .map(|field| CompletionCandidate {
            label: field.path.into(),
            detail: action_field_detail(field),
            documentation: field.description.into(),
            snippet: field.snippet(),
            catalog_target: Some(CatalogTarget::action_field(
                &invocation.action_id,
                field.path,
            )),
            replacement: None,
            filter_text: None,
        })
        .collect()
}

fn condition_matches(field: &ActionFieldDescriptor, format: Option<&str>) -> bool {
    let Some(condition) = field.condition else {
        return true;
    };
    let Some((key, choices)) = condition.split_once('=') else {
        return true;
    };
    key != "format" || format.is_some_and(|value| choices.split('|').any(|choice| choice == value))
}

fn action_field_detail(field: &ActionFieldDescriptor) -> String {
    let required = if field.required {
        "required"
    } else {
        "optional"
    };
    format!(
        "{} {required}",
        match field.kind {
            evin_action_sdk::ActionFieldKind::String => "string",
            evin_action_sdk::ActionFieldKind::Integer => "integer",
            evin_action_sdk::ActionFieldKind::Boolean => "boolean",
            evin_action_sdk::ActionFieldKind::List => "list",
            evin_action_sdk::ActionFieldKind::Object => "object",
            evin_action_sdk::ActionFieldKind::Enum(_) => "enum",
        }
    )
}

fn enum_value_candidates(
    descriptor: &ActionDescriptor,
    invocation: &ActionInvocation,
    path: &Path,
    offset: usize,
) -> Vec<CompletionCandidate> {
    let Some((field_path, _)) = invocation
        .fields
        .iter()
        .filter(|(_, range)| range.contains_inclusive(path, offset))
        .max_by_key(|(field, _)| field.matches('.').count())
    else {
        return Vec::new();
    };
    let Some(ActionFieldDescriptor {
        kind: evin_action_sdk::ActionFieldKind::Enum(values),
        description,
        ..
    }) = descriptor
        .fields
        .iter()
        .find(|field| field.path == field_path)
    else {
        return Vec::new();
    };
    values
        .iter()
        .map(|value| CompletionCandidate {
            label: (*value).to_owned(),
            detail: "enum value".into(),
            documentation: (*description).to_owned(),
            snippet: (*value).to_owned(),
            catalog_target: None,
            replacement: None,
            filter_text: None,
        })
        .collect()
}

fn mapping_get<'a>(value: &'a SpannedValue, wanted: &str) -> Option<&'a SpannedValue> {
    let SpannedValue::Mapping(entries, _) = value else {
        return None;
    };
    entries.iter().find_map(|(key, value)| match key {
        SpannedValue::String(name, _) if name == wanted => Some(value),
        _ => None,
    })
}

fn scalar_at(document: &WorkspaceDocument, range: &SourceRange) -> Option<String> {
    document.text.get(range.start..range.end).map(|value| {
        value
            .trim_matches(|character: char| {
                character == '\'' || character == '"' || character.is_whitespace()
            })
            .to_owned()
    })
}

fn range(document: &WorkspaceDocument, span: Span) -> SourceRange {
    SourceRange {
        path: document.path.clone(),
        start: span.offset,
        end: span.offset.saturating_add(span.length.max(1)),
    }
}

fn value_range(document: &WorkspaceDocument, value: &SpannedValue) -> SourceRange {
    let span = value.span();
    let mut end = span.offset.saturating_add(span.length.max(1));
    match value {
        SpannedValue::Sequence(values, _) => {
            for value in values {
                end = end.max(value_range(document, value).end);
            }
        }
        SpannedValue::Mapping(values, _) => {
            for (key, value) in values {
                end = end.max(value_range(document, key).end);
                end = end.max(value_range(document, value).end);
            }
        }
        _ => {}
    }
    SourceRange {
        path: document.path.clone(),
        start: span.offset,
        end,
    }
}

fn symbol(
    document: &WorkspaceDocument,
    kind: SemanticKind,
    name: &str,
    span: Span,
    detail: impl Into<String>,
) -> SemanticSymbol {
    SemanticSymbol {
        kind,
        name: name.into(),
        range: range(document, span),
        detail: detail.into(),
        target: None,
        stale: false,
        owner: None,
        catalog_target: None,
        token_kind: None,
    }
}
