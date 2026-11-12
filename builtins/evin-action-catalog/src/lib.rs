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

use evin_action_sdk::{
    Action, ActionDescriptor, ActionFieldDescriptor, ActionFieldKind, ActionOutputDescriptor,
    RunnerContext,
};

const DESCRIPTOR: ActionDescriptor = ActionDescriptor {
    summary: "Compile documentation symbols and keywords into a tooling catalog.",
    fields: &[
        ActionFieldDescriptor {
            path: "source_dir",
            kind: ActionFieldKind::String,
            required: false,
            description: "Markdown and reStructuredText source directory.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "out",
            kind: ActionFieldKind::String,
            required: false,
            description: "Generated catalog JSON path.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "include",
            kind: ActionFieldKind::List,
            required: false,
            description: "Included documentation paths or globs.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "exclude",
            kind: ActionFieldKind::List,
            required: false,
            description: "Excluded documentation paths or globs.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "extract_frontmatter",
            kind: ActionFieldKind::Boolean,
            required: false,
            description: "Extract Markdown YAML frontmatter.",
            condition: None,
        },
        ActionFieldDescriptor {
            path: "reference_prefix",
            kind: ActionFieldKind::String,
            required: false,
            description: "Formal reference identifier prefix.",
            condition: None,
        },
    ],
    outputs: &[
        ActionOutputDescriptor {
            name: "output_file",
            description: "Generated catalog JSON path.",
        },
        ActionOutputDescriptor {
            name: "symbol_count",
            description: "Number of exported symbols and keywords.",
        },
    ],
};

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogConfig {
    #[serde(default = "default_source")]
    source_dir: std::path::PathBuf,
    #[serde(default = "default_output")]
    out: std::path::PathBuf,
    #[serde(default)]
    include: Vec<String>,
    #[serde(default)]
    exclude: Vec<String>,
    #[serde(default = "default_true")]
    extract_frontmatter: bool,
    #[serde(default = "default_reference_prefix")]
    reference_prefix: String,
}

fn default_source() -> std::path::PathBuf {
    "docs".into()
}

fn default_output() -> std::path::PathBuf {
    "dist/catalog.json".into()
}

fn default_reference_prefix() -> String {
    "REF".into()
}

fn default_true() -> bool {
    true
}

fn issue(message: impl Into<String>, code: &'static str) -> evin_diagnostic::Diagnostic {
    evin_diagnostic::Diagnostic::new(message, evin_diagnostic::IssueCode::new(code))
}

#[allow(clippy::result_large_err)]
fn compile(
    workspace: &std::path::Path,
    config: &CatalogConfig,
) -> Result<(evin_doc::CatalogExport, usize), evin_diagnostic::Diagnostic> {
    let source_root = if config.source_dir.is_absolute() {
        config.source_dir.clone()
    } else {
        workspace.join(&config.source_dir)
    };
    let files =
        evin_doc::SourceResolver::resolve_all(&source_root, &config.include, &config.exclude)
            .map_err(|error| issue(error.message, error.code))?;
    let mut sources = Vec::with_capacity(files.len());
    for file in files {
        let content = std::fs::read_to_string(&file.absolute_path).map_err(|error| {
            issue(
                format!("Failed to read {}: {error}", file.absolute_path.display()),
                "io-error",
            )
        })?;
        sources.push((file, content));
    }
    let inputs = sources
        .iter()
        .map(|(file, content)| evin_doc::DocumentInput {
            slug: file.slug.clone(),
            source_path: file.relative_path.to_string_lossy().to_string(),
            format: if file.is_rst {
                evin_doc::DocFormat::ReStructuredText
            } else {
                evin_doc::DocFormat::Markdown
            },
            is_start_page: matches!(
                file.relative_path.to_str(),
                Some("index.md") | Some("index.rst")
            ),
            content,
        });
    let prefix = config.reference_prefix.trim().to_ascii_uppercase();
    if prefix.is_empty() || !prefix.chars().all(|ch| ch.is_ascii_alphanumeric()) {
        return Err(issue(
            "reference_prefix must contain only ASCII letters and digits",
            "invalid-config",
        ));
    }
    let site = evin_doc::compile_site(
        inputs,
        evin_doc::CompileOptions {
            extract_frontmatter: config.extract_frontmatter,
            reference_prefix: prefix,
            site_version: "1.0".into(),
        },
        None,
    )
    .map_err(|error| issue(error.message, error.code))?;
    let count = site.pages.len();
    let generated_at = std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .and_then(|seconds| chrono::DateTime::from_timestamp(seconds, 0))
        .map(|value| value.to_rfc3339())
        .unwrap_or_else(|| "1970-01-01T00:00:00+00:00".into());
    let catalog = evin_doc::build_lsp_catalog(
        &site,
        format!("catalog-generator@{}", env!("CARGO_PKG_VERSION")),
        generated_at,
    );
    catalog.validate().map_err(|error| {
        issue(
            format!("Generated catalog is invalid: {error}"),
            "invalid-catalog",
        )
    })?;
    Ok((catalog, count))
}

pub struct CatalogAction;

#[async_trait::async_trait]
impl Action for CatalogAction {
    fn editor_descriptor(&self) -> Option<&'static ActionDescriptor> {
        Some(&DESCRIPTOR)
    }

    fn validate_config(
        &self,
        with: Option<&evin_flow::SpannedValue>,
    ) -> Result<(), evin_flow::ConfigError> {
        evin_action_sdk::parse_config::<CatalogConfig>(with).map(|_| ())
    }

    async fn execute(
        &self,
        ctx: &RunnerContext,
        with: Option<evin_flow::SpannedValue>,
    ) -> Result<(), evin_diagnostic::Diagnostic> {
        let config: CatalogConfig = ctx.load_config(with.as_ref()).map_err(|error| {
            issue(
                format!("Invalid catalog configuration: {}", error.message),
                "invalid-config",
            )
        })?;
        let (catalog, page_count) = compile(ctx.fs.workspace_root(), &config)?;
        let output = if config.out.is_absolute() {
            config.out
        } else {
            ctx.fs.workspace_root().join(config.out)
        };
        let json = serde_json::to_string_pretty(&catalog).map_err(|error| {
            issue(
                format!("Failed to serialize catalog: {error}"),
                "json-error",
            )
        })?;
        ctx.fs
            .write(&output.to_string_lossy(), json)
            .map_err(|error| issue(format!("Failed to write catalog: {error}"), "io-error"))?;
        let entry_count = catalog.symbols.len() + catalog.keywords.len();
        ctx.state
            .set_output("output_file", &output.to_string_lossy());
        ctx.state
            .set_output("symbol_count", &entry_count.to_string());
        let _ = page_count;
        let _ = entry_count;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_compiles_without_web_site_settings() {
        let temp = tempfile::tempdir().unwrap();
        let docs = temp.path().join("docs");
        std::fs::create_dir_all(&docs).unwrap();
        std::fs::write(
            docs.join("echo.rst"),
            "echo\n====\n\n:synopsis: echo [args...]\n:summary: Print text\n:category: flow\n:handler: builtins::echo\n\nPrints strings.\n",
        )
        .unwrap();
        let config = CatalogConfig {
            source_dir: "docs".into(),
            out: "catalog.json".into(),
            include: vec![],
            exclude: vec![],
            extract_frontmatter: true,
            reference_prefix: "REF".into(),
        };

        let (catalog, pages) = compile(temp.path(), &config).unwrap();
        assert_eq!(pages, 1);
        assert_eq!(catalog.symbols["echo"].synopsis, "echo [args...]");
        assert_eq!(catalog.symbols["echo"].binding, "builtins::echo");
        assert!(
            catalog.symbols["echo"]
                .hover_markdown
                .contains("Prints strings.")
        );
    }

    #[test]
    fn tracked_esh_catalog_is_reproducible_from_its_reference() {
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let workspace = manifest_dir.parent().unwrap().parent().unwrap();
        let config = CatalogConfig {
            source_dir: "docs/reference/esh".into(),
            out: "unused.json".into(),
            include: vec!["100.runtime-reference.3010.md".into()],
            exclude: vec![],
            extract_frontmatter: true,
            reference_prefix: "REF".into(),
        };
        let (generated, _) = compile(workspace, &config).unwrap();
        let checked_in = evin_doc::CatalogExport::from_json(
            &std::fs::read_to_string(workspace.join("crates/esh/assets/builtins-catalog.json"))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(generated.symbols, checked_in.symbols);
        assert_eq!(generated.keywords, checked_in.keywords);
    }

    #[test]
    fn flow_configuration_references_compile_to_a_catalog_without_site_url() {
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let workspace = manifest_dir.parent().unwrap().parent().unwrap();
        let config = CatalogConfig {
            source_dir: "docs/reference".into(),
            out: "unused.json".into(),
            include: vec![
                "20.workspace-configuration.4500.md".into(),
                "30.module-configuration.4510.md".into(),
                "35.namespace-configuration.4515.md".into(),
                "40.environment-configuration.4520.md".into(),
                "50.git-rules-configuration.4530.md".into(),
            ],
            exclude: vec![],
            extract_frontmatter: true,
            reference_prefix: "REF".into(),
        };
        let (catalog, _) = compile(workspace, &config).unwrap();
        let tasks = &catalog.symbols["workspace.tasks"];
        assert!(tasks.binding.is_empty());
        assert!(tasks.hover_markdown.starts_with("```flow\ntasks:"));
        assert!(
            tasks
                .hover_markdown
                .contains("**Task definitions** (`workspace.tasks`):")
        );
        assert!(catalog.symbols.contains_key("GitRules.releases"));
    }

    #[test]
    fn configuration_is_closed_and_has_no_site_url_requirement() {
        let valid = evin_flow::parse_yaml("source_dir: docs\nout: dist/catalog.json\n").unwrap();
        evin_action_sdk::parse_config::<CatalogConfig>(Some(&valid)).unwrap();

        let invalid =
            evin_flow::parse_yaml("source_dir: docs\nsite_url: https://example.com\n").unwrap();
        assert!(evin_action_sdk::parse_config::<CatalogConfig>(Some(&invalid)).is_err());
    }

    #[tokio::test]
    async fn action_writes_catalog_and_publishes_outputs() {
        let temp = tempfile::tempdir().unwrap();
        let docs = temp.path().join("docs");
        std::fs::create_dir_all(&docs).unwrap();
        std::fs::write(
            docs.join("echo.rst"),
            "echo\n====\n\n:synopsis: echo\n:summary: Print text\n:category: flow\n:handler: builtins::echo\n\nPrints strings.\n",
        )
        .unwrap();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let secrets = std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default());
        let log = evin_action_sdk::Logger::new("catalog".into(), tx.clone(), secrets.clone());
        let git = std::sync::Arc::new(evin_action_sdk::git::GitProvider::new(
            temp.path().to_path_buf(),
            evin_action_sdk::PipelineMode::Normal,
            log,
        ));
        let ctx = evin_action_sdk::RunnerContextBuilder::new(
            "catalog".into(),
            tx,
            temp.path().to_path_buf(),
            git,
            secrets,
        )
        .build();
        let config = evin_flow::parse_yaml("source_dir: docs\nout: dist/catalog.json\n").unwrap();

        CatalogAction.execute(&ctx, Some(config)).await.unwrap();

        let output = temp.path().join("dist/catalog.json");
        let catalog =
            evin_doc::CatalogExport::from_json(&std::fs::read_to_string(&output).unwrap()).unwrap();
        assert!(catalog.symbols.contains_key("echo"));
        assert_eq!(
            ctx.state.outputs.lock().unwrap().get("symbol_count"),
            Some(&"1".to_string())
        );
        assert_eq!(
            ctx.state.outputs.lock().unwrap().get("output_file"),
            Some(&output.to_string_lossy().to_string())
        );
    }
}
