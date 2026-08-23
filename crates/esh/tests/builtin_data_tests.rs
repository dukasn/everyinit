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

use esh::{Engine, ExecutionContext};
use tempfile::tempdir;

#[tokio::test]
async fn test_json_get_and_valid() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        json -g version '{"name": "esh", "version": "1.0.0", "details": {"active": true}}'
        json -g details.active '{"name": "esh", "details": {"active": true}}'
        json -g items[1] '{"items": ["a", "b", "c"]}'
        json -v '{"valid": true}' && echo "json is valid"
    "#;

    let status = engine
        .eval_str(script, &mut ctx)
        .await
        .expect("eval failed");
    assert_eq!(status, 0);

    let output = ctx.get_captured_stdout().unwrap();
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines[0], "1.0.0");
    assert_eq!(lines[1], "true");
    assert_eq!(lines[2], "b");
    assert!(output.contains("json is valid"));
}

#[tokio::test]
async fn test_yaml_get_and_to_json() {
    let tmp = tempdir().unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(tmp.path()).unwrap();
    ctx.enable_capture();

    let yaml_content = r#"
app:
  name: evin
  port: 8080
"#;
    std::fs::write(tmp.path().join("config.yaml"), yaml_content).unwrap();

    let script = r#"
        yaml -g app.name config.yaml
        yaml -g app.port config.yaml
        yaml -v config.yaml && echo "yaml is valid"
        yaml -j config.yaml
    "#;

    let status = engine
        .eval_str(script, &mut ctx)
        .await
        .expect("eval failed");
    assert_eq!(status, 0);

    let output = ctx.get_captured_stdout().unwrap();
    assert!(output.contains("evin"));
    assert!(output.contains("8080"));
    assert!(output.contains("yaml is valid"));
    assert!(output.contains("\"name\": \"evin\""));
}
