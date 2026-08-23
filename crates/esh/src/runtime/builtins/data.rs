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

use crate::error::ExecError;
use crate::runtime::context::ExecutionContext;
use crate::runtime::traits::BoxFuture;
use serde_json::Value as JsonValue;
use serde_yaml::Value as YamlValue;
use std::fs;
use std::path::Path;

fn read_source_content(ctx: &mut ExecutionContext, source: Option<&str>) -> Option<String> {
    match source {
        Some("-") | None => ctx
            .take_stdin()
            .map(|b| String::from_utf8_lossy(&b).to_string()),
        Some(src) => {
            let p = Path::new(src);
            let resolved = if p.is_absolute() {
                p.to_path_buf()
            } else {
                ctx.cwd().join(p)
            };

            if resolved.is_file() {
                fs::read_to_string(resolved).ok()
            } else {
                Some(src.to_string())
            }
        }
    }
}

fn query_json<'a>(val: &'a JsonValue, path: &str) -> Option<&'a JsonValue> {
    if path.is_empty() {
        return Some(val);
    }
    let mut current = val;
    for segment in path.split('.') {
        if segment.is_empty() {
            continue;
        }
        if let Some(idx_start) = segment.find('[') {
            let key = &segment[..idx_start];
            if !key.is_empty() {
                current = current.get(key)?;
            }
            let idx_str = segment
                .strip_prefix(&format!("{}[", key))?
                .strip_suffix(']')?;
            let idx: usize = idx_str.parse().ok()?;
            current = current.get(idx)?;
        } else {
            current = current.get(segment)?;
        }
    }
    Some(current)
}

fn format_json_val(val: &JsonValue) -> String {
    match val {
        JsonValue::String(s) => s.clone(),
        JsonValue::Number(n) => n.to_string(),
        JsonValue::Bool(b) => b.to_string(),
        JsonValue::Null => "null".to_string(),
        other => serde_json::to_string_pretty(other).unwrap_or_default(),
    }
}

pub fn json<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        if args.is_empty() {
            ctx.write_stderr(b"esh: json: usage: json [-g|--get key_path [file|-]] [-v|--valid [file|-]] [-k|--keys [file|-]] [-p|--pretty [file|-]]\n");
            return Ok(1);
        }

        let flag = args[0].as_str();
        match flag {
            "-g" | "--get" => {
                if args.len() < 2 {
                    ctx.write_stderr(b"esh: json -g: usage: json -g <key_path> [file|-]\n");
                    return Ok(1);
                }
                let (path_arg, source_arg) = if args.len() >= 3 {
                    (args[1].as_str(), Some(args[2].as_str()))
                } else {
                    (args[1].as_str(), None)
                };

                let content = match read_source_content(ctx, source_arg) {
                    Some(c) => c,
                    None => {
                        ctx.write_stderr(b"esh: json: failed to read source\n");
                        return Ok(1);
                    }
                };

                let parsed: JsonValue = match serde_json::from_str(&content) {
                    Ok(v) => v,
                    Err(e) => {
                        ctx.write_stderr(format!("esh: json: invalid JSON: {}\n", e).as_bytes());
                        return Ok(1);
                    }
                };

                match query_json(&parsed, path_arg) {
                    Some(v) => {
                        ctx.write_stdout(format!("{}\n", format_json_val(v)).as_bytes());
                        Ok(0)
                    }
                    None => {
                        ctx.write_stderr(
                            format!("esh: json: key '{}' not found\n", path_arg).as_bytes(),
                        );
                        Ok(1)
                    }
                }
            }
            "-v" | "--valid" => {
                let source_arg = args.get(1).map(String::as_str);
                let content = match read_source_content(ctx, source_arg) {
                    Some(c) => c,
                    None => return Ok(1),
                };
                if serde_json::from_str::<JsonValue>(&content).is_ok() {
                    Ok(0)
                } else {
                    Ok(1)
                }
            }
            "-k" | "--keys" => {
                let source_arg = args.get(1).map(String::as_str);
                let content = match read_source_content(ctx, source_arg) {
                    Some(c) => c,
                    None => return Ok(1),
                };
                let parsed: JsonValue = match serde_json::from_str(&content) {
                    Ok(v) => v,
                    Err(_) => return Ok(1),
                };
                if let JsonValue::Object(map) = parsed {
                    for key in map.keys() {
                        ctx.write_stdout(format!("{}\n", key).as_bytes());
                    }
                    Ok(0)
                } else {
                    Ok(1)
                }
            }
            "-p" | "--pretty" => {
                let source_arg = args.get(1).map(String::as_str);
                let content = match read_source_content(ctx, source_arg) {
                    Some(c) => c,
                    None => return Ok(1),
                };
                let parsed: JsonValue = match serde_json::from_str(&content) {
                    Ok(v) => v,
                    Err(e) => {
                        ctx.write_stderr(format!("esh: json: invalid JSON: {}\n", e).as_bytes());
                        return Ok(1);
                    }
                };
                let formatted = serde_json::to_string_pretty(&parsed).unwrap_or_default();
                ctx.write_stdout(format!("{}\n", formatted).as_bytes());
                Ok(0)
            }
            other => {
                ctx.write_stderr(
                    format!("esh: json: unrecognized option '{}'\n", other).as_bytes(),
                );
                Ok(1)
            }
        }
    })
}

fn query_yaml<'a>(val: &'a YamlValue, path: &str) -> Option<&'a YamlValue> {
    if path.is_empty() {
        return Some(val);
    }
    let mut current = val;
    for segment in path.split('.') {
        if segment.is_empty() {
            continue;
        }
        match current {
            YamlValue::Mapping(map) => {
                let key = YamlValue::String(segment.to_string());
                current = map.get(&key)?;
            }
            YamlValue::Sequence(seq) => {
                let idx: usize = segment.parse().ok()?;
                current = seq.get(idx)?;
            }
            _ => return None,
        }
    }
    Some(current)
}

fn format_yaml_val(val: &YamlValue) -> String {
    match val {
        YamlValue::String(s) => s.clone(),
        YamlValue::Number(n) => n.to_string(),
        YamlValue::Bool(b) => b.to_string(),
        YamlValue::Null => "null".to_string(),
        other => serde_yaml::to_string(other)
            .unwrap_or_default()
            .trim()
            .to_string(),
    }
}

pub fn yaml<'a>(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>> {
    Box::pin(async move {
        if args.is_empty() {
            ctx.write_stderr(b"esh: yaml: usage: yaml [-g|--get key_path [file|-]] [-j|--to-json [file|-]] [-v|--valid [file|-]] [-k|--keys [file|-]]\n");
            return Ok(1);
        }

        let flag = args[0].as_str();
        match flag {
            "-g" | "--get" => {
                if args.len() < 2 {
                    ctx.write_stderr(b"esh: yaml -g: usage: yaml -g <key_path> [file|-]\n");
                    return Ok(1);
                }
                let (path_arg, source_arg) = if args.len() >= 3 {
                    (args[1].as_str(), Some(args[2].as_str()))
                } else {
                    (args[1].as_str(), None)
                };

                let content = match read_source_content(ctx, source_arg) {
                    Some(c) => c,
                    None => {
                        ctx.write_stderr(b"esh: yaml: failed to read source\n");
                        return Ok(1);
                    }
                };

                let parsed: YamlValue = match serde_yaml::from_str(&content) {
                    Ok(v) => v,
                    Err(e) => {
                        ctx.write_stderr(format!("esh: yaml: invalid YAML: {}\n", e).as_bytes());
                        return Ok(1);
                    }
                };

                match query_yaml(&parsed, path_arg) {
                    Some(v) => {
                        ctx.write_stdout(format!("{}\n", format_yaml_val(v)).as_bytes());
                        Ok(0)
                    }
                    None => {
                        ctx.write_stderr(
                            format!("esh: yaml: key '{}' not found\n", path_arg).as_bytes(),
                        );
                        Ok(1)
                    }
                }
            }
            "-j" | "--to-json" => {
                let source_arg = args.get(1).map(String::as_str);
                let content = match read_source_content(ctx, source_arg) {
                    Some(c) => c,
                    None => return Ok(1),
                };
                let parsed: YamlValue = match serde_yaml::from_str(&content) {
                    Ok(v) => v,
                    Err(e) => {
                        ctx.write_stderr(format!("esh: yaml: invalid YAML: {}\n", e).as_bytes());
                        return Ok(1);
                    }
                };
                let json_val: JsonValue =
                    serde_json::to_value(parsed).map_err(|e| ExecError::Custom(e.to_string()))?;
                let json_str = serde_json::to_string_pretty(&json_val)
                    .map_err(|e| ExecError::Custom(e.to_string()))?;
                ctx.write_stdout(format!("{}\n", json_str).as_bytes());
                Ok(0)
            }
            "-v" | "--valid" => {
                let source_arg = args.get(1).map(String::as_str);
                let content = match read_source_content(ctx, source_arg) {
                    Some(c) => c,
                    None => return Ok(1),
                };
                if serde_yaml::from_str::<YamlValue>(&content).is_ok() {
                    Ok(0)
                } else {
                    Ok(1)
                }
            }
            "-k" | "--keys" => {
                let source_arg = args.get(1).map(String::as_str);
                let content = match read_source_content(ctx, source_arg) {
                    Some(c) => c,
                    None => return Ok(1),
                };
                let parsed: YamlValue = match serde_yaml::from_str(&content) {
                    Ok(v) => v,
                    Err(_) => return Ok(1),
                };
                if let YamlValue::Mapping(map) = parsed {
                    for key in map.keys() {
                        if let Some(s) = key.as_str() {
                            ctx.write_stdout(format!("{}\n", s).as_bytes());
                        }
                    }
                    Ok(0)
                } else {
                    Ok(1)
                }
            }
            other => {
                ctx.write_stderr(
                    format!("esh: yaml: unrecognized option '{}'\n", other).as_bytes(),
                );
                Ok(1)
            }
        }
    })
}
