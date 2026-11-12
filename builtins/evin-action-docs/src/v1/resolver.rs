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

use crate::v1::DocsConfig;
use crate::v1::LAYOUTS_DIR;
use crate::v1::fs::EmbeddedFs;
use evin_diagnostic::{Diagnostic, IssueCode, Severity};
use std::path::{Path, PathBuf};

pub use evin_doc::SourceResolver as DocResolver;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedLogo {
    CustomSvg(String),
    NeutralFallback,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedFavicon {
    Custom {
        bytes: Vec<u8>,
        extension: String,
        mime_type: String,
    },
    NeutralFallback,
}

#[derive(Debug)]
pub struct ResolvedResources {
    pub logo: ResolvedLogo,
    pub favicon: ResolvedFavicon,
    pub icons_dir: Option<PathBuf>,
    pub layout_dir: Option<PathBuf>,
    pub shell_html: String,
    pub raw_css: String,
    pub raw_search_js: String,
    pub embedded_fs: EmbeddedFs<'static>,
    pub diagnostics: Vec<Diagnostic>,
}

pub struct ResourceResolver<'a> {
    workspace_root: &'a Path,
    config: &'a DocsConfig,
    extensions_root: PathBuf,
}

impl<'a> ResourceResolver<'a> {
    pub fn new(workspace_root: &'a Path, config: &'a DocsConfig) -> Result<Self, Diagnostic> {
        let ext_rel = config
            .extensions_dir
            .as_deref()
            .unwrap_or(".evin/actions/docs");
        let ext_path = PathBuf::from(ext_rel);
        let extensions_root = if ext_path.is_absolute() {
            ext_path
        } else {
            workspace_root.join(ext_path)
        };
        Ok(Self {
            workspace_root,
            config,
            extensions_root,
        })
    }

    pub fn extensions_root(&self) -> &Path {
        &self.extensions_root
    }

    pub fn resolve_logo(
        &self,
        _diagnostics: &mut Vec<Diagnostic>,
    ) -> Result<ResolvedLogo, Diagnostic> {
        // Tier 1: Explicit config.logo
        if let Some(ref logo_rel) = self.config.appearance.logo {
            let p = logo_rel;
            let full_path = if p.is_absolute() {
                p.to_path_buf()
            } else {
                self.workspace_root.join(p)
            };
            if !full_path.is_file() {
                return Err(Diagnostic::new(
                    format!(
                        "Explicit logo path '{}' does not exist or is not a file",
                        full_path.display()
                    ),
                    IssueCode::new("invalid-config"),
                ));
            }
            let svg = std::fs::read_to_string(&full_path).map_err(|err| {
                Diagnostic::new(
                    format!(
                        "Failed to read explicit logo file '{}': {err}",
                        full_path.display()
                    ),
                    IssueCode::new("io-error"),
                )
            })?;
            return Ok(ResolvedLogo::CustomSvg(svg));
        }

        // Branding is opt-in. Do not infer a logo from repository conventions.
        Ok(ResolvedLogo::NeutralFallback)
    }

    pub fn resolve_favicon(
        &self,
        _diagnostics: &mut Vec<Diagnostic>,
    ) -> Result<ResolvedFavicon, Diagnostic> {
        // Tier 1: Explicit config.favicon
        if let Some(ref fav_rel) = self.config.appearance.favicon {
            let p = fav_rel;
            let full_path = if p.is_absolute() {
                p.to_path_buf()
            } else {
                self.workspace_root.join(p)
            };
            if !full_path.is_file() {
                return Err(Diagnostic::new(
                    format!(
                        "Explicit favicon path '{}' does not exist or is not a file",
                        full_path.display()
                    ),
                    IssueCode::new("invalid-config"),
                ));
            }
            let ext = full_path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("svg")
                .to_lowercase();
            let mime = mime_for_extension(&ext);
            let bytes = std::fs::read(&full_path).map_err(|err| {
                Diagnostic::new(
                    format!(
                        "Failed to read explicit favicon file '{}': {err}",
                        full_path.display()
                    ),
                    IssueCode::new("io-error"),
                )
            })?;
            return Ok(ResolvedFavicon::Custom {
                bytes,
                extension: ext,
                mime_type: mime.to_string(),
            });
        }

        // Tier 3 & 4: Neutral fallback
        Ok(ResolvedFavicon::NeutralFallback)
    }

    pub fn resolve_icons_dir(&self) -> Result<Option<PathBuf>, Diagnostic> {
        // Tier 1: Explicit config.icon_dir
        if let Some(ref icon_rel) = self.config.icon_dir {
            let p = Path::new(icon_rel);
            let full_path = if p.is_absolute() {
                p.to_path_buf()
            } else {
                self.workspace_root.join(p)
            };
            if !full_path.is_dir() {
                return Err(Diagnostic::new(
                    format!(
                        "Explicit icon directory '{}' does not exist or is not a directory",
                        full_path.display()
                    ),
                    IssueCode::new("invalid-config"),
                ));
            }
            return Ok(Some(full_path));
        }

        // Tier 3 & 4: None
        Ok(None)
    }

    pub fn resolve_layout_dir(
        &self,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Result<Option<PathBuf>, Diagnostic> {
        // Tier 1: Explicit config.layout_dir
        if let Some(ref layout_rel) = self.config.layout_dir {
            let p = Path::new(layout_rel);
            let full_path = if p.is_absolute() {
                p.to_path_buf()
            } else {
                self.workspace_root.join(p)
            };
            if !full_path.is_dir() {
                return Err(Diagnostic::new(
                    format!(
                        "Explicit layout directory '{}' does not exist or is not a directory",
                        full_path.display()
                    ),
                    IssueCode::new("invalid-config"),
                ));
            }
            return Ok(Some(full_path));
        }

        // Tier 2: Conventional <extensions_root>/layouts/<theme>
        let conv = self
            .extensions_root
            .join("layouts")
            .join(&self.config.theme);
        if conv.is_dir() {
            return Ok(Some(conv));
        }

        // Tier 3: Legacy .evin/layouts/<theme> with deprecation warning
        let legacy = self
            .workspace_root
            .join(".evin/layouts")
            .join(&self.config.theme);
        if legacy.is_dir() {
            let ext_rel = self
                .config
                .extensions_dir
                .as_deref()
                .unwrap_or(".evin/actions/docs");
            diagnostics.push(
                Diagnostic::new(
                    format!(
                        "Using legacy layout directory '.evin/layouts/{}'. Please migrate layout overrides to '{ext_rel}/layouts/{}'.",
                        self.config.theme, self.config.theme
                    ),
                    IssueCode::new("deprecated-layout-path"),
                )
                .with_severity(Severity::Warning),
            );
            return Ok(Some(legacy));
        }

        // Tier 4: Embedded
        Ok(None)
    }

    pub fn resolve_all(&self) -> Result<ResolvedResources, Diagnostic> {
        let mut diagnostics = Vec::new();
        let logo = self.resolve_logo(&mut diagnostics)?;
        let favicon = self.resolve_favicon(&mut diagnostics)?;
        let icons_dir = self.resolve_icons_dir()?;
        let layout_dir = self.resolve_layout_dir(&mut diagnostics)?;

        let read_custom = |names: &[&str]| {
            layout_dir.as_ref().and_then(|custom_path| {
                names
                    .iter()
                    .map(|name| custom_path.join(name))
                    .find(|path| path.is_file())
                    .and_then(|path| std::fs::read_to_string(path).ok())
            })
        };
        let embedded = |names: &[&str]| {
            names
                .iter()
                .find_map(|name| {
                    LAYOUTS_DIR
                        .get_file(format!("{}/{}", self.config.theme, name))
                        .or_else(|| LAYOUTS_DIR.get_file(format!("matter/{}", name)))
                        .and_then(|file| file.contents_utf8())
                        .map(str::to_string)
                })
                .unwrap_or_default()
        };
        let shell_html = read_custom(&["shell.html"]).unwrap_or_else(|| embedded(&["shell.html"]));
        let raw_css = read_custom(&["theme.scss", "theme.css"])
            .unwrap_or_else(|| embedded(&["theme.scss", "theme.css"]));
        let raw_search_js = read_custom(&["app.js", "search.js"])
            .unwrap_or_else(|| embedded(&["app.js", "search.js"]));
        let embedded_fs = EmbeddedFs::new(&LAYOUTS_DIR, &self.config.theme, layout_dir.clone());

        Ok(ResolvedResources {
            logo,
            favicon,
            icons_dir,
            layout_dir,
            shell_html,
            raw_css,
            raw_search_js,
            embedded_fs,
            diagnostics,
        })
    }
}

pub fn mime_for_extension(ext: &str) -> &'static str {
    match ext {
        "ico" => "image/x-icon",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "svg" => "image/svg+xml",
        _ => "image/svg+xml",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_resolver_omits_unconfigured_branding_without_warnings() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config = DocsConfig::default();
        let resolver = ResourceResolver::new(temp_dir.path(), &config).unwrap();
        let mut diagnostics = Vec::new();
        let logo = resolver.resolve_logo(&mut diagnostics).unwrap();
        assert_eq!(logo, ResolvedLogo::NeutralFallback);
        assert!(diagnostics.is_empty());

        let favicon = resolver.resolve_favicon(&mut diagnostics).unwrap();
        assert_eq!(favicon, ResolvedFavicon::NeutralFallback);
    }

    #[test]
    fn test_resource_resolver_uses_only_explicit_logo_configuration() {
        let temp_dir = tempfile::tempdir().unwrap();
        let ext_dir = temp_dir.path().join(".evin/actions/docs");
        std::fs::create_dir_all(&ext_dir).unwrap();
        std::fs::write(ext_dir.join("logo.svg"), "<svg id='conv'></svg>").unwrap();

        let explicit_logo = temp_dir.path().join("my-logo.svg");
        std::fs::write(&explicit_logo, "<svg id='explicit'></svg>").unwrap();

        let config = DocsConfig {
            appearance: crate::v1::AppearanceConfig {
                logo: Some(PathBuf::from("my-logo.svg")),
                ..Default::default()
            },
            ..DocsConfig::default()
        };

        let resolver = ResourceResolver::new(temp_dir.path(), &config).unwrap();
        let mut diagnostics = Vec::new();
        let logo = resolver.resolve_logo(&mut diagnostics).unwrap();
        assert_eq!(
            logo,
            ResolvedLogo::CustomSvg("<svg id='explicit'></svg>".to_string())
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_resource_resolver_does_not_infer_logo_from_repository_paths() {
        let temp_dir = tempfile::tempdir().unwrap();
        let ext_dir = temp_dir.path().join(".evin/actions/docs");
        std::fs::create_dir_all(&ext_dir).unwrap();
        std::fs::write(ext_dir.join("logo.svg"), "<svg id='implicit'></svg>").unwrap();

        let config = DocsConfig::default();
        let resolver = ResourceResolver::new(temp_dir.path(), &config).unwrap();
        let mut diagnostics = Vec::new();
        assert_eq!(
            resolver.resolve_logo(&mut diagnostics).unwrap(),
            ResolvedLogo::NeutralFallback
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_resource_resolver_missing_explicit_fails() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config = DocsConfig {
            appearance: crate::v1::AppearanceConfig {
                logo: Some(PathBuf::from("missing-logo.svg")),
                ..Default::default()
            },
            ..DocsConfig::default()
        };

        let resolver = ResourceResolver::new(temp_dir.path(), &config).unwrap();
        let mut diagnostics = Vec::new();
        let err = resolver.resolve_logo(&mut diagnostics).unwrap_err();
        assert_eq!(err.code.key, "invalid-config");
    }

    #[test]
    fn test_resource_resolver_does_not_infer_favicon_from_repository_paths() {
        let temp_dir = tempfile::tempdir().unwrap();
        let ext_dir = temp_dir.path().join(".evin/actions/docs");
        std::fs::create_dir_all(&ext_dir).unwrap();
        std::fs::write(ext_dir.join("favicon.svg"), "<svg></svg>").unwrap();
        let config = DocsConfig::default();
        let resolver = ResourceResolver::new(temp_dir.path(), &config).unwrap();
        assert_eq!(
            resolver.resolve_favicon(&mut Vec::new()).unwrap(),
            ResolvedFavicon::NeutralFallback
        );
    }
}
