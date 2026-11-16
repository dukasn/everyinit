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

use super::super::CatalogAction;
use evin_action_sdk::Action;

fn context(root: &std::path::Path) -> evin_action_sdk::RunnerContext {
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let secrets = std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::default());
    let log = evin_action_sdk::Logger::new("catalog".into(), tx.clone(), secrets.clone());
    let git = std::sync::Arc::new(evin_action_sdk::git::GitProvider::new(
        root.to_path_buf(),
        evin_action_sdk::PipelineMode::Normal,
        log,
    ));
    evin_action_sdk::RunnerContextBuilder::new(
        "catalog".into(),
        tx,
        root.to_path_buf(),
        git,
        secrets,
    )
    .build()
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
    let ctx = context(temp.path());
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

#[tokio::test]
async fn action_configuration_error_uses_fluent_message() {
    let temp = tempfile::tempdir().unwrap();
    let ctx = context(temp.path());
    let config = evin_flow::parse_yaml("unknown: true\n").unwrap();
    let config_error =
        evin_action_sdk::parse_config::<super::super::config::CatalogConfig>(Some(&config))
            .unwrap_err();

    let error = CatalogAction.execute(&ctx, Some(config)).await.unwrap_err();
    let expected = evin_locales::t!("catalog-invalid-config", error = config_error.message);
    assert_eq!(error.code.key, "catalog-invalid-config");
    assert_eq!(evin_locales::render_issue(&error.code), expected);
    assert_eq!(
        error.title,
        evin_locales::t!("catalog-invalid-config-title")
    );
}

#[tokio::test]
async fn action_output_write_error_uses_fluent_message() {
    let temp = tempfile::tempdir().unwrap();
    let docs = temp.path().join("docs");
    std::fs::create_dir_all(&docs).unwrap();
    std::fs::write(
        docs.join("echo.rst"),
        "echo\n====\n\n:synopsis: echo\n:summary: Print text\n:category: flow\n:handler: builtins::echo\n\nPrints strings.\n",
    )
    .unwrap();
    std::fs::write(temp.path().join("blocked"), "file").unwrap();
    let ctx = context(temp.path());
    let config = evin_flow::parse_yaml("source_dir: docs\nout: blocked/catalog.json\n").unwrap();

    let error = CatalogAction.execute(&ctx, Some(config)).await.unwrap_err();
    assert_eq!(error.code.key, "catalog-output-write-failed");
    let detail = &error.code.args[0].1;
    assert_eq!(
        evin_locales::render_issue(&error.code),
        evin_locales::t!("catalog-output-write-failed", error = detail)
    );
    assert_eq!(
        error.title,
        evin_locales::t!("catalog-output-write-failed-title")
    );
}
