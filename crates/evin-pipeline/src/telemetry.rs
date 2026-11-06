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
use std::collections::BTreeMap;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TraceFormat {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub other_data: Option<TraceMetadata>,
    pub trace_events: Vec<TraceEvent>,
    pub display_time_unit: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TraceMetadata {
    pub schema: String,
    pub run_id: String,
    pub scope: TraceScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_trace: Option<String>,
    pub evin_version: String,
    pub command: String,
    pub targets: Vec<String>,
    pub context: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hook: Option<String>,
    pub mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub watch_session: Option<u64>,
    pub os: String,
    pub arch: String,
    pub started_at: String,
    pub duration_us: u64,
    pub outcome: String,
    pub summary: TraceSummary,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modules: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum TraceScope {
    Workspace,
    Module { module: String, root: String },
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct TraceSummary {
    pub tasks: usize,
    pub failed: usize,
    pub unverified: usize,
    pub cancelled: usize,
    pub max_concurrency: usize,
    pub critical_path_us: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct TraceEvent {
    pub name: String,
    pub cat: String,
    pub ph: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ts: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dur: Option<u64>,
    pub pid: u32,
    pub tid: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bp: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub s: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub args: BTreeMap<String, serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_format_roundtrips_and_serializes_deterministically() {
        let mut args = BTreeMap::new();
        args.insert("status".to_string(), serde_json::json!("success"));
        args.insert("queuedUs".to_string(), serde_json::json!(120));
        args.insert("readyUs".to_string(), serde_json::json!(1000));
        args.insert("module".to_string(), serde_json::json!("evin"));

        let event = TraceEvent {
            name: "evin:check".to_string(),
            cat: "task".to_string(),
            ph: "X".to_string(),
            ts: Some(1_000_000),
            dur: Some(250_000),
            pid: 2,
            tid: 1,
            id: None,
            bp: None,
            s: None,
            args,
        };

        let format = TraceFormat {
            other_data: Some(TraceMetadata {
                schema: "evin.trace/v1".to_string(),
                run_id: "01TEST".to_string(),
                scope: TraceScope::Module {
                    module: "evin".to_string(),
                    root: "crates/evin".to_string(),
                },
                workspace_trace: Some("../../.evin/trace.json".to_string()),
                evin_version: "0.2.0".to_string(),
                command: "run".to_string(),
                targets: vec!["evin:check".to_string()],
                context: "run".to_string(),
                hook: None,
                mode: "normal".to_string(),
                watch_session: None,
                os: "linux".to_string(),
                arch: "x86_64".to_string(),
                started_at: "2026-11-06T00:00:00Z".to_string(),
                duration_us: 250_000,
                outcome: "success".to_string(),
                summary: TraceSummary {
                    tasks: 1,
                    failed: 0,
                    unverified: 0,
                    cancelled: 0,
                    max_concurrency: 1,
                    critical_path_us: 250_000,
                },
                modules: vec![],
            }),
            trace_events: vec![event.clone()],
            display_time_unit: "ms".to_string(),
        };

        let json = serde_json::to_string_pretty(&format).expect("serialization succeeds");
        assert!(json.contains("\"schema\": \"evin.trace/v1\""));
        assert!(json.contains("\"displayTimeUnit\": \"ms\""));

        let deserialized: TraceFormat =
            serde_json::from_str(&json).expect("deserialization succeeds");
        assert_eq!(format, deserialized);
    }
}
