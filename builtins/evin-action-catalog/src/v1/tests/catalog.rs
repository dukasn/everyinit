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

use super::super::compiler::compile;
use super::super::config::CatalogConfig;

#[test]
fn reference_prefix_error_uses_fluent_message() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join("docs")).unwrap();
    let config = CatalogConfig {
        source_dir: "docs".into(),
        out: "dist/catalog.json".into(),
        include: Vec::new(),
        exclude: Vec::new(),
        extract_frontmatter: true,
        reference_prefix: " ".into(),
    };
    let error = compile(temp.path(), &config).unwrap_err();
    let expected = evin_locales::t!("catalog-reference-prefix-invalid");
    assert!(
        !expected.starts_with("<missing locale key:"),
        "expected Fluent message to exist: {expected}"
    );
    assert_eq!(error.code.key, "catalog-reference-prefix-invalid");
    assert_eq!(evin_locales::render_issue(&error.code), expected);
    assert_eq!(
        error.title,
        evin_locales::t!("catalog-reference-prefix-invalid-title")
    );
}

#[test]
fn source_navigation_order_error_uses_fluent_message() {
    let temp = tempfile::tempdir().unwrap();
    let docs = temp.path().join("docs");
    std::fs::create_dir_all(&docs).unwrap();
    std::fs::write(docs.join("999999999999999999999999.guide.md"), "Guide\n").unwrap();
    let config = CatalogConfig {
        source_dir: "docs".into(),
        out: "dist/catalog.json".into(),
        include: Vec::new(),
        exclude: Vec::new(),
        extract_frontmatter: true,
        reference_prefix: "REF".into(),
    };

    let error = compile(temp.path(), &config).unwrap_err();
    let expected = evin_locales::t!(
        "catalog-source-navigation-order-invalid",
        path = "999999999999999999999999.guide.md"
    );
    assert_eq!(error.code.key, "catalog-source-navigation-order-invalid");
    assert_eq!(evin_locales::render_issue(&error.code), expected);
    assert_eq!(
        error.title,
        evin_locales::t!("catalog-source-navigation-order-invalid-title")
    );
}

#[test]
fn formal_reference_error_uses_fluent_message() {
    let temp = tempfile::tempdir().unwrap();
    let docs = temp.path().join("docs");
    std::fs::create_dir_all(&docs).unwrap();
    std::fs::write(
        docs.join("first-1000.md"),
        "---\ndepends_on: REF-2000\n---\n# First\n",
    )
    .unwrap();
    std::fs::write(
        docs.join("second-2000.md"),
        "---\ndepends_on: REF-1000\n---\n# Second\n",
    )
    .unwrap();
    let config = CatalogConfig {
        source_dir: "docs".into(),
        out: "dist/catalog.json".into(),
        include: Vec::new(),
        exclude: Vec::new(),
        extract_frontmatter: true,
        reference_prefix: "REF".into(),
    };

    let error = compile(temp.path(), &config).unwrap_err();
    assert_eq!(error.code.key, "catalog-source-formal-reference-invalid");
    let detail = &error.code.args[0].1;
    assert_eq!(
        evin_locales::render_issue(&error.code),
        evin_locales::t!("catalog-source-formal-reference-invalid", error = detail)
    );
    assert_eq!(
        error.title,
        evin_locales::t!("catalog-source-formal-reference-invalid-title")
    );
}

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
