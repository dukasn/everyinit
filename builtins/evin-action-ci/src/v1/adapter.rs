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

use super::config::CiConfig;
use evin_action_sdk::RunnerContext;
use evin_ci::{
    ArtifactInspector, ChangelogSlicer, CiPlan, CiRemoteState, GitStateProvider, LocalArtifactInfo,
    LocalTagInfo, NoteSlice, NotesExtractor, PlanSink, PolicyEvaluator, ProtocolError,
    RemoteRelease, RemoteStateSource,
};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};

pub struct FsRemoteStateSource<'a> {
    pub workspace_root: &'a Path,
    pub state_path: &'a str,
}

impl<'a> RemoteStateSource for FsRemoteStateSource<'a> {
    fn load_remote_state(&self) -> Result<CiRemoteState, ProtocolError> {
        let full_path = self.workspace_root.join(self.state_path);
        if !full_path.exists() {
            return Err(ProtocolError::StateIngestion(format!(
                "Remote state file not found at '{}'",
                full_path.display()
            )));
        }
        let content = std::fs::read_to_string(&full_path).map_err(|e| {
            ProtocolError::StateIngestion(format!("Failed to read state file: {e}"))
        })?;
        let state: CiRemoteState = serde_json::from_str(&content)
            .map_err(|e| ProtocolError::SchemaValidation(format!("Invalid state JSON: {e}")))?;
        Ok(state)
    }
}

pub struct FsNotesExtractor {
    slicer: ChangelogSlicer,
}

impl FsNotesExtractor {
    pub fn new(workspace_root: &Path, changelog_path: &str) -> Self {
        let full_path = workspace_root.join(changelog_path);
        let slicer = if full_path.exists() {
            let content = std::fs::read_to_string(full_path).unwrap_or_default();
            ChangelogSlicer::parse(&content)
        } else {
            ChangelogSlicer::default()
        };
        Self { slicer }
    }
}

impl NotesExtractor for FsNotesExtractor {
    fn extract_notes(&self, tag: &str) -> Result<Option<NoteSlice>, ProtocolError> {
        Ok(self.slicer.get_slice(tag).cloned())
    }
}

pub struct CommandGitStateProvider<'a> {
    pub workspace_root: &'a Path,
    pub tag: Option<&'a str>,
    pub title: Option<&'a str>,
}

impl<'a> GitStateProvider for CommandGitStateProvider<'a> {
    fn local_tags(&self) -> Result<Vec<LocalTagInfo>, ProtocolError> {
        let output = std::process::Command::new("git")
            .args([
                "tag",
                "-l",
                "--format=%(refname:short)|%(*objectname)|%(objectname)|%(contents:subject)",
            ])
            .current_dir(self.workspace_root)
            .output()
            .map_err(|e| ProtocolError::GitInspection(format!("Failed to run git tag: {e}")))?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(ProtocolError::GitInspection(format!(
                "git tag failed: {err}"
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut tags = Vec::new();

        for line in stdout.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let mut parts = line.split('|');
            let tag_name = parts.next().unwrap_or("").trim().to_string();
            let peeled_commit = parts.next().unwrap_or("").trim().to_string();
            let raw_commit = parts.next().unwrap_or("").trim().to_string();
            let subject = parts.next().unwrap_or("").trim().to_string();

            if tag_name.is_empty() {
                continue;
            }

            let is_annotated = !peeled_commit.is_empty();
            let commit = if is_annotated {
                peeled_commit
            } else {
                raw_commit
            };

            let title = if let Some(title) = self.title {
                Some(title.to_string())
            } else if !subject.is_empty() {
                Some(subject)
            } else {
                Some(tag_name.clone())
            };

            tags.push(LocalTagInfo {
                tag: tag_name,
                commit,
                title,
                is_annotated,
            });
        }

        if let Some(tag) = self.tag {
            tags.retain(|local| local.tag == tag);
            if tags.is_empty() {
                return Err(ProtocolError::GitInspection(format!(
                    "Configured release tag '{tag}' does not exist locally"
                )));
            }
        }
        Ok(tags)
    }
}

pub struct FsArtifactInspector<'a> {
    pub workspace_root: &'a Path,
    pub artifacts_dir: &'a str,
}

impl<'a> ArtifactInspector for FsArtifactInspector<'a> {
    fn local_artifacts(&self, _tag: &str) -> Result<Vec<LocalArtifactInfo>, ProtocolError> {
        let dir = self.workspace_root.join(self.artifacts_dir);
        if !dir.is_dir() {
            return Ok(Vec::new());
        }

        let mut artifacts = Vec::new();
        let entries = std::fs::read_dir(&dir).map_err(|e| {
            ProtocolError::ArtifactInspection(format!("Failed to read artifacts dir: {e}"))
        })?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                let metadata = entry.metadata().map_err(|e| {
                    ProtocolError::ArtifactInspection(format!("Failed to read metadata: {e}"))
                })?;
                let mut file = std::fs::File::open(&path).map_err(|e| {
                    ProtocolError::ArtifactInspection(format!("Failed to open artifact: {e}"))
                })?;
                let mut hasher = Sha256::new();
                let mut buffer = [0; 64 * 1024];
                loop {
                    let read = file.read(&mut buffer).map_err(|e| {
                        ProtocolError::ArtifactInspection(format!("Failed to hash artifact: {e}"))
                    })?;
                    if read == 0 {
                        break;
                    }
                    hasher.update(&buffer[..read]);
                }
                let file_name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();
                let relative = path
                    .strip_prefix(self.workspace_root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .to_string();

                artifacts.push(LocalArtifactInfo {
                    local_path: relative,
                    destination_name: file_name,
                    size_bytes: metadata.len(),
                    sha256: Some(format!("{:x}", hasher.finalize())),
                });
            }
        }

        artifacts.sort_by(|left, right| left.destination_name.cmp(&right.destination_name));
        Ok(artifacts)
    }
}

pub struct ContextPolicyEvaluator<'a> {
    pub ctx: &'a RunnerContext,
    pub prune_orphans: bool,
}

impl<'a> PolicyEvaluator for ContextPolicyEvaluator<'a> {
    fn is_prerelease(&self, tag: &str) -> bool {
        let clean = tag.trim_start_matches('v');
        if let Ok(semver) = semver::Version::parse(clean) {
            return !semver.pre.is_empty();
        }
        tag.contains('-')
            || tag.contains("rc")
            || tag.contains("canary")
            || tag.contains("beta")
            || tag.contains("alpha")
    }

    fn is_draft(&self, _tag: &str) -> bool {
        false
    }

    fn should_prune_remote(&self, release: &RemoteRelease) -> bool {
        self.prune_orphans && release.is_draft
    }
}

pub struct FsPlanSink<'a> {
    pub workspace_root: &'a Path,
    pub config: &'a CiConfig,
    pub written_notes: Vec<PathBuf>,
}

impl<'a> FsPlanSink<'a> {
    pub fn new(workspace_root: &'a Path, config: &'a CiConfig) -> Self {
        Self {
            workspace_root,
            config,
            written_notes: Vec::new(),
        }
    }
}

impl<'a> PlanSink for FsPlanSink<'a> {
    fn write_notes_file(&mut self, filename: &str, content: &str) -> Result<String, ProtocolError> {
        let relative_path = Path::new(&self.config.notes_dir).join(filename);
        let full_path = self.workspace_root.join(&relative_path);

        if !self.config.dry_run {
            if let Some(parent) = full_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    ProtocolError::PlanWrite(format!(
                        "Failed to create notes dir '{}': {e}",
                        parent.display()
                    ))
                })?;
            }
            std::fs::write(&full_path, content).map_err(|e| {
                ProtocolError::PlanWrite(format!(
                    "Failed to write note file '{}': {e}",
                    full_path.display()
                ))
            })?;
        }

        self.written_notes.push(relative_path.clone());
        Ok(relative_path.to_string_lossy().to_string())
    }

    fn write_plan(&mut self, plan: &CiPlan) -> Result<(), ProtocolError> {
        let full_path = self.workspace_root.join(&self.config.plan_out);
        if !self.config.dry_run {
            if let Some(parent) = full_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    ProtocolError::PlanWrite(format!(
                        "Failed to create plan dir '{}': {e}",
                        parent.display()
                    ))
                })?;
            }
            let serialized = serde_json::to_string_pretty(plan).map_err(|e| {
                ProtocolError::PlanWrite(format!("Failed to serialize plan JSON: {e}"))
            })?;
            std::fs::write(&full_path, serialized).map_err(|e| {
                ProtocolError::PlanWrite(format!(
                    "Failed to write plan file '{}': {e}",
                    full_path.display()
                ))
            })?;
        }
        Ok(())
    }
}
