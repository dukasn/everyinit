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
use crate::source::OverlaySource;
use evin_action_sdk::{
    ActionDescriptor, ActionFieldDescriptor, ActionFieldKind, ActionOutputDescriptor, RunnerContext,
};

const DEMO_DESCRIPTOR: ActionDescriptor = ActionDescriptor {
    summary: "Test action.",
    fields: &[
        ActionFieldDescriptor {
            path: "format",
            kind: ActionFieldKind::Enum(&["arch", "tar"]),
            required: true,
            description: "Output format.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "arch_only",
            kind: ActionFieldKind::String,
            required: false,
            description: "Arch setting.",
            condition: Some("format=arch"),
        },
        ActionFieldDescriptor {
            path: "tar_only",
            kind: ActionFieldKind::String,
            required: false,
            description: "Tar setting.",
            condition: Some("format=tar"),
        },
        ActionFieldDescriptor {
            path: "label",
            kind: ActionFieldKind::String,
            required: false,
            description: "Artifact label.",
            condition: None,
        },
    ],
    outputs: &[ActionOutputDescriptor {
        name: "artifact",
        description: "Artifact output.",
    }],
};

struct DemoAction;
#[async_trait::async_trait]
impl Action for DemoAction {
    fn editor_descriptor(&self) -> Option<&'static ActionDescriptor> {
        Some(&DEMO_DESCRIPTOR)
    }
    fn validate_config(&self, _: Option<&SpannedValue>) -> Result<(), evin_flow::ConfigError> {
        Ok(())
    }
    async fn execute(&self, _: &RunnerContext, _: Option<SpannedValue>) -> Result<(), Diagnostic> {
        Ok(())
    }
}

fn actions() -> HashMap<String, Arc<dyn Action>> {
    HashMap::from([(
        "test/action@v1".into(),
        Arc::new(DemoAction) as Arc<dyn Action>,
    )])
}

#[tokio::test]
async fn snapshot_keeps_raw_documents_and_template_definitions() {
    let mut source = OverlaySource::default();
    source.insert(".evin/workspace.flow", "workspace: { name: app, version: 1.0.0, include: [.evin/packages.flow] }\nstages: [Build]\nvalues: { artifact: dist }\n");
    source.insert(".evin/packages.flow", "namespace: { name: packages }\ntasks:\n  build:\n    values: { artifact: local }\n    steps:\n      - uses: evin/archive@v1\n        with: { format: arch, package: { name: '${artifact}' } }\n");
    let snapshot = WorkspaceSnapshot::load(&source).await;
    assert_eq!(snapshot.documents.len(), 2);
    let index = WorkspaceSemanticIndex::build(snapshot, &HashMap::new());
    let reference = index
        .symbols
        .iter()
        .find(|symbol| symbol.kind == SemanticKind::TemplateValue)
        .unwrap();
    assert_eq!(reference.name, "artifact");
    assert!(reference.target.is_some());
}

#[tokio::test]
async fn semantic_tokens_overlay_flow_meaning_on_generic_structure_roles() {
    let text = r#"workspace:
  name: app
  rules: ".evin/gitrules.flow"
  custom: { nested_unknown: "ordinary text" }
stages: [Build, Test]
tasks:
  build:
    stage: Build
    description: "compile"
    on: { hooks: [pre-commit] }
    needs: [test]
    watch: { paths: ["crates/**/*.rs"] }
    presets: [ci]
    steps:
      - uses: "evin/archive@v1"
        with: { package: { name: "${artifact}" } }
  test:
    stage: Test
"#;
    let mut source = OverlaySource::default();
    source.insert(".evin/workspace.flow", text);
    let index =
        WorkspaceSemanticIndex::build(WorkspaceSnapshot::load(&source).await, &HashMap::new());
    let tokens = index.semantic_tokens(Path::new(".evin/workspace.flow"));
    let kind_at = |needle: &str| {
        let start = text.find(needle).unwrap();
        let end = start + needle.len();
        tokens
            .iter()
            .find(|token| token.range.start < end && token.range.end > start)
            .map(|token| token.kind)
            .unwrap_or_else(|| panic!("no semantic token for {needle:?}"))
    };

    assert_eq!(kind_at("workspace:"), SemanticTokenKind::Namespace);
    assert_eq!(kind_at("stages:"), SemanticTokenKind::Property);
    assert_eq!(kind_at("custom:"), SemanticTokenKind::Property);
    assert_eq!(kind_at("nested_unknown:"), SemanticTokenKind::Property);
    assert_eq!(kind_at("build:"), SemanticTokenKind::Function);
    assert_eq!(kind_at("stage:"), SemanticTokenKind::Property);
    assert_eq!(kind_at("on: {"), SemanticTokenKind::Modifier);
    assert_eq!(kind_at("watch:"), SemanticTokenKind::Modifier);
    assert_eq!(kind_at("needs:"), SemanticTokenKind::Modifier);
    assert_eq!(kind_at("steps:"), SemanticTokenKind::Modifier);
    assert_eq!(kind_at("presets:"), SemanticTokenKind::Property);
    assert_eq!(kind_at("Build,"), SemanticTokenKind::EnumMember);
    assert_eq!(kind_at("pre-commit"), SemanticTokenKind::EnumMember);
    assert_eq!(kind_at("evin/archive@v1"), SemanticTokenKind::Method);
    assert_eq!(kind_at("${artifact}"), SemanticTokenKind::Variable);
    assert_eq!(kind_at(".evin/gitrules.flow"), SemanticTokenKind::String);
    assert_eq!(kind_at("ordinary text"), SemanticTokenKind::String);

    assert!(tokens.windows(2).all(|pair| {
        pair[0].range.start <= pair[1].range.start && pair[0].range.end <= pair[1].range.start
    }));
}

#[test]
fn semantic_ranges_are_utf8_bytes_after_unicode() {
    let path = PathBuf::from(".evin/workspace.flow");
    let text = "values: { café: preço }\nworkspace: { name: app }\n";
    let snapshot = WorkspaceSnapshot {
        config: None,
        documents: BTreeMap::from([(
            path.clone(),
            WorkspaceDocument {
                path: path.clone(),
                kind: DocumentKind::Workspace,
                header: None,
                text: text.into(),
                ast: Some(evin_flow::parse_yaml(text).unwrap()),
                completion_ast: None,
                diagnostics: Vec::new(),
            },
        )]),
        diagnostics: Vec::new(),
    };
    let index = WorkspaceSemanticIndex::build(snapshot, &HashMap::new());
    let name_start = text.find("name:").unwrap();
    let hover = index.hover_at(&path, name_start).unwrap();
    assert_eq!(&text[hover.range.start..hover.range.end], "name");
    assert_eq!(hover.catalog_target.unwrap().as_str(), "workspace.name");

    let value_start = text.find("café:").unwrap();
    let declaration = index
        .symbols
        .iter()
        .find(|symbol| symbol.kind == SemanticKind::Value && symbol.name == "café")
        .unwrap();
    assert_eq!(
        &text[declaration.range.start..declaration.range.end],
        "café"
    );
    assert_eq!(declaration.range.start, value_start);
    assert!(index.semantic_tokens(&path).iter().any(|token| {
        token.range == declaration.range && token.kind == SemanticTokenKind::Variable
    }));
}

#[test]
fn quoted_unicode_value_range_covers_the_source_scalar() {
    let path = PathBuf::from(".evin/workspace.flow");
    let text = "workspace: { name: \"🦀\", version: 1.0.0 }\nstages: []\n";
    let snapshot = WorkspaceSnapshot {
        config: None,
        documents: BTreeMap::from([(
            path.clone(),
            WorkspaceDocument {
                path: path.clone(),
                kind: DocumentKind::Workspace,
                header: None,
                text: text.into(),
                ast: Some(evin_flow::parse_yaml(text).unwrap()),
                completion_ast: None,
                diagnostics: Vec::new(),
            },
        )]),
        diagnostics: Vec::new(),
    };
    let index = WorkspaceSemanticIndex::build(snapshot, &HashMap::new());
    let hover = index.hover_at(&path, text.find("🦀").unwrap()).unwrap();
    assert_eq!(
        &text[hover.range.start..hover.range.end],
        "\"🦀\"",
        "{hover:?}"
    );
}

#[test]
fn gitrules_schema_resolves_nested_fields_dynamic_maps_and_errors() {
    let path = PathBuf::from(".evin/gitrules.flow");
    let text = "commits:\n  message_format: '{type}: {description}'\n  fields:\n    type:\n      allowed_values: [feat, fix]\n      pattern: '['\n  policies:\n    require_body: true\n    surprise: false\n    require_body: no\nreleases:\n  versioning: { strategy: invalid }\n  packages:\n    app:\n      watch: [src/**]\n      targets: [deb]\n      revision: { scope: target }\n    incomplete: { watch: [] }\n";
    let snapshot = WorkspaceSnapshot {
        config: None,
        documents: BTreeMap::from([(
            path.clone(),
            WorkspaceDocument {
                path: path.clone(),
                kind: DocumentKind::GitRules,
                header: None,
                text: text.into(),
                ast: Some(evin_flow::parse_yaml(text).unwrap()),
                completion_ast: None,
                diagnostics: Vec::new(),
            },
        )]),
        diagnostics: Vec::new(),
    };
    let index = WorkspaceSemanticIndex::build(snapshot, &HashMap::new());
    for (key, target) in [
        ("message_format", "GitRules.commits.message_format"),
        ("allowed_values", "GitRules.commits.fields.*.allowed_values"),
        ("require_body", "GitRules.commits.policies.require_body"),
        ("strategy", "GitRules.releases.versioning.strategy"),
        ("watch", "GitRules.releases.packages.*.watch"),
        ("scope", "GitRules.releases.packages.*.revision.scope"),
    ] {
        let offset = text.find(key).unwrap();
        let hover = index.hover_at(&path, offset).unwrap();
        assert_eq!(&text[hover.range.start..hover.range.end], key);
        assert_eq!(hover.catalog_target.unwrap().as_str(), target);
    }
    let message_hover = index
        .hover_at(&path, text.find("message_format").unwrap())
        .unwrap();
    assert!(message_hover.markdown.contains("string | string[]"));
    assert!(message_hover.markdown.contains("template"));

    let policy_offset = text.find("surprise").unwrap();
    let policy_labels = index
        .completions_at(&path, policy_offset, &HashMap::new())
        .into_iter()
        .map(|candidate| candidate.label)
        .collect::<BTreeSet<_>>();
    assert!(policy_labels.contains("max_body_line_length"));
    assert!(!policy_labels.contains("require_body"));
    assert!(!policy_labels.contains("allowed_values"));

    let constraint_offset = text.find("allowed_values").unwrap();
    let constraint_labels = index
        .completions_at(&path, constraint_offset, &HashMap::new())
        .into_iter()
        .map(|candidate| candidate.label)
        .collect::<BTreeSet<_>>();
    assert!(
        constraint_labels.contains("must_be"),
        "{constraint_labels:?} contexts={:?}",
        index.schema_contexts
    );
    assert!(!constraint_labels.contains("allowed_values"));

    let messages = index
        .diagnostics
        .iter()
        .flat_map(|diagnostic| diagnostic.code.args.iter().map(|(_, value)| value.clone()))
        .collect::<Vec<_>>();
    assert!(
        messages
            .iter()
            .any(|message| message.contains("Unknown field GitRules.commits.policies.surprise")),
        "{messages:?}"
    );
    assert!(messages.iter().any(|message| {
        message.contains("Duplicate field GitRules.commits.policies.require_body")
    }));
    assert!(messages.iter().any(|message| {
        message.contains("GitRules.commits.policies.require_body expects boolean")
    }));
    assert!(messages.iter().any(|message| {
        message.contains("Missing required field GitRules.releases.packages.*.targets")
    }));
    assert!(messages.iter().any(|message| {
        message.contains("Invalid regular expression for GitRules.commits.fields.*.pattern")
    }));
    assert!(
        messages
            .iter()
            .any(|message| message.contains("GitRules.releases.versioning.strategy")),
        "{messages:?}"
    );
}

#[tokio::test]
async fn optional_presence_does_not_make_defaulted_fields_nullable() {
    let mut source = OverlaySource::default();
    let text =
        "workspace:\n  name: app\n  version: 1.0.0\n  include: null\nstages: []\nvalues: null\n";
    source.insert(".evin/workspace.flow", text);
    let index = WorkspaceSemanticIndex::build(WorkspaceSnapshot::load(&source).await, &actions());
    let messages = index
        .diagnostics
        .iter()
        .flat_map(|diagnostic| diagnostic.code.args.iter().map(|(_, value)| value.clone()))
        .collect::<Vec<_>>();
    assert!(
        messages
            .iter()
            .any(|message| message.contains("workspace.values expects a map")),
        "{messages:?}"
    );
    assert!(
        !messages
            .iter()
            .any(|message| message.contains("workspace.workspace.include expects")),
        "{messages:?}"
    );
}

#[tokio::test]
async fn canonical_and_alias_fields_share_presence() {
    let mut source = OverlaySource::default();
    let text =
        "workspace:\n  name: app\n  version: 1.0.0\n  include: []\n  includes: []\nstages: []\n";
    source.insert(".evin/workspace.flow", text);
    let index = WorkspaceSemanticIndex::build(WorkspaceSnapshot::load(&source).await, &actions());
    let messages = index
        .diagnostics
        .iter()
        .flat_map(|diagnostic| diagnostic.code.args.iter().map(|(_, value)| value.clone()))
        .collect::<Vec<_>>();
    assert!(
        messages
            .iter()
            .any(|message| message.contains("Duplicate field workspace.workspace.include")),
        "{messages:?}"
    );
    let labels = index
        .completions_at(
            Path::new(".evin/workspace.flow"),
            text.find("includes:").unwrap(),
            &actions(),
        )
        .into_iter()
        .map(|candidate| candidate.label)
        .collect::<BTreeSet<_>>();
    assert!(
        !labels.contains("include") && !labels.contains("includes"),
        "{labels:?}"
    );
}

#[test]
fn owned_configuration_schemas_match_serialized_field_names() {
    use evin_flow::schema::Schema;
    fn field(schema: Schema, name: &str) -> Schema {
        let Schema::Object(fields) = schema else {
            panic!("expected object")
        };
        fields
            .iter()
            .find(|field| field.name == name)
            .unwrap()
            .schema
    }
    fn check(schema: Schema, value: impl serde::Serialize, aliases: &[&str]) {
        let Schema::Object(fields) = schema else {
            panic!("expected object")
        };
        let declared = fields
            .iter()
            .map(|field| field.name)
            .filter(|name| !aliases.contains(name))
            .collect::<BTreeSet<_>>();
        let serialized = serde_json::to_value(value).unwrap();
        let actual = serialized
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        assert_eq!(declared, actual);
    }

    let workspace: crate::workspace::WorkspaceConfig =
        serde_json::from_value(serde_json::json!({"name": "app", "version": "1.0.0"})).unwrap();
    check(
        field(crate::workspace::WORKSPACE_EDITOR_SCHEMA, "workspace"),
        workspace,
        &["includes"],
    );
    let module: crate::workspace::ModuleConfig =
        serde_json::from_value(serde_json::json!({"name": "app"})).unwrap();
    check(
        field(crate::workspace::MODULE_EDITOR_SCHEMA, "module"),
        module,
        &["includes"],
    );
    let namespace: crate::workspace::NamespaceConfig =
        serde_json::from_value(serde_json::json!({"name": "app"})).unwrap();
    check(
        field(crate::workspace::NAMESPACE_EDITOR_SCHEMA, "namespace"),
        namespace,
        &[],
    );
    check(
        crate::models::TASK_EDITOR_SCHEMA,
        crate::models::TaskObject::default(),
        &[],
    );
    check(
        field(crate::models::TASK_EDITOR_SCHEMA, "watch"),
        crate::models::WatchConfig {
            paths: Vec::new(),
            ignore: Vec::new(),
            debounce: "300ms".into(),
        },
        &[],
    );
    check(
        field(crate::models::TASK_EDITOR_SCHEMA, "on"),
        crate::models::TriggerObject { hooks: None },
        &[],
    );
    check(
        match field(crate::models::TASK_EDITOR_SCHEMA, "steps") {
            Schema::Sequence(step) => *step,
            _ => panic!("expected steps sequence"),
        },
        crate::models::StepObject {
            run: None,
            uses: None,
            with: None,
        },
        &[],
    );
    let env_var: crate::environment::EnvVarDef =
        serde_json::from_value(serde_json::json!({"type": "string"})).unwrap();
    let Schema::Map(env_var_schema) = field(crate::environment::EDITOR_SCHEMA, "environments")
    else {
        panic!("expected environments map")
    };
    check(*env_var_schema, env_var, &[]);
}

#[test]
fn completion_uses_the_open_mapping_at_a_blank_line() {
    let path = PathBuf::from(".evin/workspace.flow");
    let text = "workspace: { name: app, version: 1.0.0 }\nstages: []\ntasks:\n  build:\n    description: build\n    \n";
    let snapshot = WorkspaceSnapshot {
        config: None,
        documents: BTreeMap::from([(
            path.clone(),
            WorkspaceDocument {
                path: path.clone(),
                kind: DocumentKind::Workspace,
                header: None,
                text: text.into(),
                ast: Some(evin_flow::parse_yaml(text).unwrap()),
                completion_ast: None,
                diagnostics: Vec::new(),
            },
        )]),
        diagnostics: Vec::new(),
    };
    let index = WorkspaceSemanticIndex::build(snapshot, &HashMap::new());
    let offset = text.rfind("    \n").unwrap() + 4;
    let labels = index
        .completions_at(&path, offset, &HashMap::new())
        .into_iter()
        .map(|candidate| candidate.label)
        .collect::<BTreeSet<_>>();
    assert!(labels.contains("stage"), "{labels:?}");
    assert!(!labels.contains("description"));
    assert!(!labels.contains("workspace"));
}

#[test]
fn empty_enum_value_offers_only_declared_choices() {
    let path = PathBuf::from(".evin/workspace.flow");
    let text = "workspace:\n  name: app\n  version: 1.0.0\n  execution: \nstages: []\n";
    let snapshot = WorkspaceSnapshot {
        config: None,
        documents: BTreeMap::from([(
            path.clone(),
            WorkspaceDocument {
                path: path.clone(),
                kind: DocumentKind::Workspace,
                header: None,
                text: text.into(),
                ast: Some(evin_flow::parse_yaml(text).unwrap()),
                completion_ast: None,
                diagnostics: Vec::new(),
            },
        )]),
        diagnostics: Vec::new(),
    };
    let index = WorkspaceSemanticIndex::build(snapshot, &HashMap::new());
    let offset = text.find("execution: ").unwrap() + "execution: ".len();
    let labels = index
        .completions_at(&path, offset, &HashMap::new())
        .into_iter()
        .map(|candidate| candidate.label)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        labels,
        BTreeSet::from(["linear".into(), "parallel".into()]),
        "offset={offset} choices={:?}",
        index.schema_choices
    );
}

#[test]
fn gitrules_versioning_strategy_completion_offers_supported_presets() {
    let path = PathBuf::from(".evin/gitrules.flow");
    let text = "releases:\n  versioning:\n    strategy: \n";
    let snapshot = WorkspaceSnapshot {
        config: None,
        documents: BTreeMap::from([(
            path.clone(),
            WorkspaceDocument {
                path: path.clone(),
                kind: DocumentKind::GitRules,
                header: None,
                text: text.into(),
                ast: Some(evin_flow::parse_yaml(text).unwrap()),
                completion_ast: None,
                diagnostics: Vec::new(),
            },
        )]),
        diagnostics: Vec::new(),
    };
    let index = WorkspaceSemanticIndex::build(snapshot, &HashMap::new());
    let offset = text.find("strategy: ").unwrap() + "strategy: ".len();
    let labels = index
        .completions_at(&path, offset, &HashMap::new())
        .into_iter()
        .map(|candidate| candidate.label)
        .collect::<BTreeSet<_>>();
    assert_eq!(labels, BTreeSet::from(["cargo".into(), "semver".into()]));
}

#[tokio::test]
async fn snapshot_classifies_an_explicitly_included_marked_flow_as_a_namespace() {
    let mut source = OverlaySource::default();
    source.insert(
        ".evin/workspace.flow",
        "workspace: { name: app, version: 1.0.0, include: [automation.flow] }\nstages: []\n",
    );
    source.insert(
        "automation.flow",
        "#!evin/v1\nnamespace: { name: automation }\ntasks: {}\n",
    );

    let snapshot = WorkspaceSnapshot::load(&source).await;
    assert_eq!(
        snapshot
            .documents
            .get(&PathBuf::from("automation.flow"))
            .map(|document| document.kind),
        Some(DocumentKind::Namespace)
    );
}

#[tokio::test]
async fn snapshot_does_not_bootstrap_from_a_legacy_workspace_yaml() {
    let mut source = OverlaySource::default();
    source.insert(
        ".evin/workspace.yaml",
        "workspace: { name: legacy, version: 1.0.0 }\nstages: []\n",
    );

    let snapshot = WorkspaceSnapshot::load(&source).await;
    assert!(snapshot.documents.is_empty());
    assert_eq!(
        snapshot
            .config
            .as_ref()
            .map(|config| config.workspace_name.as_str()),
        Some("")
    );
}

#[tokio::test]
async fn snapshot_retains_headers_and_reports_an_invalid_header_once() {
    let mut source = OverlaySource::default();
    source.insert(
        ".evin/workspace.flow",
        "#!evin/v1\nworkspace: { name: app, version: 1.0.0 }\nstages: []\n",
    );
    let snapshot = WorkspaceSnapshot::load(&source).await;
    assert_eq!(
        snapshot
            .documents
            .get(&PathBuf::from(".evin/workspace.flow"))
            .and_then(|document| document.header)
            .map(|header| header.version),
        Some(1)
    );

    source.insert(".evin/workspace.flow", "#!evin/v2\nworkspace: [\n");
    let snapshot = WorkspaceSnapshot::load(&source).await;
    let document = snapshot
        .documents
        .get(&PathBuf::from(".evin/workspace.flow"))
        .unwrap();
    assert!(document.header.is_none());
    assert!(document.ast.is_none());
    assert_eq!(
        snapshot
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code.key == "issue-evin-header-invalid")
            .count(),
        1
    );
}

#[tokio::test]
async fn invalid_overlay_keeps_only_stale_local_symbols() {
    let mut source = OverlaySource::default();
    let valid =
        "workspace: { name: app, version: 1.0.0 }\nstages: []\ntasks: { build: { run: echo } }\n";
    source.insert(".evin/workspace.flow", valid);
    let mut session = WorkspaceAnalysisSession::default();
    session.update(WorkspaceSnapshot::load(&source).await, &HashMap::new());
    let build = session
        .current
        .as_ref()
        .unwrap()
        .symbols
        .iter()
        .find(|symbol| symbol.kind == SemanticKind::Task && symbol.name == "build")
        .unwrap()
        .range
        .clone();
    source.insert(".evin/workspace.flow", format!("{valid}broken: [\n"));
    session.update(WorkspaceSnapshot::load(&source).await, &HashMap::new());
    assert!(
        session
            .stale_symbols
            .contains_key(&PathBuf::from(".evin/workspace.flow"))
    );
    assert!(
        session
            .stale_symbol_at(&build.path, build.start)
            .is_some_and(|symbol| symbol.stale)
    );
    assert!(
        session
            .current
            .as_ref()
            .unwrap()
            .symbols
            .iter()
            .all(|symbol| symbol.target.is_none())
    );
}

#[tokio::test]
async fn index_resolves_task_values_outputs_and_conditional_completion() {
    let mut source = OverlaySource::default();
    let text = "workspace: { name: app, version: 1.0.0 }\nstages: []\nvalues: { label: workspace }\ntasks:\n  build:\n    values: { label: local }\n    steps:\n      - uses: test/action@v1\n        with:\n          format: arch\n          label: '${label}'\n  publish:\n    needs: [build]\n    steps:\n      - uses: test/action@v1\n        with: { label: '${build.artifact}' }\n";
    source.insert(".evin/workspace.flow", text);
    let index = WorkspaceSemanticIndex::build(WorkspaceSnapshot::load(&source).await, &actions());
    let value_reference = index
        .symbols
        .iter()
        .find(|symbol| symbol.kind == SemanticKind::TemplateValue)
        .unwrap();
    let target = index
        .definition_at(&value_reference.range.path, value_reference.range.start)
        .unwrap();
    assert_eq!(target.path, PathBuf::from(".evin/workspace.flow"));
    assert!(
        index.symbols.iter().any(
            |symbol| symbol.kind == SemanticKind::TemplateTaskOutput && symbol.target.is_some()
        )
    );
    let offset = text.find("format: arch").unwrap() + "format: arch".len();
    let completions = index.completions_at(Path::new(".evin/workspace.flow"), offset, &actions());
    assert!(
        completions
            .iter()
            .any(|candidate| candidate.label == "arch")
    );
    assert!(
        !completions
            .iter()
            .any(|candidate| candidate.label == "arch_only" || candidate.label == "tar_only")
    );
    let fields_offset = text.find("label: '${label}'").unwrap();
    let fields = index.completions_at(Path::new(".evin/workspace.flow"), fields_offset, &actions());
    assert!(
        fields
            .iter()
            .any(|candidate| candidate.label == "arch_only"),
        "{fields:#?}"
    );
    assert!(!fields.iter().any(|candidate| candidate.label == "tar_only"));
    assert_eq!(
        index
            .symbols
            .iter()
            .find(|symbol| symbol.kind == SemanticKind::Action)
            .and_then(|symbol| symbol.catalog_target.as_ref())
            .map(CatalogTarget::as_str),
        Some("action.test/action@v1")
    );
    assert_eq!(
        index
            .symbols
            .iter()
            .find(|symbol| symbol.kind == SemanticKind::ActionField && symbol.name == "format")
            .and_then(|symbol| symbol.catalog_target.as_ref())
            .map(CatalogTarget::as_str),
        Some("action.test/action@v1.with.format")
    );
    let root_completions = index.completions_at(Path::new(".evin/workspace.flow"), 0, &actions());
    assert!(
        !root_completions
            .iter()
            .any(|candidate| candidate.label == "workspace")
    );
    assert!(
        root_completions
            .iter()
            .any(|candidate| candidate.label == "expose")
    );
    assert!(
        !root_completions
            .iter()
            .any(|candidate| candidate.label == "tasks")
    );
}

#[test]
fn configuration_keys_share_canonical_completion_and_hover_targets() {
    let documents = [
        (
            ".evin/workspace.flow",
            DocumentKind::Workspace,
            "workspace: {}\nstages: []\nenv: {}\nvalues: {}\ntasks: {}\nexpose: []\n",
            &[
                "workspace",
                "workspace.stages",
                "workspace.env",
                "workspace.values",
                "workspace.tasks",
                "workspace.expose",
            ][..],
        ),
        (
            "modules/api/.evin/module.flow",
            DocumentKind::Module,
            "module: {}\nenv: {}\nvalues: {}\ntasks: {}\n",
            &["module", "module.env", "module.values", "module.tasks"][..],
        ),
        (
            ".evin/release.flow",
            DocumentKind::Namespace,
            "namespace: {}\nenv: {}\nvalues: {}\ntasks: {}\n",
            &[
                "namespace",
                "namespace.env",
                "namespace.values",
                "namespace.tasks",
            ][..],
        ),
        (
            ".evin/environment.flow",
            DocumentKind::Environment,
            "version: 1\npresets: {}\nenvironments: {}\n",
            &[
                "environment.version",
                "environment.presets",
                "environment.environments",
            ][..],
        ),
        (
            ".evin/gitrules.flow",
            DocumentKind::GitRules,
            "releases: {}\n",
            &["GitRules.releases"][..],
        ),
    ];
    let mut snapshot = WorkspaceSnapshot {
        config: None,
        documents: BTreeMap::new(),
        diagnostics: Vec::new(),
    };
    for (path, kind, text, _) in documents {
        snapshot.documents.insert(
            PathBuf::from(path),
            WorkspaceDocument {
                path: PathBuf::from(path),
                kind,
                header: None,
                text: text.into(),
                ast: Some(evin_flow::parse_yaml(text).unwrap()),
                completion_ast: None,
                diagnostics: Vec::new(),
            },
        );
    }
    let index = WorkspaceSemanticIndex::build(snapshot, &HashMap::new());

    for (path, kind, text, expected) in documents {
        let completion_targets = index
            .completions_at(Path::new(path), 0, &HashMap::new())
            .into_iter()
            .filter_map(|candidate| candidate.catalog_target)
            .map(|target| target.0)
            .collect::<BTreeSet<_>>();
        for target in expected {
            assert!(
                !completion_targets.contains(*target),
                "already-present field should not be completed: {target}"
            );
            let key = target.rsplit('.').next().unwrap();
            let offset = text.find(key).unwrap();
            assert_eq!(
                index
                    .hover_at(Path::new(path), offset)
                    .and_then(|info| info.catalog_target)
                    .as_ref()
                    .map(CatalogTarget::as_str),
                Some(*target),
                "missing hover target {target} in {kind:?}",
            );
        }
    }
    let catalog_targets = configuration_catalog_targets();
    let targets = catalog_targets
        .iter()
        .map(CatalogTarget::as_str)
        .collect::<BTreeSet<_>>();
    assert!(targets.is_superset(&BTreeSet::from([
        "GitRules",
        "GitRules.authorship",
        "GitRules.branches",
        "GitRules.commits",
        "GitRules.description",
        "GitRules.files",
        "GitRules.history",
        "GitRules.lfs",
        "GitRules.name",
        "GitRules.pull_requests",
        "GitRules.releases",
        "GitRules.remotes",
        "GitRules.submodules",
        "GitRules.tags",
        "GitRules.version",
        "environment",
        "environment.default",
        "environment.description",
        "environment.environments",
        "environment.pattern",
        "environment.presets",
        "environment.required",
        "environment.scopes",
        "environment.stages",
        "environment.type",
        "environment.version",
        "module",
        "module.env",
        "module.include",
        "module.name",
        "module.tasks",
        "module.values",
        "module.version",
        "namespace",
        "namespace.description",
        "namespace.env",
        "namespace.name",
        "namespace.tasks",
        "namespace.values",
        "task.cwd",
        "task.description",
        "task.env",
        "task.needs",
        "task.on",
        "task.on.hooks",
        "task.parallel",
        "task.presets",
        "task.pure",
        "task.run",
        "task.stage",
        "task.steps",
        "task.steps.run",
        "task.steps.uses",
        "task.steps.with",
        "task.values",
        "task.watch",
        "task.watch.debounce",
        "task.watch.ignore",
        "task.watch.paths",
        "workspace",
        "workspace.env",
        "workspace.execution",
        "workspace.expose",
        "workspace.hook_runner",
        "workspace.hook_runner.args",
        "workspace.hook_runner.command",
        "workspace.include",
        "workspace.install_hint",
        "workspace.modules",
        "workspace.name",
        "workspace.rules",
        "workspace.stages",
        "workspace.tasks",
        "workspace.values",
        "workspace.version",
    ])));
    for target in [
        "GitRules.commits.message_format",
        "GitRules.commits.fields.*.allowed_values",
        "GitRules.commits.policies.require_body",
        "GitRules.releases.versioning.strategy",
        "GitRules.releases.packages.*.revision.scope",
    ] {
        assert!(targets.contains(target), "missing schema target {target}");
    }
}

#[tokio::test]
async fn task_schema_completion_and_hover_are_contextual() {
    let mut source = OverlaySource::default();
    let text = "workspace: { name: app, version: 1.0.0 }\nstages: [Test]\ntasks:\n  test:\n    stage: Test\n    on: {}\n    watch: {}\n    steps:\n      - {}\n";
    source.insert(".evin/workspace.flow", text);
    let index = WorkspaceSemanticIndex::build(WorkspaceSnapshot::load(&source).await, &actions());
    let task_offset = text.find("stage: Test").unwrap();
    assert!(
        index
            .schema_contexts
            .iter()
            .any(|task| task.path == "workspace.tasks.*"
                && task
                    .range
                    .contains(Path::new(".evin/workspace.flow"), task_offset)),
        "{:#?}",
        index.schema_contexts
    );
    let candidates =
        index.completions_at(Path::new(".evin/workspace.flow"), task_offset, &actions());
    assert!(
        candidates.iter().any(|candidate| {
            candidate.label == "description"
                && candidate
                    .catalog_target
                    .as_ref()
                    .is_some_and(|target| target.as_str() == "task.description")
        }),
        "{candidates:#?}"
    );
    assert!(
        !candidates
            .iter()
            .any(|candidate| candidate.label == "workspace")
    );

    let on_offset = text.find("on: {}").unwrap() + 4;
    assert!(
        index
            .completions_at(Path::new(".evin/workspace.flow"), on_offset, &actions())
            .iter()
            .any(|candidate| candidate.label == "hooks")
    );
    let watch_offset = text.find("watch: {}").unwrap() + 7;
    assert!(
        index
            .completions_at(Path::new(".evin/workspace.flow"), watch_offset, &actions())
            .iter()
            .any(|candidate| candidate.label == "paths")
    );
    let step_offset = text.find("- {}").unwrap() + 3;
    assert!(
        index
            .completions_at(Path::new(".evin/workspace.flow"), step_offset, &actions())
            .iter()
            .any(|candidate| {
                candidate.label == "uses"
                    && candidate
                        .catalog_target
                        .as_ref()
                        .is_some_and(|target| target.as_str() == "task.steps.uses")
            })
    );

    assert_eq!(
        index
            .hover_at(Path::new(".evin/workspace.flow"), task_offset)
            .and_then(|info| info.catalog_target)
            .as_ref()
            .map(CatalogTarget::as_str),
        Some("task.stage")
    );
}

#[tokio::test]
async fn reference_completion_is_scoped_and_unknown_positions_are_empty() {
    let mut source = OverlaySource::default();
    let text = "workspace:\n  name: app\n  version: 1.0.0\n  include: { release: .evin/release.flow, web: .evin/web.flow }\nstages: [Build, Test]\nvalues: { root_value: root }\ntasks:\n  producer:\n    stage: Build\n    steps:\n      - uses: test/action@v1\n        with: { label: producer }\n  consumer:\n    stage: Bu\n    needs: [rel, release:]\n    run: echo ${unused}\n    # comment\n";
    source.insert(".evin/workspace.flow", text);
    source.insert(
        ".evin/release.flow",
        "namespace: { name: release }\ntasks: { build: { stage: Build, run: echo release } }\n",
    );
    source.insert(
        ".evin/web.flow",
        "namespace: { name: web }\ntasks: { build: { stage: Build, run: echo web } }\n",
    );
    source.insert(
        ".evin/environment.flow",
        "version: 1\nenvironments: { PORT: { type: number } }\n",
    );
    let index = WorkspaceSemanticIndex::build(WorkspaceSnapshot::load(&source).await, &actions());

    let needs_cursor = text.find("needs: [rel,").unwrap() + "needs: [rel".len();
    let needs = index.completions_at(Path::new(".evin/workspace.flow"), needs_cursor, &actions());
    let release = needs
        .iter()
        .find(|candidate| candidate.label == "release:build")
        .unwrap();
    assert!(
        needs
            .iter()
            .all(|candidate| !candidate.label.starts_with("web:"))
    );
    assert_eq!(
        &text[release.replacement.as_ref().unwrap().start
            ..release.replacement.as_ref().unwrap().end],
        "rel"
    );
    let qualified_cursor = text.find("release:]").unwrap() + "release:".len();
    let qualified = index.completions_at(
        Path::new(".evin/workspace.flow"),
        qualified_cursor,
        &actions(),
    );
    assert!(
        qualified
            .iter()
            .any(|candidate| candidate.label == "release:build")
    );
    assert!(
        !qualified
            .iter()
            .any(|candidate| candidate.label.starts_with("web:"))
    );

    let stage_cursor = text.find("stage: Bu").unwrap() + "stage: Bu".len();
    let stages = index.completions_at(Path::new(".evin/workspace.flow"), stage_cursor, &actions());
    assert_eq!(
        stages
            .iter()
            .map(|candidate| candidate.label.as_str())
            .collect::<Vec<_>>(),
        ["Build"]
    );

    let run_cursor = text.find("echo ${unused}").unwrap() + "echo ${unused}".len();
    assert!(
        index
            .completions_at(Path::new(".evin/workspace.flow"), run_cursor, &actions())
            .is_empty()
    );

    let needs_comment = text.find("# comment").unwrap() + 2;
    assert!(
        index
            .completions_at(Path::new(".evin/workspace.flow"), needs_comment, &actions())
            .is_empty()
    );
}

#[tokio::test]
async fn environment_hook_and_boolean_values_are_contextual() {
    let mut source = OverlaySource::default();
    let text = "workspace: { name: app, version: 1.0.0 }\nstages: [Build]\ntasks:\n  build:\n    stage: Build\n    env: [PO]\n    presets: [ru]\n    on: { hooks: [pre] }\n    pure: tr\n";
    source.insert(".evin/workspace.flow", text);
    source.insert(
        ".evin/environment.flow",
        "version: 1\npresets: { rust-web: [PORT] }\nenvironments: { PORT: { type: number } }\n",
    );
    let index = WorkspaceSemanticIndex::build(WorkspaceSnapshot::load(&source).await, &actions());
    let labels_at = |needle: &str, prefix: &str| {
        let offset = text.find(needle).unwrap() + needle.len();
        index
            .completions_at(Path::new(".evin/workspace.flow"), offset, &actions())
            .into_iter()
            .map(|candidate| candidate.label)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .filter(|candidate| candidate.starts_with(prefix))
            .collect::<Vec<_>>()
    };
    assert!(labels_at("env: [PO", "PORT").contains(&"PORT".to_owned()));
    assert!(labels_at("presets: [ru", "rust").contains(&"rust".to_owned()));
    assert!(labels_at("hooks: [pre", "pre-").contains(&"pre-commit".to_owned()));
    assert_eq!(labels_at("pure: tr", "true"), ["true"]);
}

#[tokio::test]
async fn environment_definitions_and_exposed_tasks_have_typed_completion() {
    let mut source = OverlaySource::default();
    let workspace = "workspace: { name: app, version: 1.0.0, include: { release: .evin/release.flow } }\nstages: [Build]\ntasks: { build: { stage: Build, run: echo } }\nexpose: [rel]\n";
    let environment = "version: 1\npresets: { rust-web: [PO] }\nenvironments: { PORT: { type: number, stages: [Bu] } }\n";
    source.insert(".evin/workspace.flow", workspace);
    source.insert(
        ".evin/release.flow",
        "namespace: { name: release }\ntasks: { build: { stage: Build, run: echo } }\n",
    );
    source.insert(".evin/environment.flow", environment);
    let index = WorkspaceSemanticIndex::build(WorkspaceSnapshot::load(&source).await, &actions());

    let labels_at = |path: &str, text: &str, token: &str| {
        let cursor = text.find(token).unwrap() + token.len();
        index
            .completions_at(Path::new(path), cursor, &actions())
            .into_iter()
            .map(|candidate| candidate.label)
            .collect::<BTreeSet<_>>()
    };
    assert!(labels_at(".evin/workspace.flow", workspace, "expose: [rel").contains("release:build"));
    assert!(
        labels_at(
            ".evin/environment.flow",
            environment,
            "presets: { rust-web: [PO"
        )
        .contains("PORT")
    );
    assert!(labels_at(".evin/environment.flow", environment, "stages: [Bu").contains("Build"));
    assert!(!labels_at(".evin/environment.flow", environment, "stages: [Bu").contains("rust-web"));
}

#[tokio::test]
async fn module_task_completion_respects_local_root_and_nested_namespace_scopes() {
    let mut source = OverlaySource::default();
    source.insert(
            ".evin/workspace.flow",
            "workspace: { name: app, version: 1.0.0, modules: [modules/api] }\nstages: [Build]\ntasks: { build: { stage: Build, run: echo root } }\n",
        );
    source.insert(
            "modules/api/.evin/module.flow",
            "module: { name: api, include: { release: .evin/release.flow } }\ntasks:\n  build: { stage: Build, run: echo module }\n  test:\n    stage: Build\n    needs: [bui, app, api:rel]\n    run: echo test\n",
        );
    source.insert(
        "modules/api/.evin/release.flow",
        "namespace: { name: release }\ntasks: { publish: { stage: Build, run: echo publish } }\n",
    );
    let index = WorkspaceSemanticIndex::build(WorkspaceSnapshot::load(&source).await, &actions());
    let text = &index
        .document(Path::new("modules/api/.evin/module.flow"))
        .unwrap()
        .text;
    let local_cursor = text.find("needs: [bui").unwrap() + "needs: [bui".len();
    let local = index.completions_at(
        Path::new("modules/api/.evin/module.flow"),
        local_cursor,
        &actions(),
    );
    assert!(local.iter().any(|candidate| candidate.label == "build"));
    assert!(
        !local
            .iter()
            .any(|candidate| candidate.label == "release:publish")
    );

    let root_cursor = text.find("app, api:rel").unwrap() + "app".len();
    let root = index.completions_at(
        Path::new("modules/api/.evin/module.flow"),
        root_cursor,
        &actions(),
    );
    assert!(root.iter().any(|candidate| candidate.label == "app:build"));

    let namespace_cursor = text.find("api:rel]").unwrap() + "api:rel".len();
    let namespace = index.completions_at(
        Path::new("modules/api/.evin/module.flow"),
        namespace_cursor,
        &actions(),
    );
    assert!(
        namespace
            .iter()
            .any(|candidate| candidate.label == "api:release:publish"),
        "{namespace:#?}"
    );
}

#[tokio::test]
async fn task_output_template_completion_uses_declared_prerequisites() {
    let mut source = OverlaySource::default();
    let text = "workspace: { name: app, version: 1.0.0 }\nstages: [Build]\ntasks:\n  producer:\n    stage: Build\n    steps:\n      - uses: test/action@v1\n  consumer:\n    stage: Build\n    needs: [producer]\n    steps:\n      - uses: test/action@v1\n        with: { label: '${pro}' }\n      - uses: test/action@v1\n        with: { label: '${producer.ar}' }\n      - uses: test/action@v1\n        with: { label: '${env.PO}' }\n";
    source.insert(".evin/workspace.flow", text);
    source.insert(
        ".evin/environment.flow",
        "version: 1\nenvironments: { PORT: { type: number } }\n",
    );
    let index = WorkspaceSemanticIndex::build(WorkspaceSnapshot::load(&source).await, &actions());
    let cursor = text.find("${pro}").unwrap() + "${pro".len();
    let candidates = index.completions_at(Path::new(".evin/workspace.flow"), cursor, &actions());
    let output = candidates
        .iter()
        .find(|candidate| candidate.label == "producer.artifact")
        .unwrap();
    let replacement = output.replacement.as_ref().unwrap();
    assert_eq!(&text[replacement.start..replacement.end], "pro");
    assert_eq!(output.snippet, "producer.artifact");

    let cursor = text.find("${producer.ar}").unwrap() + "${producer.ar".len();
    let candidates = index.completions_at(Path::new(".evin/workspace.flow"), cursor, &actions());
    let output = candidates
        .iter()
        .find(|candidate| candidate.label == "producer.artifact")
        .unwrap();
    let replacement = output.replacement.as_ref().unwrap();
    assert_eq!(&text[replacement.start..replacement.end], "ar");
    assert_eq!(output.snippet, "artifact");

    let cursor = text.find("${env.PO}").unwrap() + "${env.PO".len();
    let candidates = index.completions_at(Path::new(".evin/workspace.flow"), cursor, &actions());
    let environment = candidates
        .iter()
        .find(|candidate| candidate.label == "env.PORT")
        .unwrap();
    let replacement = environment.replacement.as_ref().unwrap();
    assert_eq!(&text[replacement.start..replacement.end], "PO");
    assert_eq!(environment.snippet, "PORT");
}

#[tokio::test]
async fn path_completion_is_limited_to_module_and_namespace_path_fields() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(directory.path().join(".evin")).unwrap();
    std::fs::create_dir_all(directory.path().join("modules/api/.evin")).unwrap();
    std::fs::write(
        directory.path().join("modules/api/.evin/module.flow"),
        "module: { name: api }\ntasks: {}\n",
    )
    .unwrap();
    std::fs::write(
        directory.path().join(".evin/candidate.flow"),
        "#!evin/v1\nnamespace: { name: candidate }\ntasks: {}\n",
    )
    .unwrap();
    let text = "workspace:\n  name: app\n  version: 1.0.0\n  modules: [modules/a]\n  include: [.evin/cand]\nstages: []\n";
    std::fs::write(directory.path().join(".evin/workspace.flow"), text).unwrap();
    let source = OverlaySource::for_root(directory.path());
    let index = WorkspaceSemanticIndex::build(WorkspaceSnapshot::load(&source).await, &actions());

    let module_cursor = text.find("modules/a").unwrap() + "modules/a".len();
    let modules = index.completions_at_with_source(
        Path::new(".evin/workspace.flow"),
        module_cursor,
        &actions(),
        &source,
    );
    assert!(
        modules
            .iter()
            .any(|candidate| candidate.label == "modules/api")
    );
    assert!(
        !modules
            .iter()
            .any(|candidate| candidate.label.ends_with(".flow"))
    );

    let namespace_cursor = text.find(".evin/cand").unwrap() + ".evin/cand".len();
    let namespaces = index.completions_at_with_source(
        Path::new(".evin/workspace.flow"),
        namespace_cursor,
        &actions(),
        &source,
    );
    assert!(
        namespaces
            .iter()
            .any(|candidate| candidate.label == ".evin/candidate.flow")
    );
}

#[tokio::test]
async fn completion_recovers_an_unclosed_flow_list_from_the_current_buffer() {
    let mut source = OverlaySource::default();
    let text = "workspace: { name: app, version: 1.0.0, include: { release: .evin/release.flow } }\nstages: [Build]\ntasks:\n  publish:\n    stage: Build\n    needs: [rel\n";
    source.insert(".evin/workspace.flow", text);
    source.insert(
        ".evin/release.flow",
        "namespace: { name: release }\ntasks: { build: { stage: Build, run: echo build } }\n",
    );
    let snapshot = WorkspaceSnapshot::load(&source).await;
    let document = &snapshot.documents[Path::new(".evin/workspace.flow")];
    assert!(document.ast.is_none());
    assert!(document.completion_ast.is_some());
    assert!(!snapshot.diagnostics.is_empty());
    let index = WorkspaceSemanticIndex::build(snapshot, &actions());
    let cursor = text.find("needs: [rel").unwrap() + "needs: [rel".len();
    assert!(
        index
            .completions_at(Path::new(".evin/workspace.flow"), cursor, &actions())
            .iter()
            .any(|candidate| candidate.label == "release:build")
    );
}

#[test]
fn environment_variable_fields_index_catalog_targets() {
    let text = "version: 1\nenvironments:\n  LOG_LEVEL:\n    type: string\n    required: true\n";
    let mut snapshot = WorkspaceSnapshot {
        config: None,
        documents: BTreeMap::new(),
        diagnostics: Vec::new(),
    };
    snapshot.documents.insert(
        PathBuf::from(".evin/environment.flow"),
        WorkspaceDocument {
            path: PathBuf::from(".evin/environment.flow"),
            kind: DocumentKind::Environment,
            header: None,
            text: text.into(),
            ast: Some(evin_flow::parse_yaml(text).unwrap()),
            completion_ast: None,
            diagnostics: Vec::new(),
        },
    );
    let index = WorkspaceSemanticIndex::build(snapshot, &HashMap::new());
    let type_offset = text.find("type:").unwrap();
    assert_eq!(
        index
            .hover_at(Path::new(".evin/environment.flow"), type_offset)
            .and_then(|info| info.catalog_target)
            .as_ref()
            .map(CatalogTarget::as_str),
        Some("environment.type")
    );
    let req_offset = text.find("required:").unwrap();
    assert_eq!(
        index
            .hover_at(Path::new(".evin/environment.flow"), req_offset)
            .and_then(|info| info.catalog_target)
            .as_ref()
            .map(CatalogTarget::as_str),
        Some("environment.required")
    );
}

#[test]
fn workspace_metadata_fields_index_catalog_targets() {
    let text = "workspace:\n  name: app\n  install_hint: \"cargo install\"\n  hook_runner:\n    command: cargo\n";
    let mut snapshot = WorkspaceSnapshot {
        config: None,
        documents: BTreeMap::new(),
        diagnostics: Vec::new(),
    };
    snapshot.documents.insert(
        PathBuf::from(".evin/workspace.flow"),
        WorkspaceDocument {
            path: PathBuf::from(".evin/workspace.flow"),
            kind: DocumentKind::Workspace,
            header: None,
            text: text.into(),
            ast: Some(evin_flow::parse_yaml(text).unwrap()),
            completion_ast: None,
            diagnostics: Vec::new(),
        },
    );
    let index = WorkspaceSemanticIndex::build(snapshot, &HashMap::new());
    let hint_offset = text.find("install_hint").unwrap();
    assert_eq!(
        index
            .hover_at(Path::new(".evin/workspace.flow"), hint_offset)
            .and_then(|info| info.catalog_target)
            .as_ref()
            .map(CatalogTarget::as_str),
        Some("workspace.install_hint")
    );
    let cmd_offset = text.find("command").unwrap();
    assert_eq!(
        index
            .hover_at(Path::new(".evin/workspace.flow"), cmd_offset)
            .and_then(|info| info.catalog_target)
            .as_ref()
            .map(CatalogTarget::as_str),
        Some("workspace.hook_runner.command")
    );
}

#[test]
fn comments_do_not_trigger_hover() {
    let path = PathBuf::from(".evin/gitrules.flow");
    let text = "#!evin/v1\nversion: 1\nname: Git Development Standard (Canary)\n\ncommits:\n  # The conventional commit format used for all commit messages.\n  # Can be a single string or an array of acceptable formats.\n  message_format:\n    - \"{type}({scope}?){breaking?}: {description}\"\n    - \"{type}: {description}\"\n\n  # Commits matching these regex patterns will bypass validation.\n  ignore_patterns:\n    - \"^Merge branch\"\n    - \"^Revert\"\n\n  # Enforce specific rules for different named capturing groups in your format.\n  fields:\n    type:\n      # The allowed semantic commit types.\n      allowed_values: [feat, fix, docs, style, refactor, perf, test, build, ci, chore, revert]\n      # Enforce a specific casing style.\n      must_be: lowercase-with-hyphens\n    description:\n      max_length: 72\n\n  policies:\n    # Do we require a commit body?\n    require_body: false # inline comment\n    # Do we require an issue tracker link in the footer?\n    require_ticket_reference: false\n";
    let snapshot = WorkspaceSnapshot {
        config: None,
        documents: BTreeMap::from([(
            path.clone(),
            WorkspaceDocument {
                path: path.clone(),
                kind: DocumentKind::GitRules,
                header: None,
                text: text.into(),
                ast: Some(evin_flow::parse_yaml(text).unwrap()),
                completion_ast: None,
                diagnostics: Vec::new(),
            },
        )]),
        diagnostics: Vec::new(),
    };
    let index = WorkspaceSemanticIndex::build(snapshot, &HashMap::new());

    // Comments must never trigger hover
    for line in text.lines() {
        if line.trim_start().starts_with('#') {
            let offset = text.find(line).unwrap() + line.find('#').unwrap();
            let hover = index.hover_at(&path, offset);
            assert!(
                hover.is_none(),
                "comment line {:?} returned hover {:?}",
                line,
                hover
            );
        }
    }

    // Inline comments must not trigger hover
    let inline_comment_offset = text.find("# inline comment").unwrap();
    assert!(index.hover_at(&path, inline_comment_offset).is_none());

    // Header must not trigger hover
    assert!(index.hover_at(&path, 0).is_none());

    // Valid fields and values must continue to have hover
    for key in [
        "commits",
        "message_format",
        "allowed_values",
        "require_body",
    ] {
        let offset = text.find(key).unwrap();
        let hover = index.hover_at(&path, offset);
        assert!(hover.is_some(), "key {} should have hover", key);
    }
    let false_offset = text.find("false").unwrap();
    assert!(index.hover_at(&path, false_offset).is_some());
}

/// Comments may appear anywhere: between entries at any indentation,
/// after keys, after scalar values, after flow collections, and after
/// plain scalars that contain apostrophes or `#` characters. No byte of
/// any comment may produce a hover.
#[test]
fn no_comment_byte_produces_a_hover_wherever_it_is_placed() {
    let base = "version: 1\nname: Don't panic (Canary)\ncommits:\n  message_format:\n    - \"{type}: {description}\"\n    - '{type}: it''s'\n  ignore_patterns:\n    - ^Merge#branch\n  fields:\n    type:\n      allowed_values: [feat, fix]\n      must_be: lowercase-with-hyphens\n  policies:\n    require_body: false\nreleases:\n  change_types:\n    feat:\n      bump: minor\n      changelog_section: Features\n  commit_message: \"chore(release): {version}\"\n";
    // Build a document with a comment line before every line (at the
    // line's indentation and at column 0) and a trailing comment after
    // every line.
    let mut text = String::from("#!evin/v1\n");
    for line in base.lines() {
        let indent = &line[..line.len() - line.trim_start().len()];
        text.push_str(&format!("{indent}# it's a comment: key: value\n"));
        text.push_str("# column zero comment - item\n");
        text.push_str(&format!("{line} # trailing 'quoted' comment: x\n"));
    }
    let path = PathBuf::from(".evin/gitrules.flow");
    let snapshot = WorkspaceSnapshot {
        config: None,
        documents: BTreeMap::from([(
            path.clone(),
            WorkspaceDocument {
                path: path.clone(),
                kind: DocumentKind::GitRules,
                header: None,
                text: text.clone(),
                ast: Some(evin_flow::parse_yaml(&text).unwrap()),
                completion_ast: None,
                diagnostics: Vec::new(),
            },
        )]),
        diagnostics: Vec::new(),
    };
    let index = WorkspaceSemanticIndex::build(snapshot, &HashMap::new());
    let mut failures = Vec::new();
    let mut line_start = 0;
    for line in text.split_inclusive('\n') {
        let comment_start = if line.trim_start().starts_with('#') {
            Some(0)
        } else {
            line.find(" # ").map(|position| position + 1)
        };
        if let Some(start) = comment_start {
            let end = line.trim_end_matches('\n').len();
            for column in start..end {
                if let Some(hover) = index.hover_at(&path, line_start + column) {
                    failures.push(format!("{line:?} col {column}: {}", hover.markdown));
                    break;
                }
            }
        }
        line_start += line.len();
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    for key in ["message_format", "require_body", "bump", "commit_message"] {
        let offset = text.find(&format!("{key}:")).unwrap();
        assert!(index.hover_at(&path, offset).is_some(), "{key}");
    }
}

#[test]
fn test_exact_gitrules_file() {
    let content = std::fs::read_to_string("../../.evin/gitrules.flow").unwrap();
    let path = PathBuf::from(".evin/gitrules.flow");
    let doc = WorkspaceDocument {
        path: path.clone(),
        kind: DocumentKind::GitRules,
        header: None,
        text: content.clone(),
        ast: evin_flow::parse_yaml(&content).ok(),
        completion_ast: None,
        diagnostics: Vec::new(),
    };
    let snapshot = WorkspaceSnapshot {
        config: None,
        documents: BTreeMap::from([(path.clone(), doc.clone())]),
        diagnostics: Vec::new(),
    };
    let index = WorkspaceSemanticIndex::build(snapshot, &HashMap::new());
    let b_offset = content.find("branches:").unwrap();
    let tags_offset = content.find("tags:").unwrap();
    for off in b_offset..tags_offset {
        let char_at = &content[off..off + 1];
        if char_at == " " || char_at == "\n" {
            assert!(
                index.hover_at(&path, off).is_none(),
                "Whitespace at offset {off} ({char_at:?}) must never trigger hover",
            );
        }
    }
    // Hover directly on the key "branches" works and has tight token range
    let branches_hover = index.hover_at(&path, b_offset).unwrap();
    assert_eq!(branches_hover.range.len(), "branches".len());
    // Colon after "branches:" does not trigger hover
    let colon_offset = b_offset + "branches".len();
    assert!(index.hover_at(&path, colon_offset).is_none());
    let mut failures = Vec::new();
    let mut offset = 0;
    for (line_no, line) in content.lines().enumerate() {
        if let Some(_hash_pos) = evin_flow::comment_start_in_line(line) {
            let check_start = if line.trim_start().starts_with('#') {
                0
            } else {
                _hash_pos
            };
            for col in check_start..line.len() {
                let char_offset = offset + col;
                let is_comment = doc.is_comment_at(char_offset);
                let hover = index.hover_at(&path, char_offset);
                if !is_comment || hover.is_some() {
                    failures.push(format!(
                        "Line {} col {}: {:?} (is_comment={is_comment}, hover={hover:?})",
                        line_no + 1,
                        col,
                        &line[col..],
                    ));
                }
            }
        }
        offset += line.len() + 1; // +1 for '\n'
    }
    assert!(failures.is_empty(), "Failures:\n{}", failures.join("\n"));
}
