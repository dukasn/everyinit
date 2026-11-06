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

fn collect_flow_token_facts(
    value: &SpannedValue,
    path: &Path,
    properties: &mut Vec<SemanticTokenFact>,
    strings: &mut Vec<SourceRange>,
) {
    match value {
        SpannedValue::Mapping(entries, _) => {
            for (key, value) in entries {
                if let SpannedValue::String(_, span) = key {
                    properties.push(SemanticTokenFact {
                        range: SourceRange {
                            path: path.to_owned(),
                            start: span.offset,
                            end: span.offset.saturating_add(span.length.max(1)),
                        },
                        kind: SemanticTokenKind::Property,
                    });
                }
                collect_flow_token_facts(value, path, properties, strings);
            }
        }
        SpannedValue::Sequence(values, _) => {
            for value in values {
                collect_flow_token_facts(value, path, properties, strings);
            }
        }
        SpannedValue::String(_, span) => strings.push(SourceRange {
            path: path.to_owned(),
            start: span.offset,
            end: span.offset.saturating_add(span.length.max(1)),
        }),
        _ => {}
    }
}

fn semantic_symbol_token_kind(symbol: &SemanticSymbol) -> Option<SemanticTokenKind> {
    Some(match symbol.kind {
        SemanticKind::Task | SemanticKind::TaskReference => SemanticTokenKind::Function,
        SemanticKind::Stage => SemanticTokenKind::EnumMember,
        SemanticKind::Action => SemanticTokenKind::Method,
        SemanticKind::ActionField | SemanticKind::PackagePolicy => SemanticTokenKind::Property,
        SemanticKind::TemplateValue
        | SemanticKind::TemplateEnvironment
        | SemanticKind::TemplateTaskOutput
        | SemanticKind::Value => SemanticTokenKind::Variable,
        SemanticKind::EnvironmentVariable => SemanticTokenKind::Parameter,
        SemanticKind::ConfigurationKey => symbol.token_kind.unwrap_or(SemanticTokenKind::Property),
        SemanticKind::ConfigurationValue => return None,
        SemanticKind::Workspace | SemanticKind::Module | SemanticKind::Namespace => return None,
    })
}
