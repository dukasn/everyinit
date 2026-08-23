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

use include_dir::Dir;
use std::io;
use std::path::{Path, PathBuf};

/// A custom filesystem implementation for `grass::Fs` that resolves SCSS imports
/// from a 4-tier source chain:
///
/// 1. **Custom layout directory** (on-disk) — user overrides via `layout_dir` config
/// 2. **Theme-specific embedded** — e.g. `matter/styles/_tokens.scss` in `LAYOUTS_DIR`
/// 3. **Shared embedded styles** — `styles/_tokens.scss` in `LAYOUTS_DIR`
/// 4. **Matter embedded fallback** — layout files in `matter/`
///
/// This enables Sass `@use` / `@import` to work transparently with `include_dir`'s
/// in-memory filesystem while supporting user-provided layout overrides on disk.
#[derive(Debug)]
pub struct EmbeddedFs<'a> {
    /// The embedded layouts directory (`include_dir!`)
    layouts_dir: &'a Dir<'a>,
    /// Current theme name (e.g. "matter")
    theme: String,
    /// Optional custom layout directory path on disk
    custom_path: Option<PathBuf>,
}

impl<'a> EmbeddedFs<'a> {
    pub fn new(layouts_dir: &'a Dir<'a>, theme: &str, custom_path: Option<PathBuf>) -> Self {
        Self {
            layouts_dir,
            theme: theme.to_string(),
            custom_path,
        }
    }

    fn normalize_path(path: &Path) -> PathBuf {
        let mut normalized = PathBuf::new();
        for component in path.components() {
            match component {
                std::path::Component::CurDir => {}
                std::path::Component::ParentDir => {
                    normalized.pop();
                }
                std::path::Component::Normal(c) => {
                    normalized.push(c);
                }
                _ => {}
            }
        }
        normalized
    }

    /// Resolve a path through the chain, returning the first match.
    /// Returns (source_description, resolved_embedded_path) for embedded files,
    /// or the on-disk path for custom files.
    fn resolve_file(&self, path: &Path) -> Option<ResolvedFile<'a>> {
        let norm_path = Self::normalize_path(path);

        // Tier 1: Custom layout directory on disk
        if let Some(ref custom) = self.custom_path {
            let disk_path = custom.join(&norm_path);
            if disk_path.is_file() {
                return Some(ResolvedFile::Disk(disk_path));
            }
        }

        // Tier 2: Theme-specific embedded (e.g. matter/tokens/_tokens.scss)
        let theme_path = format!("{}/{}", self.theme, norm_path.display());
        if let Some(file) = self.layouts_dir.get_file(&theme_path) {
            return Some(ResolvedFile::Embedded(file));
        }

        // Tier 3: Direct embedded lookup (if path is already prefixed)
        if let Some(file) = self.layouts_dir.get_file(&norm_path) {
            return Some(ResolvedFile::Embedded(file));
        }

        // Tier 4: Matter embedded fallback
        if self.theme != "matter" {
            let matter_path = format!("matter/{}", norm_path.display());
            if let Some(file) = self.layouts_dir.get_file(&matter_path) {
                return Some(ResolvedFile::Embedded(file));
            }
        }

        None
    }

    /// Check if a directory exists in the chain.
    fn resolve_dir(&self, path: &Path) -> bool {
        let norm_path = Self::normalize_path(path);

        // Tier 1: Custom layout directory on disk
        if let Some(ref custom) = self.custom_path
            && custom.join(&norm_path).is_dir()
        {
            return true;
        }

        // Tier 2: Theme-specific embedded
        let theme_path = format!("{}/{}", self.theme, norm_path.display());
        if self.layouts_dir.get_dir(&theme_path).is_some() {
            return true;
        }

        // Tier 3: Direct embedded lookup
        if self.layouts_dir.get_dir(&norm_path).is_some() {
            return true;
        }

        // Tier 4: Matter embedded fallback
        if self.theme != "matter" {
            let matter_path = format!("matter/{}", norm_path.display());
            if self.layouts_dir.get_dir(&matter_path).is_some() {
                return true;
            }
        }

        false
    }
}

/// Represents a resolved file from either the on-disk filesystem or embedded assets.
enum ResolvedFile<'a> {
    Disk(PathBuf),
    Embedded(&'a include_dir::File<'a>),
}

impl grass::Fs for EmbeddedFs<'_> {
    fn is_file(&self, path: &Path) -> bool {
        self.resolve_file(path).is_some()
    }

    fn is_dir(&self, path: &Path) -> bool {
        self.resolve_dir(path)
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        match self.resolve_file(path) {
            Some(ResolvedFile::Disk(disk_path)) => std::fs::read(&disk_path),
            Some(ResolvedFile::Embedded(file)) => Ok(file.contents().to_vec()),
            None => Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "SCSS file not found in layout chain: '{}' (theme: '{}')",
                    path.display(),
                    self.theme
                ),
            )),
        }
    }

    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        // For embedded files, canonicalization is a no-op since paths are virtual.
        // For on-disk files in custom_path, we could canonicalize, but keeping it
        // simple avoids issues with symlinks and different filesystem behaviors.
        Ok(path.to_path_buf())
    }
}
