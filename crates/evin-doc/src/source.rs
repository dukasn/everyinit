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

//! Filesystem-neutral documentation source discovery.

use crate::site::DocIssue;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedDocFile {
    pub absolute_path: PathBuf,
    pub relative_path: PathBuf,
    pub slug: String,
    pub is_rst: bool,
}

pub struct SourceResolver;

impl SourceResolver {
    pub fn resolve_all(
        source_root: &Path,
        include_patterns: &[String],
        exclude_patterns: &[String],
    ) -> Result<Vec<ResolvedDocFile>, DocIssue> {
        let mut results = Vec::new();
        if include_patterns.is_empty() {
            Self::traverse_directory(source_root, source_root, &mut results, exclude_patterns)?;
        } else {
            for pattern in include_patterns {
                Self::resolve_pattern(source_root, pattern, &mut results, exclude_patterns)?;
            }
        }
        results.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
        results.dedup_by(|a, b| a.relative_path == b.relative_path);
        Ok(results)
    }

    fn resolve_pattern(
        source_root: &Path,
        pattern: &str,
        results: &mut Vec<ResolvedDocFile>,
        exclude_patterns: &[String],
    ) -> Result<(), DocIssue> {
        let raw_target = source_root.join(pattern);
        if raw_target.is_file()
            && let Some(doc) = Self::create_resolved(source_root, &raw_target, exclude_patterns)?
        {
            results.push(doc);
            return Ok(());
        }
        if raw_target.is_dir() {
            return Self::traverse_directory(source_root, &raw_target, results, exclude_patterns);
        }
        let candidates = [
            format!("{pattern}.md"),
            format!("{pattern}.rst"),
            format!("{pattern}.markdown"),
            format!("{pattern}/index.md"),
            format!("{pattern}/index.rst"),
        ];
        let mut found_probe = false;
        for candidate in candidates {
            let candidate_path = source_root.join(candidate);
            if candidate_path.is_file()
                && let Some(doc) =
                    Self::create_resolved(source_root, &candidate_path, exclude_patterns)?
            {
                results.push(doc);
                found_probe = true;
                break;
            }
        }
        if !found_probe
            && (pattern.contains('*') || pattern.contains('?'))
            && let Ok(glob) = globset::Glob::new(&format!("{}/{}", source_root.display(), pattern))
        {
            Self::traverse_matching(
                source_root,
                source_root,
                &glob.compile_matcher(),
                results,
                exclude_patterns,
            )?;
        }
        Ok(())
    }

    fn traverse_directory(
        source_root: &Path,
        dir: &Path,
        results: &mut Vec<ResolvedDocFile>,
        exclude_patterns: &[String],
    ) -> Result<(), DocIssue> {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    Self::traverse_directory(source_root, &path, results, exclude_patterns)?;
                } else if path.is_file()
                    && let Some(doc) = Self::create_resolved(source_root, &path, exclude_patterns)?
                {
                    results.push(doc);
                }
            }
        }
        Ok(())
    }

    fn traverse_matching(
        source_root: &Path,
        dir: &Path,
        matcher: &globset::GlobMatcher,
        results: &mut Vec<ResolvedDocFile>,
        exclude_patterns: &[String],
    ) -> Result<(), DocIssue> {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    Self::traverse_matching(
                        source_root,
                        &path,
                        matcher,
                        results,
                        exclude_patterns,
                    )?;
                } else if path.is_file()
                    && matcher.is_match(&path)
                    && let Some(doc) = Self::create_resolved(source_root, &path, exclude_patterns)?
                {
                    results.push(doc);
                }
            }
        }
        Ok(())
    }

    fn create_resolved(
        source_root: &Path,
        file_path: &Path,
        exclude_patterns: &[String],
    ) -> Result<Option<ResolvedDocFile>, DocIssue> {
        let Some(ext) = file_path
            .extension()
            .and_then(|extension| extension.to_str())
        else {
            return Ok(None);
        };
        let ext = ext.to_ascii_lowercase();
        if !matches!(ext.as_str(), "md" | "rst" | "markdown") {
            return Ok(None);
        }
        let Some(relative_path) = file_path
            .strip_prefix(source_root)
            .ok()
            .map(Path::to_path_buf)
        else {
            return Ok(None);
        };
        if exclude_patterns.iter().any(|pattern| {
            globset::Glob::new(pattern)
                .map(|glob| glob.compile_matcher().is_match(&relative_path))
                .unwrap_or(false)
        }) {
            return Ok(None);
        }
        Self::navigation_order(&relative_path)?;
        Ok(Some(ResolvedDocFile {
            absolute_path: file_path.to_path_buf(),
            slug: Self::compute_slug(&relative_path),
            relative_path,
            is_rst: ext == "rst",
        }))
    }

    /// Returns the optional source-only navigation order encoded as `NN.` in a filename.
    pub fn navigation_order(relative_path: &Path) -> Result<Option<u64>, DocIssue> {
        let stem = relative_path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or_else(|| {
                DocIssue::error(
                    "invalid-nav-order",
                    format!(
                        "Documentation filename '{}' is not valid UTF-8",
                        relative_path.display()
                    ),
                )
                .with_source_path(relative_path.display().to_string())
            })?;
        let Some((digits, name)) = navigation_prefix(stem) else {
            return Ok(None);
        };
        if name.is_empty() {
            return Err(DocIssue::error(
                "invalid-nav-order",
                format!(
                    "Documentation filename '{}' must include a name after its navigation prefix",
                    relative_path.display()
                ),
            )
            .with_source_path(relative_path.display().to_string()));
        }
        let order = digits.parse::<u64>().map_err(|_| {
            DocIssue::error(
                "invalid-nav-order",
                format!(
                    "Navigation prefix '{digits}' in '{}' is too large",
                    relative_path.display()
                ),
            )
            .with_source_path(relative_path.display().to_string())
        })?;
        Ok(Some(order))
    }

    pub fn compute_slug(relative_path: &Path) -> String {
        let mut slug_path = relative_path.with_extension("");
        let file_name = slug_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("")
            .to_owned();
        if let Some((_, name)) = navigation_prefix(&file_name)
            && !name.is_empty()
        {
            slug_path.set_file_name(name);
        }
        let file_name = slug_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if matches!(file_name, "index" | "README")
            && let Some(parent) = slug_path.parent()
        {
            slug_path = parent.to_path_buf();
        }
        let slug = slug_path.to_string_lossy().replace('\\', "/");
        if slug.is_empty() {
            "index".to_string()
        } else {
            slug
        }
    }
}

fn navigation_prefix(stem: &str) -> Option<(&str, &str)> {
    let (digits, name) = stem.split_once('.')?;
    (!digits.is_empty() && digits.chars().all(|character| character.is_ascii_digit()))
        .then_some((digits, name))
}

#[cfg(test)]
mod tests {
    use super::SourceResolver;
    use std::path::Path;

    #[test]
    fn index_and_readme_resolve_to_parent_routes() {
        assert_eq!(
            SourceResolver::compute_slug(Path::new("index.rst")),
            "index"
        );
        assert_eq!(
            SourceResolver::compute_slug(Path::new("guide/index.md")),
            "guide"
        );
        assert_eq!(
            SourceResolver::compute_slug(Path::new("guide/README.md")),
            "guide"
        );
    }

    #[test]
    fn navigation_prefix_is_source_only_route_metadata() {
        assert_eq!(
            SourceResolver::compute_slug(Path::new("esh/00.index.md")),
            "esh"
        );
        assert_eq!(
            SourceResolver::compute_slug(Path::new("esh/10.writing-scripts.md")),
            "esh/writing-scripts"
        );
        assert_eq!(
            SourceResolver::navigation_order(Path::new("esh/010.writing-scripts.md")).unwrap(),
            Some(10)
        );
        assert_eq!(
            SourceResolver::navigation_order(Path::new("esh/writing-scripts.md")).unwrap(),
            None
        );
    }

    #[test]
    fn invalid_navigation_prefixes_are_rejected() {
        assert_eq!(
            SourceResolver::navigation_order(Path::new("esh/01..md"))
                .unwrap_err()
                .code,
            "invalid-nav-order"
        );
        assert_eq!(
            SourceResolver::navigation_order(Path::new(
                "esh/999999999999999999999999999999999999.guide.md"
            ))
            .unwrap_err()
            .code,
            "invalid-nav-order"
        );
    }

    #[test]
    fn source_resolution_reports_invalid_navigation_prefixes() {
        let temp = tempfile::tempdir().unwrap();
        let invalid = temp.path().join("01..md");
        std::fs::write(&invalid, "# Invalid").unwrap();

        let error = SourceResolver::resolve_all(temp.path(), &[], &[]).unwrap_err();
        assert_eq!(error.code, "invalid-nav-order");
        assert_eq!(error.source_path.as_deref(), Some("01..md"));
    }
}
