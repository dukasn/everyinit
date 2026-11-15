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

use crate::traits::NoteSlice;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

/// Slices CHANGELOG.md into version sections and calculates SHA-256 digests
#[derive(Debug, Clone, Default)]
pub struct ChangelogSlicer {
    slices: HashMap<String, NoteSlice>,
    headings: HashSet<String>,
    duplicates: HashSet<String>,
}

impl ChangelogSlicer {
    pub fn parse(markdown: &str) -> Self {
        let mut slices = HashMap::new();
        let mut seen = HashSet::new();
        let mut duplicates = HashSet::new();
        let mut current_tag: Option<String> = None;
        let mut current_body_lines: Vec<&str> = Vec::new();

        let flush =
            |tag: &Option<String>, lines: &[&str], slices: &mut HashMap<String, NoteSlice>| {
                if let Some(t) = tag {
                    let trimmed_body = lines.join("\n").trim().to_string();
                    if !trimmed_body.is_empty() {
                        let mut hasher = Sha256::new();
                        hasher.update(trimmed_body.as_bytes());
                        let sha256 = hasher
                            .finalize()
                            .iter()
                            .map(|byte| format!("{byte:02x}"))
                            .collect();
                        let sanitized = sanitize_tag_filename(t);
                        slices.insert(
                            t.clone(),
                            NoteSlice {
                                body: trimmed_body,
                                sha256,
                                sanitized_filename: sanitized,
                            },
                        );
                    }
                }
            };

        for line in markdown.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("## ") {
                let heading_content = trimmed.trim_start_matches("## ").trim();
                let candidate_tag = if let Some(inside) = heading_content.strip_prefix('[') {
                    inside.split(']').next().unwrap_or("").trim()
                } else {
                    heading_content
                        .split_whitespace()
                        .next()
                        .unwrap_or("")
                        .trim()
                };

                flush(&current_tag, &current_body_lines, &mut slices);
                current_body_lines.clear();
                if !candidate_tag.is_empty() && candidate_tag != "Unreleased" {
                    if !seen.insert(candidate_tag.to_string()) {
                        duplicates.insert(candidate_tag.to_string());
                    }
                    current_tag = Some(candidate_tag.to_string());
                } else {
                    current_tag = None;
                }
                continue;
            }

            if current_tag.is_some() {
                current_body_lines.push(line);
            }
        }

        flush(&current_tag, &current_body_lines, &mut slices);
        Self {
            slices,
            headings: seen,
            duplicates,
        }
    }

    pub fn get_slice(&self, tag: &str) -> Option<&NoteSlice> {
        let clean = tag.trim();
        self.slices.get(clean).or_else(|| {
            // Also try with or without leading 'v'
            if let Some(stripped) = clean.strip_prefix('v') {
                self.slices.get(stripped)
            } else {
                let with_v = format!("v{clean}");
                self.slices.get(&with_v)
            }
        })
    }

    pub fn get_unique_slice(&self, tag: &str) -> Result<Option<&NoteSlice>, String> {
        let clean = tag.trim();
        let alternate = clean
            .strip_prefix('v')
            .map(str::to_string)
            .unwrap_or_else(|| format!("v{clean}"));
        if self.duplicates.contains(clean)
            || self.duplicates.contains(&alternate)
            || (self.headings.contains(clean) && self.headings.contains(&alternate))
        {
            return Err(format!("Duplicate changelog sections for {clean}"));
        }
        Ok(self.get_slice(clean))
    }
}

pub fn sanitize_tag_filename(tag: &str) -> String {
    let clean: String = tag
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '.' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = clean.trim_matches('_');
    if trimmed.is_empty() {
        "release.md".to_string()
    } else {
        format!("{trimmed}.md")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_slices_changelog_sections() {
        let sample = r#"# Changelog
All notable changes to this project will be documented in this file.

## [Unreleased]
### Features
- upcoming feature

## [v1.0.0-canary.6] - 2026-08-25
### Features
- **docs**: deterministic offline mermaid support
- **process**: path lookup fix on windows

### Bug Fixes
- fixed pipe drain on exit

## [v1.0.0-canary.5] - 2026-08-20
### Features
- initial canary
"#;

        let slicer = ChangelogSlicer::parse(sample);
        let slice6 = slicer.get_slice("v1.0.0-canary.6").unwrap();
        assert!(
            slice6
                .body
                .contains("deterministic offline mermaid support")
        );
        assert!(slice6.body.contains("fixed pipe drain on exit"));
        assert_eq!(slice6.sanitized_filename, "v1.0.0-canary.6.md");
        assert_eq!(slice6.sha256.len(), 64);

        let slice5 = slicer.get_slice("v1.0.0-canary.5").unwrap();
        assert!(slice5.body.contains("initial canary"));
        assert_ne!(slice6.sha256, slice5.sha256);

        assert!(slicer.get_slice("v2.0.0").is_none());
    }

    #[test]
    fn unique_slices_reject_duplicates_and_stop_at_unreleased() {
        let slicer = ChangelogSlicer::parse(
            "## [v0.1.1]\n### Fixed\n- first\n## [Unreleased]\n- future\n## [v0.1.0]\n- old\n",
        );
        assert_eq!(
            slicer.get_unique_slice("v0.1.1").unwrap().unwrap().body,
            "### Fixed\n- first"
        );
        assert!(slicer.get_unique_slice("v0.2.0").unwrap().is_none());
        for duplicate in [
            "## [v0.1.1]\n- first\n## [v0.1.1]\n",
            "## [v0.1.1]\n- first\n## [0.1.1]\n",
        ] {
            assert!(
                ChangelogSlicer::parse(duplicate)
                    .get_unique_slice("v0.1.1")
                    .is_err()
            );
        }
    }
}
