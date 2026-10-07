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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticKind {
    Workspace,
    Module,
    Namespace,
    Stage,
    Task,
    TaskReference,
    Value,
    Action,
    ActionField,
    TemplateValue,
    TemplateEnvironment,
    TemplateTaskOutput,
    EnvironmentVariable,
    PackagePolicy,
    ConfigurationKey,
    ConfigurationValue,
}

/// Stable documentation identity for a semantic fact. This is deliberately a
/// small value type: configuration analysis does not load or validate any
/// documentation catalog.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CatalogTarget(String);

impl CatalogTarget {
    pub fn configuration_path(segments: &[&str]) -> Self {
        Self(segments.join("."))
    }

    pub fn action(action_id: &str) -> Self {
        Self(format!("action.{action_id}"))
    }

    pub fn action_field(action_id: &str, field_path: &str) -> Self {
        Self(format!("action.{action_id}.with.{field_path}"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Stable configuration identities generated from the schemas owned by each
/// configuration model. The catalog itself remains an optional prose layer.
pub fn configuration_catalog_targets() -> BTreeSet<CatalogTarget> {
    let mut targets = BTreeSet::from([
        CatalogTarget::configuration_path(&["workspace"]),
        CatalogTarget::configuration_path(&["module"]),
        CatalogTarget::configuration_path(&["namespace"]),
        CatalogTarget::configuration_path(&["environment"]),
        CatalogTarget::configuration_path(&["GitRules"]),
    ]);
    for (schema, root) in [
        (crate::workspace::WORKSPACE_EDITOR_SCHEMA, "workspace"),
        (crate::workspace::MODULE_EDITOR_SCHEMA, "module"),
        (crate::workspace::NAMESPACE_EDITOR_SCHEMA, "namespace"),
        (crate::environment::EDITOR_SCHEMA, "environment"),
        (evin_gitrules::editor_schema::GITRULES_SCHEMA, "GitRules"),
    ] {
        collect_schema_targets(schema, root, &mut targets);
    }
    targets
}

fn collect_schema_targets(
    schema: evin_yaml::schema::Schema,
    path: &str,
    targets: &mut BTreeSet<CatalogTarget>,
) {
    match schema {
        evin_yaml::schema::Schema::Object(fields) => {
            for field in fields {
                let target = field
                    .catalog_target
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("{path}.{}", field.name));
                targets.insert(CatalogTarget(target.clone()));
                collect_schema_targets(field.schema, &target, targets);
            }
        }
        evin_yaml::schema::Schema::Map(value) => {
            let target = format!("{path}.*");
            targets.insert(CatalogTarget(target.clone()));
            collect_schema_targets(*value, &target, targets);
        }
        evin_yaml::schema::Schema::Sequence(value) => {
            collect_schema_targets(*value, &format!("{path}[]"), targets);
        }
        evin_yaml::schema::Schema::Union(variants) => {
            for variant in variants {
                collect_schema_targets(*variant, path, targets);
            }
        }
        _ => {}
    }
}
