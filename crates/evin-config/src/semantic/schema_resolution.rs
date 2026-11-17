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

use evin_flow::schema::{Field, FieldRole, Schema, ValueConstraint};

pub(super) fn schema_description(key: &str) -> String {
    evin_locales::render_message(key, &[])
}

fn schema_message(key: &str, args: &[(&str, String)]) -> String {
    evin_locales::render_message(key, args)
}

fn schema_diagnostic(
    index: &mut WorkspaceSemanticIndex,
    document: &WorkspaceDocument,
    at: &SourceRange,
    message: String,
) {
    index.diagnostics.push(
        Diagnostic::new(
            evin_locales::render_message("flow-schema-diagnostic-title", &[]),
            evin_diagnostic::IssueCode::custom(message),
        )
        .with_file_span(
            document.path.display().to_string(),
            at.start,
            at.len().max(1),
            None,
        ),
    );
}

fn field_detail(field: Field) -> String {
    let presence = if field.required {
        evin_locales::render_message("flow-schema-required", &[])
    } else {
        evin_locales::render_message("flow-schema-optional", &[])
    };
    let mut detail = evin_locales::render_message(
        "flow-schema-field-detail",
        &[
            ("type", field.schema.type_name()),
            ("presence", presence),
            ("description", schema_description(field.description_key)),
        ],
    );
    if let Schema::Enum(choices) = field.schema {
        detail.push_str(&evin_locales::render_message(
            "flow-schema-allowed-values",
            &[("values", choices.join(", "))],
        ));
    }
    if let Some(default) = field.default_value {
        detail.push_str(&evin_locales::render_message(
            "flow-schema-default-value",
            &[("value", default.to_owned())],
        ));
    }
    detail
}

fn empty_schema_value(value: &SpannedValue) -> bool {
    matches!(value, SpannedValue::Null(_))
        || matches!(value, SpannedValue::String(text, _) if text.is_empty())
}

fn schema_token_role(role: FieldRole) -> SemanticTokenKind {
    match role {
        FieldRole::Property => SemanticTokenKind::Property,
        FieldRole::Namespace => SemanticTokenKind::Namespace,
        FieldRole::Modifier => SemanticTokenKind::Modifier,
        FieldRole::EnumMember => SemanticTokenKind::EnumMember,
    }
}

fn collect_schema_value_tokens(
    index: &mut WorkspaceSemanticIndex,
    document: &WorkspaceDocument,
    value: &SpannedValue,
    kind: SemanticTokenKind,
) {
    match value {
        SpannedValue::String(text, span) if !text.is_empty() => {
            index.schema_value_tokens.push(SemanticTokenFact {
                range: range(document, *span),
                kind,
            });
        }
        SpannedValue::Sequence(values, _) => {
            for value in values {
                collect_schema_value_tokens(index, document, value, kind);
            }
        }
        _ => {}
    }
}

fn validate_schema_constraints(
    index: &mut WorkspaceSemanticIndex,
    document: &WorkspaceDocument,
    value: &SpannedValue,
    constraints: &[ValueConstraint],
    path: &str,
) {
    match value {
        SpannedValue::Sequence(values, _) => {
            for value in values {
                validate_schema_constraints(index, document, value, constraints, path);
            }
        }
        SpannedValue::String(text, _) if !text.is_empty() => {
            for constraint in constraints {
                let error = match constraint {
                    ValueConstraint::Regex => regex::Regex::new(text).err().map(|error| {
                        schema_message(
                            "flow-schema-invalid-regex",
                            &[("path", path.to_owned()), ("reason", error.to_string())],
                        )
                    }),
                    ValueConstraint::GitTemplate => {
                        evin_flow::template::regex::compile_template(text)
                            .err()
                            .map(|error| {
                                schema_message(
                                    "flow-schema-invalid-git-template",
                                    &[("path", path.to_owned()), ("reason", error.to_string())],
                                )
                            })
                    }
                };
                if let Some(error) = error {
                    schema_diagnostic(index, document, &value_range(document, value), error);
                }
            }
        }
        _ => {}
    }
}

fn resolve_schema_node(
    index: &mut WorkspaceSemanticIndex,
    document: &WorkspaceDocument,
    value: &SpannedValue,
    schema: Schema,
    path: &str,
    allow_null: bool,
    start_floor: usize,
) {
    if allow_null && empty_schema_value(value) {
        return;
    }
    match schema {
        Schema::Object(fields) => {
            let SpannedValue::Mapping(entries, _) = value else {
                schema_diagnostic(
                    index,
                    document,
                    &value_range(document, value),
                    schema_message("flow-schema-expects-object", &[("path", path.into())]),
                );
                return;
            };
            let mut present = BTreeSet::new();
            let mut keys = Vec::new();
            let mut occupied = Vec::new();
            for (key, child) in entries {
                let SpannedValue::String(name, key_span) = key else {
                    schema_diagnostic(
                        index,
                        document,
                        &range(document, key.span()),
                        schema_message(
                            "flow-schema-expects-string-field-names",
                            &[("path", path.into())],
                        ),
                    );
                    continue;
                };
                let key_range = range(document, *key_span);
                keys.push(key_range.clone());
                occupied.push(key_range.clone());
                occupied.push(value_range(document, child));
                let Some(field) = fields.iter().find(|field| field.name == name) else {
                    schema_diagnostic(
                        index,
                        document,
                        &key_range,
                        schema_message(
                            "flow-schema-unknown-field",
                            &[("path", path.into()), ("field", name.clone())],
                        ),
                    );
                    continue;
                };
                let canonical = field.alias_of.unwrap_or(field.name);
                if !present.insert(canonical.into()) {
                    schema_diagnostic(
                        index,
                        document,
                        &key_range,
                        schema_message(
                            "flow-schema-duplicate-field",
                            &[("path", path.into()), ("field", canonical.to_owned())],
                        ),
                    );
                }
                let child_path = format!("{path}.{name}");
                let target = CatalogTarget(field.catalog_target.unwrap_or(&child_path).to_owned());
                let detail = field_detail(*field);
                let mut key_symbol = symbol(
                    document,
                    SemanticKind::ConfigurationKey,
                    name,
                    *key_span,
                    &detail,
                );
                key_symbol.catalog_target = Some(target.clone());
                key_symbol.token_kind = Some(schema_token_role(field.role));
                index.symbols.push(key_symbol);
                if let Some(role) = field.value_role {
                    collect_schema_value_tokens(index, document, child, schema_token_role(role));
                }
                match child {
                    SpannedValue::Mapping(_, _) | SpannedValue::Null(_) => {}
                    SpannedValue::Sequence(entries, _) => {
                        for item in entries {
                            if !matches!(
                                item,
                                SpannedValue::Mapping(_, _)
                                    | SpannedValue::Sequence(_, _)
                                    | SpannedValue::Null(_)
                            ) {
                                let mut value_symbol = symbol(
                                    document,
                                    SemanticKind::ConfigurationValue,
                                    name,
                                    item.span(),
                                    &detail,
                                );
                                value_symbol.range = range(document, item.span());
                                value_symbol.catalog_target = Some(target.clone());
                                index.symbols.push(value_symbol);
                            }
                        }
                    }
                    _ => {
                        let mut value_symbol = symbol(
                            document,
                            SemanticKind::ConfigurationValue,
                            name,
                            child.span(),
                            &detail,
                        );
                        value_symbol.range = range(document, child.span());
                        value_symbol.catalog_target = Some(target);
                        index.symbols.push(value_symbol);
                    }
                }
                let empty_value = empty_schema_value(child);
                if let (true, Schema::Enum(choices)) = (empty_value, field.schema) {
                    let end = document.text[key_range.end..]
                        .find('\n')
                        .map(|distance| key_range.end + distance)
                        .unwrap_or(document.text.len());
                    index.schema_choices.push(SchemaChoiceContext {
                        range: SourceRange {
                            path: document.path.clone(),
                            start: key_range.end,
                            end,
                        },
                        choices,
                        description_key: field.description_key,
                    });
                }
                resolve_schema_node(
                    index,
                    document,
                    child,
                    field.schema,
                    &child_path,
                    field.nullable,
                    key_range.end,
                );
                validate_schema_constraints(index, document, child, field.constraints, &child_path);
            }
            let mut container_range = value_range(document, value);
            container_range.start = start_floor;
            container_range.end = container_range.end.max(container_range.start);
            for field in fields
                .iter()
                .filter(|field| field.required && !present.contains(field.name))
            {
                schema_diagnostic(
                    index,
                    document,
                    &container_range,
                    schema_message(
                        "flow-schema-missing-required-field",
                        &[("path", path.into()), ("field", field.name.into())],
                    ),
                );
            }
            index.schema_contexts.push(SchemaContext {
                range: container_range,
                schema,
                present,
                keys,
                occupied,
                path: path.into(),
            });
        }
        Schema::Map(item_schema) => {
            let SpannedValue::Mapping(entries, _) = value else {
                schema_diagnostic(
                    index,
                    document,
                    &value_range(document, value),
                    schema_message("flow-schema-expects-map", &[("path", path.into())]),
                );
                return;
            };
            let mut present = BTreeSet::new();
            for (key, child) in entries {
                let SpannedValue::String(name, key_span) = key else {
                    schema_diagnostic(
                        index,
                        document,
                        &range(document, key.span()),
                        schema_message(
                            "flow-schema-expects-string-map-keys",
                            &[("path", path.into())],
                        ),
                    );
                    continue;
                };
                let key_range = range(document, *key_span);
                if !present.insert(name.clone()) {
                    schema_diagnostic(
                        index,
                        document,
                        &key_range,
                        schema_message(
                            "flow-schema-duplicate-map-key",
                            &[("path", path.into()), ("key", name.clone())],
                        ),
                    );
                }
                let child_path = format!("{path}.*");
                if !path.ends_with(".tasks") && !path.ends_with(".values") {
                    let kind = if path == "RepositoryPolicy.releases.packages" {
                        SemanticKind::PackagePolicy
                    } else if path == "environment.environments" {
                        SemanticKind::EnvironmentVariable
                    } else {
                        SemanticKind::ConfigurationKey
                    };
                    let mut key_symbol = symbol(
                        document,
                        kind,
                        name,
                        *key_span,
                        schema_message(
                            "flow-schema-user-defined-key",
                            &[("type", item_schema.type_name())],
                        ),
                    );
                    key_symbol.catalog_target = Some(CatalogTarget(child_path.clone()));
                    index.symbols.push(key_symbol);
                }
                resolve_schema_node(
                    index,
                    document,
                    child,
                    *item_schema,
                    &child_path,
                    false,
                    key_range.end,
                );
            }
            let mut container_range = value_range(document, value);
            container_range.start = start_floor;
            container_range.end = container_range.end.max(container_range.start);
            index.schema_contexts.push(SchemaContext {
                range: container_range,
                schema,
                present,
                keys: entries.iter().map(|(key, _)| range(document, key.span())).collect(),
                occupied: entries
                    .iter()
                    .flat_map(|(key, child)| {
                        [range(document, key.span()), value_range(document, child)]
                    })
                    .collect(),
                path: path.into(),
            });
        }
        Schema::Sequence(item_schema) => {
            let SpannedValue::Sequence(values, _) = value else {
                schema_diagnostic(
                    index,
                    document,
                    &value_range(document, value),
                    schema_message(
                        "flow-schema-expects-type",
                        &[("path", path.into()), ("type", schema.type_name())],
                    ),
                );
                return;
            };
            for child in values {
                resolve_schema_node(
                    index,
                    document,
                    child,
                    *item_schema,
                    &format!("{path}[]"),
                    false,
                    start_floor,
                );
            }
        }
        Schema::Union(variants) => {
            if let Some(variant) = variants
                .iter()
                .find(|variant| schema_matches(**variant, value))
            {
                resolve_schema_node(index, document, value, *variant, path, false, start_floor);
            } else {
                schema_diagnostic(
                    index,
                    document,
                    &value_range(document, value),
                    schema_message(
                        "flow-schema-expects-type",
                        &[("path", path.into()), ("type", schema.type_name())],
                    ),
                );
            }
        }
        Schema::Enum(choices) => {
            if !empty_schema_value(value) {
                collect_schema_value_tokens(index, document, value, SemanticTokenKind::EnumMember);
                index.schema_choices.push(SchemaChoiceContext {
                    range: value_range(document, value),
                    choices,
                    description_key: "flow-schema-allowed-value",
                });
            }
            if let SpannedValue::String(actual, _) = value {
                if !choices.contains(&actual.as_str()) {
                    schema_diagnostic(
                        index,
                        document,
                        &value_range(document, value),
                        schema_message(
                            "flow-schema-expects-one-of",
                            &[("path", path.into()), ("values", choices.join(", "))],
                        ),
                    );
                }
            } else {
                schema_diagnostic(
                    index,
                    document,
                    &value_range(document, value),
                    schema_message(
                        "flow-schema-expects-one-of",
                        &[("path", path.into()), ("values", choices.join(", "))],
                    ),
                );
            }
        }
        Schema::Boolean => {
            let context_range = if empty_schema_value(value) {
                let span = value.span();
                let end = document.text[span.offset.min(document.text.len())..]
                    .find('\n')
                    .map(|distance| span.offset + distance)
                    .unwrap_or(document.text.len());
                SourceRange {
                    path: document.path.clone(),
                    start: span.offset,
                    end,
                }
            } else {
                value_range(document, value)
            };
            index.schema_choices.push(SchemaChoiceContext {
                range: context_range,
                choices: &["true", "false"],
                description_key: "flow-schema-boolean-value",
            });
            if !schema_matches(schema, value) && !empty_schema_value(value) {
                schema_diagnostic(
                    index,
                    document,
                    &value_range(document, value),
                    schema_message(
                        "flow-schema-expects-type",
                        &[("path", path.into()), ("type", schema.type_name())],
                    ),
                );
            }
        }
        Schema::String | Schema::Integer | Schema::Number | Schema::Any => {
            if !schema_matches(schema, value) {
                schema_diagnostic(
                    index,
                    document,
                    &value_range(document, value),
                    schema_message(
                        "flow-schema-expects-type",
                        &[("path", path.into()), ("type", schema.type_name())],
                    ),
                );
            }
        }
    }
}

fn schema_matches(schema: Schema, value: &SpannedValue) -> bool {
    match schema {
        Schema::Object(_) | Schema::Map(_) => matches!(value, SpannedValue::Mapping(_, _)),
        Schema::Sequence(_) => matches!(value, SpannedValue::Sequence(_, _)),
        Schema::Union(variants) => variants
            .iter()
            .any(|variant| schema_matches(*variant, value)),
        Schema::String | Schema::Enum(_) => matches!(value, SpannedValue::String(_, _)),
        Schema::Integer => matches!(value, SpannedValue::Integer(n, _) if *n >= 0),
        Schema::Number => matches!(
            value,
            SpannedValue::Integer(_, _) | SpannedValue::Float(_, _)
        ),
        Schema::Boolean => matches!(value, SpannedValue::Bool(_, _)),
        Schema::Any => true,
    }
}
