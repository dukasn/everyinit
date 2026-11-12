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

//! Local documentation-image discovery and publishing.
//!
//! Images deliberately stay outside the document resolver: only files named by
//! a document are loaded, converted, and published. This keeps the generated
//! `assets/media` directory a managed, deterministic build artifact.

use crate::v1::ast::{DocNode, DocSiteAst, InlineNode, MediaKind};
use evin_doc::analysis::{Visitor, walk_nodes};
use image::ImageReader;
use ravif::{Encoder, Img, RGBA8};
use rayon::prelude::*;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

const PUBLISHED_MEDIA_DIR: &str = "assets/media";

#[derive(Debug, Clone)]
struct ResolvedAsset {
    output_relative: PathBuf,
    contents: Vec<u8>,
}

/// The resolved local-image registry for one documentation build.
#[derive(Debug, Clone, Default)]
pub struct ResolvedAssets {
    entries: BTreeMap<String, ResolvedAsset>,
}

impl ResolvedAssets {
    pub fn build(
        source_root: &Path,
        asset_dir: &str,
        site_ast: &DocSiteAst,
    ) -> Result<Self, evin_diagnostic::Diagnostic> {
        let asset_dir = asset_dir_path(asset_dir)?;
        let asset_prefix = format!("/{}/", path_to_url(&asset_dir));
        let references = image_references(site_ast)
            .into_iter()
            .filter(|source| source.starts_with(&asset_prefix))
            .collect::<BTreeSet<_>>();
        if references.is_empty() {
            return Ok(Self::default());
        }

        let source_root = canonicalize(source_root, "documentation source directory")?;
        let asset_root = canonicalize(
            &source_root.join(&asset_dir),
            "configured documentation asset directory",
        )?;
        if !asset_root.starts_with(&source_root) {
            return Err(invalid_asset("asset_dir must resolve inside source_dir"));
        }

        let sources = references
            .into_iter()
            .map(|reference| {
                let relative = local_reference_path(&reference, &asset_prefix)?;
                let source_path = source_path(&asset_root, &relative, &reference)?;
                let output_relative = output_path(&relative, &reference)?;
                Ok((reference, source_path, output_relative))
            })
            .collect::<Result<Vec<_>, evin_diagnostic::Diagnostic>>()?;

        let mut outputs = BTreeMap::new();
        for (reference, _, output) in &sources {
            if let Some(existing) = outputs.insert(output.clone(), reference) {
                return Err(invalid_asset(&format!(
                    "local image references '{}' and '{}' both publish to '{}'",
                    existing,
                    reference,
                    output.display()
                )));
            }
        }

        let generated = sources
            .par_iter()
            .map(|(reference, source_path, output_relative)| {
                generate_asset(source_path, reference).map(|contents| {
                    (
                        reference.clone(),
                        ResolvedAsset {
                            output_relative: output_relative.clone(),
                            contents,
                        },
                    )
                })
            })
            .collect::<Vec<Result<(String, ResolvedAsset), evin_diagnostic::Diagnostic>>>();
        let entries = generated
            .into_iter()
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        Ok(Self { entries })
    }

    pub fn is_registered_reference(&self, source: &str) -> bool {
        self.entries.contains_key(source)
    }

    /// Returns the generated asset paths referenced by this build.
    pub fn manifest_entries(&self) -> BTreeMap<String, String> {
        self.entries
            .iter()
            .map(|(source, asset)| (source.clone(), path_to_url(&asset.output_relative)))
            .collect()
    }

    pub fn is_root_relative_image(source: &str) -> bool {
        source.starts_with('/')
    }

    /// Resolves a document-local asset to its published URL without mutating the AST.
    pub fn render_url(&self, source: &str, page_slug: &str) -> Option<String> {
        self.url_for_page(source, page_slug)
    }

    pub fn write_to(&self, out_dir: &Path) -> Result<usize, evin_diagnostic::Diagnostic> {
        let mut assets = self.entries.values().collect::<Vec<_>>();
        assets.sort_by(|left, right| left.output_relative.cmp(&right.output_relative));
        for asset in &assets {
            let destination = out_dir.join(&asset.output_relative);
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent).map_err(|error| {
                    invalid_asset(&format!(
                        "failed to create generated media directory '{}': {error}",
                        parent.display()
                    ))
                })?;
            }
            std::fs::write(&destination, &asset.contents).map_err(|error| {
                invalid_asset(&format!(
                    "failed to publish local image '{}': {error}",
                    destination.display()
                ))
            })?;
        }
        self.prune_stale(out_dir)?;
        Ok(assets.len())
    }

    fn prune_stale(&self, out_dir: &Path) -> Result<(), evin_diagnostic::Diagnostic> {
        let media_root = out_dir.join(PUBLISHED_MEDIA_DIR);
        if !media_root.exists() {
            return Ok(());
        }
        let expected = self
            .entries
            .values()
            .map(|asset| out_dir.join(&asset.output_relative))
            .collect::<BTreeSet<_>>();
        let mut files = Vec::new();
        collect_files(&media_root, &mut files)?;
        for file in files {
            if !expected.contains(&file) {
                std::fs::remove_file(&file).map_err(|error| {
                    invalid_asset(&format!(
                        "failed to prune stale generated image '{}': {error}",
                        file.display()
                    ))
                })?;
            }
        }
        prune_empty_dirs(&media_root)?;
        Ok(())
    }

    fn url_for_page(&self, source: &str, slug: &str) -> Option<String> {
        let output = &self.entries.get(source)?.output_relative;
        let depth = if slug == "index" {
            0
        } else {
            slug.split('/').count()
        };
        let prefix = if depth == 0 {
            "./".to_string()
        } else {
            "../".repeat(depth)
        };
        Some(format!("{}{}", prefix, path_to_url(output)))
    }
}

pub fn validate_asset_dir(asset_dir: &str) -> Result<(), String> {
    asset_dir_path(asset_dir)
        .map(|_| ())
        .map_err(|error| error.title)
}

pub fn fingerprint_files(source_root: &Path, asset_dir: &str) -> Vec<PathBuf> {
    let Ok(asset_dir) = asset_dir_path(asset_dir) else {
        return Vec::new();
    };
    let root = source_root.join(asset_dir);
    let mut files = Vec::new();
    if collect_files(&root, &mut files).is_err() {
        return Vec::new();
    }
    files.sort();
    files
}

fn image_references(site_ast: &DocSiteAst) -> BTreeSet<String> {
    let mut references = BTreeSet::new();
    for page in &site_ast.pages {
        walk_nodes(
            &page.ast,
            &mut AssetReferenceCollector {
                references: &mut references,
            },
        );
    }
    references
}

struct AssetReferenceCollector<'a> {
    references: &'a mut BTreeSet<String>,
}

impl Visitor for AssetReferenceCollector<'_> {
    fn visit_node(&mut self, node: &DocNode) {
        if let DocNode::Media {
            kind: MediaKind::Image,
            src,
            ..
        } = node
        {
            self.references.insert(src.clone());
        }
    }

    fn visit_inline(&mut self, inline: &InlineNode) {
        if let InlineNode::Image { src, .. } = inline {
            self.references.insert(src.clone());
        }
    }
}

fn asset_dir_path(asset_dir: &str) -> Result<PathBuf, evin_diagnostic::Diagnostic> {
    let path = Path::new(asset_dir.trim());
    if asset_dir.trim().is_empty() || path.is_absolute() {
        return Err(invalid_asset(
            "asset_dir must be a non-empty path relative to source_dir without '..'",
        ));
    }
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => normalized.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(invalid_asset(
                    "asset_dir must be a non-empty path relative to source_dir without '..'",
                ));
            }
        }
    }
    if normalized.as_os_str().is_empty() {
        return Err(invalid_asset(
            "asset_dir must be a non-empty path relative to source_dir without '..'",
        ));
    }
    Ok(normalized)
}

fn local_reference_path(
    reference: &str,
    prefix: &str,
) -> Result<PathBuf, evin_diagnostic::Diagnostic> {
    let suffix = reference.strip_prefix(prefix).unwrap_or_default();
    if suffix.is_empty()
        || suffix.split('/').any(|part| {
            part.is_empty()
                || part == "."
                || part == ".."
                || part.contains('?')
                || part.contains('#')
        })
    {
        return Err(invalid_asset(&format!(
            "local image reference '{}' escapes or is outside the asset directory",
            reference
        )));
    }
    Ok(PathBuf::from(suffix))
}

fn source_path(
    asset_root: &Path,
    relative: &Path,
    reference: &str,
) -> Result<PathBuf, evin_diagnostic::Diagnostic> {
    let path = asset_root.join(relative);
    let canonical = canonicalize(&path, &format!("local image '{reference}'"))?;
    if !canonical.starts_with(asset_root) || !canonical.is_file() {
        return Err(invalid_asset(&format!(
            "local image reference '{}' must name a file inside the asset directory",
            reference
        )));
    }
    Ok(canonical)
}

fn output_path(relative: &Path, reference: &str) -> Result<PathBuf, evin_diagnostic::Diagnostic> {
    let extension = relative
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| {
            invalid_asset(&format!(
                "local image '{}' has no supported extension",
                reference
            ))
        })?;
    let mut output = PathBuf::from(PUBLISHED_MEDIA_DIR).join(relative);
    match extension.as_str() {
        "png" | "jpg" | "jpeg" => {
            output.set_extension("avif");
        }
        "avif" | "svg" => {}
        _ => {
            return Err(invalid_asset(&format!(
                "local image '{}' uses unsupported extension '.{}' (use PNG, JPEG, AVIF, or SVG)",
                reference, extension
            )));
        }
    }
    Ok(output)
}

fn generate_asset(
    source_path: &Path,
    reference: &str,
) -> Result<Vec<u8>, evin_diagnostic::Diagnostic> {
    let extension = source_path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    match extension.as_str() {
        "svg" | "avif" => std::fs::read(source_path).map_err(|error| {
            invalid_asset(&format!(
                "failed to read local image '{}': {error}",
                reference
            ))
        }),
        "png" | "jpg" | "jpeg" => {
            let image = ImageReader::open(source_path)
                .map_err(|error| {
                    invalid_asset(&format!(
                        "failed to read local image '{}': {error}",
                        reference
                    ))
                })?
                .decode()
                .map_err(|error| {
                    invalid_asset(&format!("invalid raster image '{}': {error}", reference))
                })?
                .to_rgba8();
            let pixels = image
                .as_raw()
                .as_chunks::<4>()
                .0
                .iter()
                .map(|pixel| RGBA8::new(pixel[0], pixel[1], pixel[2], pixel[3]))
                .collect::<Vec<_>>();
            Encoder::new()
                .with_quality(80.0)
                .encode_rgba(Img::new(
                    &pixels,
                    image.width() as usize,
                    image.height() as usize,
                ))
                .map(|encoded| encoded.avif_file)
                .map_err(|error| {
                    invalid_asset(&format!(
                        "failed to encode local image '{}' as AVIF: {error}",
                        reference
                    ))
                })
        }
        _ => unreachable!("unsupported extensions are rejected before publishing"),
    }
}

fn canonicalize(path: &Path, description: &str) -> Result<PathBuf, evin_diagnostic::Diagnostic> {
    std::fs::canonicalize(path).map_err(|error| {
        invalid_asset(&format!(
            "failed to resolve {description} '{}': {error}",
            path.display()
        ))
    })
}

fn collect_files(root: &Path, files: &mut Vec<PathBuf>) -> Result<(), evin_diagnostic::Diagnostic> {
    if !root.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(root).map_err(|error| {
        invalid_asset(&format!(
            "failed to read asset directory '{}': {error}",
            root.display()
        ))
    })? {
        let entry =
            entry.map_err(|error| invalid_asset(&format!("failed to inspect asset: {error}")))?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, files)?;
        } else if path.is_file() {
            files.push(path);
        }
    }
    Ok(())
}

fn prune_empty_dirs(root: &Path) -> Result<(), evin_diagnostic::Diagnostic> {
    if !root.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(root).map_err(|error| {
        invalid_asset(&format!(
            "failed to read generated media '{}': {error}",
            root.display()
        ))
    })? {
        let entry = entry.map_err(|error| {
            invalid_asset(&format!("failed to inspect generated media: {error}"))
        })?;
        let path = entry.path();
        if path.is_dir() {
            prune_empty_dirs(&path)?;
            let _ = std::fs::remove_dir(&path);
        }
    }
    let _ = std::fs::remove_dir(root);
    Ok(())
}

fn path_to_url(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn invalid_asset(message: &str) -> evin_diagnostic::Diagnostic {
    evin_diagnostic::Diagnostic::new(message, evin_diagnostic::IssueCode::new("invalid-asset"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v1::ast::{DocFormat, DocPage};
    use std::collections::HashMap;
    use std::io::Cursor;

    fn site(images: &[&str]) -> DocSiteAst {
        DocSiteAst {
            version: "1.0".to_string(),
            title: None,
            reference_prefix: "REF".to_string(),
            pages: vec![DocPage {
                slug: "guides/nested".to_string(),
                source_path: "guides/nested.rst".to_string(),
                format: DocFormat::ReStructuredText,
                metadata: HashMap::new(),
                reference: None,
                toc: vec![],
                ast: images
                    .iter()
                    .map(|src| DocNode::Paragraph {
                        children: vec![InlineNode::Image {
                            src: (*src).to_string(),
                            alt: "A test image".to_string(),
                            title: None,
                        }],
                    })
                    .collect(),
            }],
        }
    }

    fn write_png(path: &Path) {
        let image = image::RgbaImage::from_pixel(2, 2, image::Rgba([40, 90, 160, 255]));
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes.into_inner()).unwrap();
    }

    #[test]
    fn converts_referenced_rasters_and_resolves_nested_urls_without_mutation() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("docs");
        write_png(&source.join("assets/diagrams/map.png"));
        let assets =
            ResolvedAssets::build(&source, "assets", &site(&["/assets/diagrams/map.png"])).unwrap();

        assert_eq!(
            assets.render_url("/assets/diagrams/map.png", "guides/nested"),
            Some("../../assets/media/diagrams/map.avif".to_string())
        );

        let out = temp.path().join("dist");
        assert_eq!(assets.write_to(&out).unwrap(), 1);
        let published = std::fs::read(out.join("assets/media/diagrams/map.avif")).unwrap();
        assert!(published.windows(4).any(|window| window == b"avif"));
    }

    #[test]
    fn passes_through_svg_and_avif_and_deduplicates_references() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("docs");
        let svg = b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>";
        let svg_path = source.join("assets/brand/logo.svg");
        std::fs::create_dir_all(svg_path.parent().unwrap()).unwrap();
        std::fs::write(&svg_path, svg).unwrap();
        let avif = b"already encoded AVIF";
        std::fs::write(source.join("assets/brand/logo.avif"), avif).unwrap();
        let assets = ResolvedAssets::build(
            &source,
            "assets",
            &site(&[
                "/assets/brand/logo.svg",
                "/assets/brand/logo.svg",
                "/assets/brand/logo.avif",
            ]),
        )
        .unwrap();

        let out = temp.path().join("dist");
        let stale = out.join("assets/media/obsolete/old.avif");
        std::fs::create_dir_all(stale.parent().unwrap()).unwrap();
        std::fs::write(&stale, "stale").unwrap();
        assert_eq!(assets.write_to(&out).unwrap(), 2);
        assert!(!stale.exists());
        assert_eq!(
            std::fs::read(out.join("assets/media/brand/logo.svg")).unwrap(),
            svg
        );
        assert_eq!(
            std::fs::read(out.join("assets/media/brand/logo.avif")).unwrap(),
            avif
        );
    }

    #[test]
    fn rejects_traversal_and_output_collisions() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("docs");
        write_png(&source.join("assets/diagram.png"));
        let traversal =
            ResolvedAssets::build(&source, "assets", &site(&["/assets/../diagram.png"]));
        assert!(traversal.unwrap_err().title.contains("escapes"));

        write_png(&source.join("assets/diagram.jpg"));
        let collision = ResolvedAssets::build(
            &source,
            "assets",
            &site(&["/assets/diagram.png", "/assets/diagram.jpg"]),
        );
        assert!(collision.unwrap_err().title.contains("both publish"));
    }
}
