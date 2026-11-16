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

use crate::models::{ReleaseSyncPlan, RemoteRelease, RemoteReleaseState};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("Unsupported remote state schema version '{0}', expected 1")]
    UnsupportedSchemaVersion(u32),

    #[error("Remote state file not found at '{path}'")]
    StateFileNotFound { path: std::path::PathBuf },

    #[error("Failed to read remote state file '{path}': {source}")]
    StateFileRead {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Remote state JSON is invalid: {source}")]
    StateJsonInvalid {
        #[source]
        source: serde_json::Error,
    },

    #[error("Failed to run git tag: {source}")]
    GitTagCommandFailed {
        #[source]
        source: std::io::Error,
    },

    #[error("git tag failed: {stderr}")]
    GitTagFailed { stderr: String },

    #[error("Configured release tag '{tag}' does not exist locally")]
    ConfiguredTagMissing { tag: String },

    #[error("Failed to read artifacts directory: {source}")]
    ArtifactsDirectoryRead {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to read artifact metadata for '{path}': {source}")]
    ArtifactMetadataRead {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to open artifact '{path}': {source}")]
    ArtifactOpen {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to hash artifact '{path}': {source}")]
    ArtifactHash {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to create release notes directory '{path}': {source}")]
    NotesDirectoryCreate {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to write release notes file '{path}': {source}")]
    NotesFileWrite {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to create release plan directory '{path}': {source}")]
    PlanDirectoryCreate {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to serialize release plan: {source}")]
    PlanSerialize {
        #[source]
        source: serde_json::Error,
    },

    #[error("Failed to write release plan '{path}': {source}")]
    PlanFileWrite {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },

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
    fn load_remote_state(&self) -> Result<RemoteReleaseState, ProtocolError>;
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
    fn write_plan(&mut self, plan: &ReleaseSyncPlan) -> Result<(), ProtocolError>;
}
