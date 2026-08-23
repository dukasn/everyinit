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

use serde::Serialize;
use std::collections::HashMap;

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TraceFormat {
    pub trace_events: Vec<TraceEvent>,
    pub display_time_unit: String,
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct TraceEvent {
    pub name: String,
    pub cat: String,
    pub ph: String,
    pub ts: u64,
    pub dur: u64,
    pub pid: u32,
    pub tid: u64,
    pub args: HashMap<String, String>,
}
