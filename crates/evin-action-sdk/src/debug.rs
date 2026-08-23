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

//! Typed, locale-independent structural debug records.

mod private {
    pub trait StructuredDebugEvent {}
}

#[derive(Debug, Clone, PartialEq)]
pub enum DebugValue {
    String(String),
    Bool(bool),
    Signed(i64),
    Unsigned(u64),
    List(Vec<String>),
    Null,
}

macro_rules! impl_unsigned {
    ($($ty:ty),*) => {$(impl From<$ty> for DebugValue { fn from(value: $ty) -> Self { Self::Unsigned(value as u64) } })*};
}
macro_rules! impl_signed {
    ($($ty:ty),*) => {$(impl From<$ty> for DebugValue { fn from(value: $ty) -> Self { Self::Signed(value as i64) } })*};
}
impl_unsigned!(u8, u16, u32, u64, usize);
impl_signed!(i8, i16, i32, i64, isize);
impl From<u128> for DebugValue {
    fn from(value: u128) -> Self {
        Self::Unsigned(value.min(u64::MAX as u128) as u64)
    }
}
impl From<bool> for DebugValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}
impl From<String> for DebugValue {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}
impl From<&str> for DebugValue {
    fn from(value: &str) -> Self {
        Self::String(value.to_string())
    }
}
impl From<Vec<String>> for DebugValue {
    fn from(value: Vec<String>) -> Self {
        Self::List(value)
    }
}
impl<T: Into<DebugValue>> From<Option<T>> for DebugValue {
    fn from(value: Option<T>) -> Self {
        value.map(Into::into).unwrap_or(Self::Null)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DebugAggregation {
    Append,
    Snapshot { key: &'static str },
}

#[derive(Debug, Clone, PartialEq)]
pub struct DebugRecord {
    pub code: &'static str,
    pub payload: Vec<(&'static str, DebugValue)>,
    pub aggregation: DebugAggregation,
}

pub trait StructuredDebugEvent: private::StructuredDebugEvent + Send {
    fn into_record(self) -> DebugRecord;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DebugDescriptor {
    pub code: &'static str,
    pub fields: &'static [&'static str],
    pub snapshot_key: Option<&'static str>,
}

fn safe_string(value: &str) -> bool {
    !value.is_empty()
        && !matches!(value, "true" | "false" | "null")
        && value.parse::<f64>().is_err()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._:/@+%-".contains(c))
}

fn format_value(value: &DebugValue) -> String {
    match value {
        DebugValue::String(value) if safe_string(value) => value.clone(),
        DebugValue::String(value) => serde_json::to_string(value).expect("string serialization"),
        DebugValue::Bool(value) => value.to_string(),
        DebugValue::Signed(value) => value.to_string(),
        DebugValue::Unsigned(value) => value.to_string(),
        DebugValue::List(value) => serde_json::to_string(value).expect("list serialization"),
        DebugValue::Null => "null".to_string(),
    }
}

pub fn format_record(record: &DebugRecord) -> String {
    let mut result = format!("[{}]", record.code);
    for (key, value) in &record.payload {
        result.push(' ');
        result.push_str(key);
        result.push('=');
        result.push_str(&format_value(value));
    }
    result
}

fn parse_json_value(input: &str) -> Option<(serde_json::Value, usize)> {
    let mut stream = serde_json::Deserializer::from_str(input).into_iter::<serde_json::Value>();
    let value = stream.next()?.ok()?;
    Some((value, stream.byte_offset()))
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedDebugRecord {
    pub code: String,
    pub payload: Vec<(String, DebugValue)>,
}

pub fn parse_record(input: &str) -> Result<ParsedDebugRecord, String> {
    let input = input.trim();
    let close = input.find(']').ok_or("missing debug code terminator")?;
    if !input.starts_with("[E_") {
        return Err("invalid debug code".to_string());
    }
    let code = input[1..close].to_string();
    if code.len() != 5
        || !code[2..]
            .chars()
            .all(|character| character.is_ascii_digit())
    {
        return Err("invalid debug code".to_string());
    }
    let mut rest = input[close + 1..].trim_start();
    let mut payload = Vec::new();
    while !rest.is_empty() {
        let equals = rest.find('=').ok_or("missing field separator")?;
        let key = &rest[..equals];
        if key.is_empty()
            || !key
                .chars()
                .all(|c| c == '_' || c.is_ascii_lowercase() || c.is_ascii_digit())
        {
            return Err(format!("invalid debug key '{key}'"));
        }
        rest = &rest[equals + 1..];
        let (value, consumed) = if rest.starts_with('"') || rest.starts_with('[') {
            let (json, consumed) = parse_json_value(rest).ok_or("invalid JSON debug value")?;
            let value = match json {
                serde_json::Value::String(value) => DebugValue::String(value),
                serde_json::Value::Array(values) => DebugValue::List(
                    values
                        .into_iter()
                        .map(|v| {
                            v.as_str()
                                .map(str::to_string)
                                .ok_or("debug lists must contain strings")
                        })
                        .collect::<Result<_, _>>()?,
                ),
                _ => return Err("unsupported JSON debug value".to_string()),
            };
            (value, consumed)
        } else {
            let consumed = rest.find(char::is_whitespace).unwrap_or(rest.len());
            let token = &rest[..consumed];
            let value = match token {
                "true" => DebugValue::Bool(true),
                "false" => DebugValue::Bool(false),
                "null" => DebugValue::Null,
                _ if token.starts_with('-') => token
                    .parse::<i64>()
                    .map(DebugValue::Signed)
                    .unwrap_or_else(|_| DebugValue::String(token.to_string())),
                _ => token
                    .parse::<u64>()
                    .map(DebugValue::Unsigned)
                    .unwrap_or_else(|_| DebugValue::String(token.to_string())),
            };
            (value, consumed)
        };
        payload.push((key.to_string(), value));
        rest = rest[consumed..].trim_start();
    }
    Ok(ParsedDebugRecord { code, payload })
}

macro_rules! aggregation {
    (append) => {
        DebugAggregation::Append
    };
    (snapshot($variant:ident = $key:literal)) => {
        DebugAggregation::Snapshot { key: $key }
    };
}
macro_rules! snapshot_key {
    (append) => {
        None
    };
    (snapshot($variant:ident = $key:literal)) => {
        Some($key)
    };
}
macro_rules! family_range {
    (context) => {
        (100, 199)
    };
    (resolution) => {
        (200, 299)
    };
    (execution) => {
        (300, 399)
    };
    (filesystem) => {
        (400, 499)
    };
    (core) => {
        (501, 501)
    };
    (builtin) => {
        (510, 599)
    };
    (subprocess) => {
        (600, 699)
    };
    (daemon) => {
        (700, 799)
    };
}

macro_rules! define_debug_registry {
    ($(pub struct $name:ident { $($field:ident : $ty:ty),* $(,)? } => ($code:literal, $family:ident, $aggregation:ident $(( $snapshot_variant:ident = $snapshot:literal ))?);)*) => {
        const _: () = {
            let codes = &[$($code),*];
            let ranges = &[$(family_range!($family)),*];
            let mut index = 0;
            while index < codes.len() {
                assert!(codes[index] >= ranges[index].0 && codes[index] <= ranges[index].1, "debug code outside its assigned range");
                let mut other = index + 1;
                while other < codes.len() {
                    assert!(codes[index] != codes[other], "duplicate debug code");
                    other += 1;
                }
                index += 1;
            }
        };
        #[allow(dead_code, clippy::enum_variant_names)]
        enum RegisteredSnapshotKey {
            $($($snapshot_variant,)?) *
        }
        $(
            #[derive(Debug, Clone)]
            pub struct $name { $(pub $field: $ty),* }
            impl private::StructuredDebugEvent for $name {}
            impl StructuredDebugEvent for $name {
                fn into_record(self) -> DebugRecord {
                    DebugRecord {
                        code: concat!("E_", stringify!($code)),
                        payload: vec![$((stringify!($field), DebugValue::from(self.$field))),*],
                        aggregation: aggregation!($aggregation $(($snapshot_variant = $snapshot))?),
                    }
                }
            }
        )*
        pub const ALL_DEBUG_EVENTS: &[DebugDescriptor] = &[$(
            DebugDescriptor {
                code: concat!("E_", stringify!($code)),
                fields: &[$(stringify!($field)),*],
                snapshot_key: snapshot_key!($aggregation $(($snapshot_variant = $snapshot))?),
            }
        ),*];
        pub const ALL_CODES: &[&str] = &[$(concat!("E_", stringify!($code))),*];
    };
}

define_debug_registry! {
    pub struct ConfigLoaded { target: String, name: String, bytes: usize, status: String } => (101, context, append);
    pub struct WorkspaceContext { workspace_root: String, current_cwd: String, config_origin: String, write_permissions: String } => (102, context, append);
    pub struct GitState { git_branch: String, head_commit: String, working_tree: String, hooks_dir: String } => (103, context, append);
    pub struct GitrulesLoaded { target: String, loaded_rules: usize } => (104, context, append);
    pub struct ModuleLoaded { target: String, name: String, tasks: usize } => (105, context, append);
    pub struct EnvironmentResolved { vars: usize, schema: Option<String> } => (106, context, append);
    pub struct LocaleDiagnostics { locale_env: String, resolved_lang: String, loaded_resources: usize, fallback_occurred: bool } => (107, context, append);
    pub struct EnvironmentTrace { env_sources: Vec<String>, exported_vars_count: usize, redacted_secrets_count: usize } => (108, context, append);
    pub struct ActionConfig { overrides: usize, defaults: usize, override_keys: Vec<String> } => (109, context, append);
    pub struct DagGenerated { nodes: usize, edges: usize, cycles: usize } => (201, resolution, append);
    pub struct DependencyUpdate { task_id: String, in_degree: usize } => (202, resolution, append);
    pub struct TargetResolved { input: String, matched: usize } => (203, resolution, append);
    pub struct ExecutionSkipped { reason: String } => (301, execution, append);
    pub struct FileGenerated { target: String, action: String } => (401, filesystem, append);
    pub struct DirectoryChecked { target: String, is_writable: bool } => (402, filesystem, append);
    pub struct MemorySnapshot { phase: String, heap_alloc_kb: u64, task_delta_kb: i64 } => (501, core, append);
    pub struct ChangelogAnalysis { commits_scanned: usize } => (511, builtin, append);
    pub struct ChangelogOutput { target: String, sections_generated: usize } => (512, builtin, append);
    pub struct GitRelease { tag: String, provider: String, draft: bool } => (521, builtin, append);
    pub struct ArchiveOutput { format: String, size_bytes: u64, files: usize } => (531, builtin, append);
    pub struct VersionDiscovery { previous: String, next: String, impact: String } => (541, builtin, append);
    pub struct VersionOutput { count: usize } => (542, builtin, append);
    pub struct DocsScan { scans: usize, pages: usize, rst: usize, markdown: usize, references: usize, read_failures: usize } => (551, builtin, snapshot(DocsScan = "docs.scan"));
    pub struct DocsLinkAudit { enabled: bool, audits: usize, links: usize, anchors: usize, issues: usize } => (552, builtin, snapshot(DocsLinks = "docs.links"));
    pub struct DocsDiscoveryAudit { audits: usize, indexable: usize, excluded: usize, sitemap_files: usize, quality_warnings: usize, missing_descriptions: usize, long_titles: usize, description_length: usize, duplicate_titles: usize, duplicate_descriptions: usize, deployment_warnings: usize } => (553, builtin, snapshot(DocsDiscovery = "docs.discovery"));
    pub struct DocsOutput { renders: usize, pages_written: usize, assets_written: usize, write_failures: usize, bundle: String } => (554, builtin, snapshot(DocsOutput = "docs.output"));
    pub struct DocsServe { changes: usize, rebuilds: usize, succeeded: usize, failed: usize, reloads: usize, last_duration_ms: u128 } => (555, builtin, snapshot(DocsServe = "docs.serve"));
    pub struct SubprocessStart { command: String, cwd: String, shell: String } => (601, subprocess, append);
    pub struct SubprocessEnvironment { injected_vars: Vec<String> } => (602, subprocess, append);
    pub struct SubprocessFinish { duration_ms: u128, exit_code: Option<i64> } => (603, subprocess, append);
    pub struct SubprocessStep { task_id: String, pid: u32, step_cwd: String, exit_code: i32, duration_ms: u64 } => (604, subprocess, append);
    pub struct SubprocessRedaction { redaction_count: usize } => (605, subprocess, append);
}

#[macro_export]
macro_rules! debug_event {
    ($logger:expr, $event:expr $(,)?) => {{ $logger.debug_event($event) }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn registry_codes_are_unique_and_in_reserved_ranges() {
        let mut codes = HashSet::new();
        for code in ALL_CODES {
            assert!(codes.insert(*code), "duplicate debug code {code}");
            let number: u16 = code.strip_prefix("E_").unwrap().parse().unwrap();
            assert!((100..=799).contains(&number));
        }
    }

    #[test]
    fn registry_fields_and_snapshot_keys_are_valid_and_unique() {
        let snake_case = regex::Regex::new(r"^[a-z][a-z0-9_]*$").unwrap();
        let mut snapshots = HashSet::new();
        for descriptor in ALL_DEBUG_EVENTS {
            assert!(
                !descriptor.fields.is_empty(),
                "{} has no fields",
                descriptor.code
            );
            for field in descriptor.fields {
                assert!(snake_case.is_match(field), "invalid field {field}");
            }
            if let Some(key) = descriptor.snapshot_key {
                assert!(
                    !key.is_empty(),
                    "{} has an empty snapshot key",
                    descriptor.code
                );
                assert!(snapshots.insert(key), "duplicate snapshot key {key}");
            }
        }
    }

    #[test]
    fn formatter_and_parser_round_trip_supported_values() {
        let record = DebugRecord {
            code: "E_109",
            aggregation: DebugAggregation::Append,
            payload: vec![
                ("plain", "ready".into()),
                ("spaced", "hello world".into()),
                ("quoted", "say \"hello\"".into()),
                ("equals", "left=right".into()),
                ("unicode", "olá 世界".into()),
                ("newline", "first\nsecond".into()),
                ("numeric_string", "42".into()),
                ("float_string", "1.25".into()),
                ("enabled", true.into()),
                ("delta", (-9i64).into()),
                ("count", 42usize.into()),
                (
                    "items",
                    vec!["one".to_string(), "two words".to_string()].into(),
                ),
                ("missing", DebugValue::Null),
            ],
        };
        let formatted = format_record(&record);
        let parsed = parse_record(&formatted).unwrap();
        assert_eq!(parsed.code, record.code);
        assert_eq!(
            parsed.payload,
            record
                .payload
                .into_iter()
                .map(|(key, value)| (key.to_string(), value))
                .collect::<Vec<_>>()
        );
    }
}
