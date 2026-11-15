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

use evin_release_sync::{GitStateProvider, LocalTagInfo, ProtocolError};
use std::path::Path;

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
