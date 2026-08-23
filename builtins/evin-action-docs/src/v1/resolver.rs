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
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Result<ResolvedLogo, Diagnostic> {
        // Tier 1: Explicit config.logo
        if let Some(ref logo_rel) = self.config.logo {
            let p = Path::new(logo_rel);
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

        // Tier 2: Conventional <extensions_root>/logo.svg or <extensions_root>/icons/logo.svg
        let conv1 = self.extensions_root.join("logo.svg");
        if conv1.is_file() {
            let svg = std::fs::read_to_string(&conv1).map_err(|err| {
                Diagnostic::new(
                    format!(
                        "Failed to read conventional logo file '{}': {err}",
                        conv1.display()
                    ),
                    IssueCode::new("io-error"),
                )
            })?;
            return Ok(ResolvedLogo::CustomSvg(svg));
        }

        let conv2 = self.extensions_root.join("icons/logo.svg");
        if conv2.is_file() {
            let svg = std::fs::read_to_string(&conv2).map_err(|err| {
                Diagnostic::new(
                    format!(
                        "Failed to read conventional logo file '{}': {err}",
                        conv2.display()
                    ),
                    IssueCode::new("io-error"),
                )
            })?;
            return Ok(ResolvedLogo::CustomSvg(svg));
        }

        // Tier 3 & 4: Neutral fallback with build warning
        diagnostics.push(
            Diagnostic::new(
                "Documentation build using neutral fallback brand symbol (🕮). Provide a brand logo via 'logo' configuration or place 'logo.svg' in '.evin/actions/docs/' (or custom 'extensions_dir').",
                IssueCode::new("neutral-fallback"),
            )
            .with_severity(Severity::Warning),
        );
        Ok(ResolvedLogo::NeutralFallback)
    }

    pub fn resolve_favicon(
        &self,
        _diagnostics: &mut Vec<Diagnostic>,
    ) -> Result<ResolvedFavicon, Diagnostic> {
        // Tier 1: Explicit config.favicon
        if let Some(ref fav_rel) = self.config.favicon {
            let p = Path::new(fav_rel);
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

        // Tier 2: Conventional <extensions_root>/favicon.{svg,png,ico}
        if self.extensions_root.is_dir()
            && let Ok(entries) = std::fs::read_dir(&self.extensions_root)
        {
            let mut candidates = Vec::new();
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file()
                    && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
                    && stem.eq_ignore_ascii_case("favicon")
                    && let Some(ext) = path.extension().and_then(|e| e.to_str())
                {
                    let ext_lower = ext.to_lowercase();
                    if matches!(ext_lower.as_str(), "svg" | "png" | "ico") {
                        candidates.push(path);
                    }
                }
            }

            if candidates.len() > 1 {
                let names: Vec<String> = candidates
                    .iter()
                    .map(|p| {
                        p.file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string()
                    })
                    .collect();
                return Err(Diagnostic::new(
                    format!(
                        "Multiple conventional favicon candidates found in '{}': [{}]. Specify explicit 'favicon' in configuration or remove ambiguous files.",
                        self.extensions_root.display(),
                        names.join(", ")
                    ),
                    IssueCode::new("ambiguous-favicon"),
                ));
            }

            if candidates.len() == 1 {
                let full_path = &candidates[0];
                let ext = full_path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("svg")
                    .to_lowercase();
                let mime = mime_for_extension(&ext);
                let bytes = std::fs::read(full_path).map_err(|err| {
                    Diagnostic::new(
                        format!(
                            "Failed to read conventional favicon file '{}': {err}",
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

        // Tier 2: Conventional <extensions_root>/icons
        let conv = self.extensions_root.join("icons");
        if conv.is_dir() {
            return Ok(Some(conv));
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
    fn test_resource_resolver_defaults_to_neutral() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config = DocsConfig::default();
        let resolver = ResourceResolver::new(temp_dir.path(), &config).unwrap();
        let mut diagnostics = Vec::new();
        let logo = resolver.resolve_logo(&mut diagnostics).unwrap();
        assert_eq!(logo, ResolvedLogo::NeutralFallback);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code.key, "neutral-fallback");

        let favicon = resolver.resolve_favicon(&mut diagnostics).unwrap();
        assert_eq!(favicon, ResolvedFavicon::NeutralFallback);
    }

    #[test]
    fn test_resource_resolver_precedence_explicit_over_conventional() {
        let temp_dir = tempfile::tempdir().unwrap();
        let ext_dir = temp_dir.path().join(".evin/actions/docs");
        std::fs::create_dir_all(&ext_dir).unwrap();
        std::fs::write(ext_dir.join("logo.svg"), "<svg id='conv'></svg>").unwrap();

        let explicit_logo = temp_dir.path().join("my-logo.svg");
        std::fs::write(&explicit_logo, "<svg id='explicit'></svg>").unwrap();

        let mut config = DocsConfig::default();
        config.logo = Some("my-logo.svg".to_string());

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
    fn test_resource_resolver_conventional_logo() {
        let temp_dir = tempfile::tempdir().unwrap();
        let ext_dir = temp_dir.path().join(".evin/actions/docs");
        std::fs::create_dir_all(&ext_dir).unwrap();
        std::fs::write(ext_dir.join("logo.svg"), "<svg id='conv'></svg>").unwrap();

        let config = DocsConfig::default();
        let resolver = ResourceResolver::new(temp_dir.path(), &config).unwrap();
        let mut diagnostics = Vec::new();
        let logo = resolver.resolve_logo(&mut diagnostics).unwrap();
        assert_eq!(
            logo,
            ResolvedLogo::CustomSvg("<svg id='conv'></svg>".to_string())
        );
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_resource_resolver_missing_explicit_fails() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut config = DocsConfig::default();
        config.logo = Some("missing-logo.svg".to_string());

        let resolver = ResourceResolver::new(temp_dir.path(), &config).unwrap();
        let mut diagnostics = Vec::new();
        let err = resolver.resolve_logo(&mut diagnostics).unwrap_err();
        assert_eq!(err.code.key, "invalid-config");
    }

    #[test]
    fn test_resource_resolver_ambiguous_favicon() {
        let temp_dir = tempfile::tempdir().unwrap();
        let ext_dir = temp_dir.path().join(".evin/actions/docs");
        std::fs::create_dir_all(&ext_dir).unwrap();
        std::fs::write(ext_dir.join("favicon.svg"), "<svg></svg>").unwrap();
        std::fs::write(ext_dir.join("favicon.png"), b"fake png").unwrap();

        let config = DocsConfig::default();
        let resolver = ResourceResolver::new(temp_dir.path(), &config).unwrap();
        let mut diagnostics = Vec::new();
        let err = resolver.resolve_favicon(&mut diagnostics).unwrap_err();
        assert_eq!(err.code.key, "ambiguous-favicon");
    }
}
