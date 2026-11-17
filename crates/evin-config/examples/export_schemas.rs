// Copyright (C) 2026 Eduardo Marques Santos
//
// This program is free software; you can redistribute it and/or modify
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

use evin_config::environment::EnvironmentSchema;
use evin_repository_policy::RepositoryPolicy;
use schemars::JsonSchema;
use serde_json::{Value, json};
use std::env;
use std::fs;
use std::path::PathBuf;

fn schema_for<T: JsonSchema>(id: &str, marker: &str) -> Value {
    let mut schema = serde_json::to_value(schemars::schema_for!(T)).expect("serialize schema");
    schema["$schema"] = json!("https://json-schema.org/draft/2020-12/schema");
    schema["$id"] = json!(id);
    schema["properties"]["$schema"] = json!({"const": marker, "type": "string"});
    if id.ends_with("/environment") {
        schema["$defs"]["EnvVarDef"]["properties"]["default"] = json!({
            "oneOf": [
                {"type": "string"},
                {"type": "number"},
                {"type": "boolean"},
                {"type": "null"}
            ]
        });
    }
    if !schema["required"].is_array() {
        schema["required"] = json!([]);
    }
    let required = schema["required"].as_array_mut().expect("required list");
    if !required.iter().any(|value| value == "$schema") {
        required.push(json!("$schema"));
    }
    schema
}

fn main() {
    let check = env::args().any(|arg| arg == "--check");
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let schemas = [
        (
            "policy",
            schema_for::<RepositoryPolicy>(
                "https://dl.everyinit.dev/schema/v1/policy",
                "https://dl.everyinit.dev/schema/v1/policy",
            ),
        ),
        (
            "environment",
            schema_for::<EnvironmentSchema>(
                "https://dl.everyinit.dev/schema/v1/environment",
                "https://dl.everyinit.dev/schema/v1/environment",
            ),
        ),
    ];
    for (name, schema) in schemas {
        let path = root.join(format!("schemas/v1/{name}.schema.json"));
        let generated = serde_json::to_string_pretty(&schema).unwrap() + "\n";
        if check {
            let checked_in = fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
            assert_eq!(
                checked_in,
                generated,
                "{} is stale; regenerate schemas",
                path.display()
            );
        } else {
            fs::create_dir_all(path.parent().unwrap()).expect("create schema directory");
            fs::write(&path, generated).expect("write schema");
        }
    }
}
