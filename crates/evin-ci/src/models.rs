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

pub const SCHEMA_REMOTE_STATE_V1: &str = "https://everyinit.dev/schemas/v1/ci-remote-state.json";
pub const SCHEMA_PLAN_V1: &str = "https://everyinit.dev/schemas/v1/ci-plan.json";

/// ECP-1 Normalized Ingestion State (`remote-state.v1.json`)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CiRemoteState {
    #[serde(default = "default_remote_state_schema", rename = "$schema")]
    pub schema: Option<String>,
    pub v: u32,
    pub platform: String,
    pub repository: String,
    #[serde(default)]
    pub releases: Vec<RemoteRelease>,
}

fn default_remote_state_schema() -> Option<String> {
    Some(SCHEMA_REMOTE_STATE_V1.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteRelease {
    pub tag: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default)]
    pub is_draft: bool,
    #[serde(default)]
    pub is_prerelease: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_at: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assets: Vec<RemoteAsset>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteAsset {
    pub name: String,
    pub size_bytes: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

/// ECP-1 Reconciliation Plan (`plan.v1.json`)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CiPlan {
    #[serde(default = "default_plan_schema", rename = "$schema")]
    pub schema: Option<String>,
    pub v: u32,
    pub status: u16,
    pub reconciled: bool,
    pub delta_count: usize,
    #[serde(default)]
    pub plan: Vec<CiPlanAction>,
}

fn default_plan_schema() -> Option<String> {
    Some(SCHEMA_PLAN_V1.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CiPlanAction {
    pub id: String,
    pub op: u16,
    pub code: ActionOpcode,
    pub reason: ReasonCode,
    pub r#ref: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes_src: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flags: Option<ReleaseFlags>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub src: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dest: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ReleaseFlags {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prerelease: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionOpcode {
    #[serde(rename = "SYNC_OK")]
    SyncOk = 100,
    #[serde(rename = "REL_INIT")]
    RelInit = 201,
    #[serde(rename = "REL_MUT")]
    RelMut = 202,
    #[serde(rename = "REL_NOTES_SYNC")]
    RelNotesSync = 203,
    #[serde(rename = "REL_PRUNE")]
    RelPrune = 204,
    #[serde(rename = "REL_RETARGET")]
    RelRetarget = 205,
    #[serde(rename = "ART_PUT")]
    ArtPut = 301,
    #[serde(rename = "ART_PRUNE")]
    ArtPrune = 304,
}

impl ActionOpcode {
    pub fn opcode_number(&self) -> u16 {
        *self as u16
    }

    pub fn code_name(&self) -> &'static str {
        match self {
            Self::SyncOk => "SYNC_OK",
            Self::RelInit => "REL_INIT",
            Self::RelMut => "REL_MUT",
            Self::RelNotesSync => "REL_NOTES_SYNC",
            Self::RelPrune => "REL_PRUNE",
            Self::RelRetarget => "REL_RETARGET",
            Self::ArtPut => "ART_PUT",
            Self::ArtPrune => "ART_PRUNE",
        }
    }

    pub fn is_mutating(&self) -> bool {
        !matches!(self, Self::SyncOk)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReasonCode {
    #[serde(rename = "DIFF_ABSENT")]
    DiffAbsent = 0x01,
    #[serde(rename = "DIFF_CHECKSUM")]
    DiffChecksum = 0x02,
    #[serde(rename = "DIFF_BODY_STALE")]
    DiffBodyStale = 0x03,
    #[serde(rename = "DIFF_TARGET_DRIFT")]
    DiffTargetDrift = 0x04,
    #[serde(rename = "DIFF_PRUNE_POLICY")]
    DiffPrunePolicy = 0x05,
    #[serde(rename = "DIFF_BODY_EMPTY")]
    DiffBodyEmpty = 0x06,
    #[serde(rename = "DIFF_CHANNEL_DESYNC")]
    DiffChannelDesync = 0x07,
}

impl ReasonCode {
    pub fn reason_name(&self) -> &'static str {
        match self {
            Self::DiffAbsent => "DIFF_ABSENT",
            Self::DiffChecksum => "DIFF_CHECKSUM",
            Self::DiffBodyStale => "DIFF_BODY_STALE",
            Self::DiffTargetDrift => "DIFF_TARGET_DRIFT",
            Self::DiffPrunePolicy => "DIFF_PRUNE_POLICY",
            Self::DiffBodyEmpty => "DIFF_BODY_EMPTY",
            Self::DiffChannelDesync => "DIFF_CHANNEL_DESYNC",
        }
    }

    pub fn hex_code(&self) -> u8 {
        *self as u8
    }
}
