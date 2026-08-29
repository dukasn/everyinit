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

use crate::models::{CiPlan, CiRemoteState, RemoteRelease};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("Schema validation failed: {0}")]
    SchemaValidation(String),

    #[error("State ingestion error: {0}")]
    StateIngestion(String),

    #[error("Git inspection error: {0}")]
    GitInspection(String),

    #[error("Notes extraction error: {0}")]
    NotesExtraction(String),

    #[error("Artifact inspection error: {0}")]
    ArtifactInspection(String),

    #[error("Plan serialization/write error: {0}")]
    PlanWrite(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Release note slice with computed SHA-256 digest
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteSlice {
    pub body: String,
    pub sha256: String,
    pub sanitized_filename: String,
}

/// Local Git tag state
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalTagInfo {
    pub tag: String,
    pub commit: String,
    pub title: Option<String>,
    pub is_annotated: bool,
}

/// Local distribution artifact
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalArtifactInfo {
    pub local_path: String,
    pub destination_name: String,
    pub size_bytes: u64,
    pub sha256: Option<String>,
}

/// Ingestion source trait for remote forge release data
pub trait RemoteStateSource {
    fn load_remote_state(&self) -> Result<CiRemoteState, ProtocolError>;
}

/// Provider extracting markdown release notes slices
pub trait NotesExtractor {
    fn extract_notes(&self, tag: &str) -> Result<Option<NoteSlice>, ProtocolError>;
}

/// Provider querying local Git verified tags and target commit SHAs
pub trait GitStateProvider {
    fn local_tags(&self) -> Result<Vec<LocalTagInfo>, ProtocolError>;
}

/// Inspector scanning local distribution build outputs
pub trait ArtifactInspector {
    fn local_artifacts(&self, tag: &str) -> Result<Vec<LocalArtifactInfo>, ProtocolError>;
}

/// Policy evaluator deciding prerelease channels and draft/retention rules
pub trait PolicyEvaluator {
    fn is_prerelease(&self, tag: &str) -> bool;
    fn is_draft(&self, tag: &str) -> bool;
    fn should_prune_remote(&self, release: &RemoteRelease) -> bool;
}

/// Sink responsible for writing notes slices and plan.v1.json
pub trait PlanSink {
    fn write_notes_file(&mut self, filename: &str, content: &str) -> Result<String, ProtocolError>;
    fn write_plan(&mut self, plan: &CiPlan) -> Result<(), ProtocolError>;
}
