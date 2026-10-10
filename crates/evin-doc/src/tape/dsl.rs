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

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const DEFAULT_ROWS: u16 = 24;
pub const DEFAULT_COLS: u16 = 80;
pub const DEFAULT_SPEED_MS: u32 = 80;
pub const DEFAULT_TIMEOUT_MS: u64 = 30_000;
pub const DEFAULT_MAX_OUTPUT_BYTES: usize = 1_048_576;
pub const DEFAULT_MAX_FRAMES: usize = 2_000;
pub const DEFAULT_COMMAND_SETTLE_MS: u32 = 150;
pub const DEFAULT_PATH_ROOT: &str = "/workspace";

/// Selects the execution host for a terminal tape. ESH is deliberately the
/// default so a recording's shell language remains portable and inspectable.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TapeRuntime {
    #[default]
    Esh,
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TapeConfig {
    pub title: Option<String>,
    pub runtime: TapeRuntime,
    /// Stable display alias for paths beneath the real workspace. This does
    /// not affect the command's current directory or filesystem access.
    pub path_root: String,
    /// When enabled by `rows=auto`, keep the bounded recording screen but
    /// trim the rendered SVG to its occupied visual rows.
    #[serde(default)]
    pub auto_rows: bool,
    pub rows: u16,
    pub cols: u16,
    pub speed_ms: u32,
    pub timeout_ms: u64,
    pub max_output_bytes: usize,
    pub max_frames: usize,
    pub command_settle_ms: u32,
}

impl Default for TapeConfig {
    fn default() -> Self {
        Self {
            title: None,
            runtime: TapeRuntime::Esh,
            path_root: DEFAULT_PATH_ROOT.to_string(),
            auto_rows: false,
            rows: DEFAULT_ROWS,
            cols: DEFAULT_COLS,
            speed_ms: DEFAULT_SPEED_MS,
            timeout_ms: DEFAULT_TIMEOUT_MS,
            max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
            max_frames: DEFAULT_MAX_FRAMES,
            command_settle_ms: DEFAULT_COMMAND_SETTLE_MS,
        }
    }
}

impl TapeConfig {
    pub fn from_attributes(attributes: &HashMap<String, String>) -> Result<Self, String> {
        let attributes = attributes
            .iter()
            .map(|(key, value)| (key.replace('_', "-"), value))
            .collect::<HashMap<_, _>>();
        for key in attributes.keys() {
            if !matches!(
                key.as_str(),
                "title"
                    | "runtime"
                    | "path-root"
                    | "rows"
                    | "cols"
                    | "speed"
                    | "timeout"
                    | "max-output"
                    | "max-frames"
            ) {
                return Err(format!("unknown tape attribute '{key}'"));
            }
        }
        let mut config = Self {
            title: attributes.get("title").cloned().cloned(),
            ..Self::default()
        };
        if let Some(value) = attributes.get("runtime") {
            config.runtime = match value.as_str() {
                "esh" => TapeRuntime::Esh,
                "system" => TapeRuntime::System,
                _ => return Err("tape runtime must be 'esh' or 'system'".to_string()),
            };
        }
        if let Some(value) = attributes.get("path-root") {
            config.path_root = parse_path_root(value)?;
        }
        if let Some(value) = attributes.get("rows") {
            if value.eq_ignore_ascii_case("auto") {
                config.auto_rows = true;
            } else {
                config.rows = parse_nonzero(value, "rows")?;
            }
        }
        if let Some(value) = attributes.get("cols") {
            config.cols = parse_nonzero(value, "cols")?;
        }
        if let Some(value) = attributes.get("speed") {
            config.speed_ms = parse_duration(value)? as u32;
        }
        if let Some(value) = attributes.get("timeout") {
            config.timeout_ms = parse_duration(value)?;
        }
        if let Some(value) = attributes.get("max-output") {
            config.max_output_bytes = parse_bytes(value)?;
        }
        if let Some(value) = attributes.get("max-frames") {
            config.max_frames = parse_positive_usize(value, "max-frames")?;
        }
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.rows == 0 || self.cols == 0 {
            return Err("tape rows and cols must be greater than zero".to_string());
        }
        if self.timeout_ms == 0 || self.max_output_bytes == 0 || self.max_frames == 0 {
            return Err(
                "tape timeout, max-output, and max-frames must be greater than zero".to_string(),
            );
        }
        parse_path_root(&self.path_root)?;
        Ok(())
    }
}

fn parse_path_root(value: &str) -> Result<String, String> {
    let value = value.trim();
    if !value.starts_with('/') || value.chars().any(char::is_control) {
        return Err("tape path-root must be a non-empty slash-rooted path".to_string());
    }
    let value = value.trim_end_matches('/');
    Ok(if value.is_empty() {
        "/".to_string()
    } else {
        value.to_string()
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TapeScript {
    pub commands: Vec<TapeCommand>,
}

impl TapeScript {
    pub fn parse(script: &str) -> Result<Self, String> {
        let mut commands = Vec::new();
        let mut setup_ended = false;
        for (index, raw_line) in script.lines().enumerate() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (name, argument) = line
                .split_once(char::is_whitespace)
                .map(|(name, arg)| (name, arg.trim()))
                .unwrap_or((line, ""));
            let command = match name {
                "setup" => {
                    if setup_ended {
                        return Err(format!(
                            "tape line {}: setup commands must appear before recording commands",
                            index + 1
                        ));
                    }
                    let command = parse_json_string(argument, index + 1)?;
                    if command.trim().is_empty() {
                        return Err(format!(
                            "tape line {}: setup command must not be empty",
                            index + 1
                        ));
                    }
                    TapeCommand::Setup(command)
                }
                "type" => TapeCommand::Type(parse_json_string(argument, index + 1)?),
                "alias" => {
                    let (name, replacement) = argument
                        .split_once(char::is_whitespace)
                        .map(|(name, replacement)| (name, replacement.trim()))
                        .ok_or_else(|| {
                            format!("tape line {}: alias requires two JSON strings", index + 1)
                        })?;
                    let name = parse_json_string(name, index + 1)?;
                    let replacement = parse_json_string(replacement, index + 1)?;
                    if name.is_empty()
                        || name.chars().any(char::is_whitespace)
                        || replacement.is_empty()
                    {
                        return Err(format!(
                            "tape line {}: alias name and replacement must be non-empty; name must be one command word",
                            index + 1
                        ));
                    }
                    TapeCommand::Alias { name, replacement }
                }
                "sleep" => TapeCommand::Sleep(parse_duration(argument)? as u32),
                "enter" if argument.is_empty() => TapeCommand::Enter,
                "wait" if argument.is_empty() => TapeCommand::Wait,
                "expect" => TapeCommand::Expect(parse_json_string(argument, index + 1)?),
                "backspace" => TapeCommand::Backspace(if argument.is_empty() {
                    1
                } else {
                    parse_nonzero(argument, "backspace")? as usize
                }),
                "ctrl" => {
                    let mut chars = argument.chars();
                    let Some(letter) = chars.next() else {
                        return Err(format!(
                            "tape line {}: ctrl requires one ASCII letter",
                            index + 1
                        ));
                    };
                    if chars.next().is_some() || !letter.is_ascii_alphabetic() {
                        return Err(format!(
                            "tape line {}: ctrl requires one ASCII letter",
                            index + 1
                        ));
                    }
                    TapeCommand::Ctrl(letter.to_ascii_uppercase())
                }
                _ => {
                    return Err(format!(
                        "tape line {}: unknown or malformed command '{line}'",
                        index + 1
                    ));
                }
            };
            if !matches!(command, TapeCommand::Setup(_)) {
                setup_ended = true;
            }
            commands.push(command);
        }
        Ok(Self { commands })
    }

    /// Rebuilds the completed command lines for the tape's copy action.
    /// Recording controls intentionally do not appear in the copied script.
    pub fn to_shell_script(&self) -> String {
        let mut script = String::new();
        let mut input = String::new();
        let mut interrupted = false;
        let mut aliases = Vec::new();
        for command in &self.commands {
            match command {
                TapeCommand::Alias { name, replacement } => {
                    aliases.retain(|(existing, _): &(String, String)| existing != name);
                    aliases.push((name.clone(), replacement.clone()));
                }
                TapeCommand::Type(value) => {
                    input.push_str(value);
                    interrupted = false;
                }
                TapeCommand::Backspace(count) => {
                    for _ in 0..*count {
                        input.pop();
                    }
                    interrupted = false;
                }
                TapeCommand::Ctrl('C') => {
                    input.clear();
                    interrupted = true;
                }
                TapeCommand::Enter => {
                    if !interrupted {
                        script.push_str(&expand_alias(&input, &aliases));
                        script.push('\n');
                    }
                    input.clear();
                    interrupted = false;
                }
                TapeCommand::Sleep(_)
                | TapeCommand::Setup(_)
                | TapeCommand::Wait
                | TapeCommand::Expect(_)
                | TapeCommand::Ctrl(_) => {}
            }
        }
        script
    }
}

/// Expand only an exact first command word, preserving the user's displayed
/// input while producing a directly runnable command for execution and copy.
pub fn expand_alias(input: &str, aliases: &[(String, String)]) -> String {
    if input.is_empty() {
        return input.to_string();
    }
    let end = input.find(char::is_whitespace).unwrap_or(input.len());
    let command = &input[..end];
    let Some((_, replacement)) = aliases.iter().rev().find(|(name, _)| name == command) else {
        return input.to_string();
    };
    format!("{replacement}{}", &input[end..])
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TapeCommand {
    Setup(String),
    Type(String),
    Alias { name: String, replacement: String },
    Sleep(u32),
    Enter,
    Wait,
    Expect(String),
    Backspace(usize),
    Ctrl(char),
}

pub fn parse_tape_fence_info(info: &str) -> Result<TapeConfig, String> {
    let values = split_attributes(info)?;
    if values.first().map(String::as_str) != Some("tape") {
        return Err("tape fence must begin with 'tape'".to_string());
    }
    let mut attributes = HashMap::new();
    for value in values.into_iter().skip(1) {
        let Some((key, value)) = value.split_once('=') else {
            return Err(format!("invalid tape attribute '{value}'"));
        };
        attributes.insert(key.to_string(), value.to_string());
    }
    TapeConfig::from_attributes(&attributes)
}

fn split_attributes(input: &str) -> Result<Vec<String>, String> {
    let mut values = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    let mut escaped = false;
    for character in input.chars() {
        if escaped {
            current.push(character);
            escaped = false;
            continue;
        }
        if character == '\\' && quote.is_some() {
            current.push(character);
            escaped = true;
            continue;
        }
        if matches!(character, '\'' | '"') {
            if quote == Some(character) {
                quote = None;
            } else if quote.is_none() {
                quote = Some(character);
            }
            current.push(character);
            continue;
        }
        if character.is_whitespace() && quote.is_none() {
            if !current.is_empty() {
                values.push(unquote(&current)?);
                current.clear();
            }
        } else {
            current.push(character);
        }
    }
    if quote.is_some() {
        return Err("unterminated tape attribute quote".to_string());
    }
    if !current.is_empty() {
        values.push(unquote(&current)?);
    }
    Ok(values)
}

fn unquote(value: &str) -> Result<String, String> {
    if let Some((key, raw)) = value.split_once('=') {
        if raw.starts_with('"') {
            return Ok(format!("{key}={}", parse_json_string(raw, 0)?));
        }
        if raw.starts_with('\'') && raw.ends_with('\'') {
            return Ok(format!("{key}={}", &raw[1..raw.len() - 1]));
        }
    }
    Ok(value.to_string())
}

fn parse_json_string(value: &str, line: usize) -> Result<String, String> {
    serde_json::from_str(value).map_err(|_| {
        if line == 0 {
            "tape attribute values with spaces must use JSON double quotes".to_string()
        } else {
            format!("tape line {line}: expected a JSON string")
        }
    })
}

pub fn parse_duration(value: &str) -> Result<u64, String> {
    let value = value.trim();
    if let Some(milliseconds) = value.strip_suffix("ms") {
        return milliseconds
            .parse()
            .map_err(|_| format!("invalid tape duration '{value}'"));
    }
    if let Some(seconds) = value.strip_suffix('s') {
        return seconds
            .parse::<u64>()
            .map(|seconds| seconds.saturating_mul(1_000))
            .map_err(|_| format!("invalid tape duration '{value}'"));
    }
    Err(format!("tape duration '{value}' must use ms or s"))
}

fn parse_bytes(value: &str) -> Result<usize, String> {
    let value = value.trim();
    if let Some(mebibytes) = value.strip_suffix("MiB") {
        return mebibytes
            .parse::<usize>()
            .map(|value| value.saturating_mul(1024 * 1024))
            .map_err(|_| format!("invalid tape max-output '{value}'"));
    }
    value
        .parse()
        .map_err(|_| format!("invalid tape max-output '{value}'"))
}

fn parse_nonzero(value: &str, field: &str) -> Result<u16, String> {
    value
        .parse::<u16>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| format!("tape {field} must be a positive integer"))
}

fn parse_positive_usize(value: &str, field: &str) -> Result<usize, String> {
    value
        .parse::<usize>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| format!("tape {field} must be a positive integer"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fence_attributes_and_control_commands() {
        let config =
            parse_tape_fence_info("tape title=\"Build docs\" rows=14 cols=70 speed=40ms").unwrap();
        assert_eq!(config.title.as_deref(), Some("Build docs"));
        assert_eq!((config.rows, config.cols, config.speed_ms), (14, 70, 40));
        assert_eq!(config.runtime, TapeRuntime::Esh);
        assert_eq!(config.path_root, DEFAULT_PATH_ROOT);
        let script = TapeScript::parse("type \"echo hi\"\nenter\nwait\nctrl c").unwrap();
        assert_eq!(script.commands.len(), 4);
    }

    #[test]
    fn defaults_to_a_readable_typing_cadence() {
        assert_eq!(TapeConfig::default().speed_ms, 80);
    }

    #[test]
    fn rows_auto_preserves_a_bounded_capture_screen() {
        let config = parse_tape_fence_info("tape rows=auto cols=70").unwrap();

        assert!(config.auto_rows);
        assert_eq!(config.rows, DEFAULT_ROWS);
        assert_eq!(config.cols, 70);
    }

    #[test]
    fn rejects_invalid_commands_and_limits() {
        assert!(TapeScript::parse("type not-json").is_err());
        assert!(TapeScript::parse("ctrl cc").is_err());
        assert!(parse_tape_fence_info("tape rows=0").is_err());
        assert!(parse_tape_fence_info("tape max-frames=0").is_err());
        assert!(parse_tape_fence_info("tape runtime=unknown").is_err());
        assert!(parse_tape_fence_info("tape path-root=workspace").is_err());
        assert!(parse_tape_fence_info("tape unknown=value").is_err());
    }

    #[test]
    fn parses_explicit_system_runtime() {
        let config = parse_tape_fence_info("tape runtime=system path_root=/docs/").unwrap();
        assert_eq!(config.runtime, TapeRuntime::System);
        assert_eq!(config.path_root, "/docs");
    }

    #[test]
    fn reconstructs_completed_commands_for_copying() {
        let script = TapeScript::parse(
            "type \"echo hix\"\nbackspace\nenter\nsleep 1s\nwait\ntype \"ignored\"\nctrl c\nenter\ntype \"printf λ\"\nenter\nexpect \"λ\"",
        )
        .unwrap();
        assert_eq!(script.to_shell_script(), "echo hi\nprintf λ\n");
    }
}
