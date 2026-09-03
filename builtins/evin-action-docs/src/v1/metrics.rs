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

//! Documentation site compression analytics, Frame-0 budget gate, and search engine inspection.

use crate::v1::DocsConfig;
use chrono::Utc;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BudgetStatus {
    Pass,
    Fail,
    NotConfigured,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Frame0Metrics {
    pub files: Vec<String>,
    pub raw_bytes: u64,
    pub gzip_bytes: u64,
    pub brotli_bytes: u64,
    pub budget_limit_bytes: Option<u64>,
    pub budget_used_pct: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CatalogMetrics {
    pub document_count: u32,
    pub prompt_count: u32,
    pub string_pool_bytes: u32,
    pub raw_bytes: u64,
    pub gzip_bytes: u64,
    pub brotli_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IndexMetrics {
    pub vocabulary_count: u32,
    pub bucket_count: u32,
    pub posting_count: u64,
    pub string_pool_bytes: u32,
    pub elias_fano_low_bytes: u64,
    pub elias_fano_high_bytes: u64,
    pub bits_per_posting: f64,
    pub raw_bytes: u64,
    pub gzip_bytes: u64,
    pub brotli_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchEngineMetrics {
    pub catalog: Option<CatalogMetrics>,
    pub index: Option<IndexMetrics>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IconTreeShakingMetrics {
    pub total_symbols: usize,
    pub used_symbols: usize,
    pub unused_symbols: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FontTreeShakingMetrics {
    pub total_fonts: usize,
    pub preloaded_fonts: usize,
    pub active_faces: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TreeShakingMetrics {
    pub icons: IconTreeShakingMetrics,
    pub fonts: FontTreeShakingMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AssetMetric {
    pub path: String,
    pub mime: String,
    pub raw_bytes: u64,
    pub gzip_bytes: u64,
    pub brotli_bytes: u64,
    pub gzip_savings_pct: f64,
    pub brotli_savings_pct: f64,
    pub frame0: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TotalsMetrics {
    pub file_count: usize,
    pub raw_bytes: u64,
    pub gzip_bytes: u64,
    pub brotli_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricsReport {
    pub generated_at: String,
    pub budget_kb: Option<u32>,
    pub budget_status: BudgetStatus,
    pub frame0: Frame0Metrics,
    pub search: Option<SearchEngineMetrics>,
    pub tree_shaking: TreeShakingMetrics,
    pub assets: Vec<AssetMetric>,
    pub totals: TotalsMetrics,
}

/// Compress arbitrary payload using canonical Gzip at level 9 (maximum standard compression).
pub fn compress_gzip(data: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::new(9));
    encoder.write_all(data)?;
    encoder.finish()
}

/// Compress arbitrary payload using canonical Brotli at quality 11 (maximum compression, window 22).
pub fn compress_brotli(data: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut writer = brotli::CompressorWriter::new(&mut out, 4096, 11, 22);
    writer.write_all(data)?;
    drop(writer);
    Ok(out)
}

/// Resolve MIME content type from file path extension.
pub fn resolve_mime_type(path: &Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()).unwrap_or("") {
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "js" => "application/javascript",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "bin" => "application/octet-stream",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "txt" => "text/plain",
        "xml" => "application/xml",
        "webmanifest" => "application/manifest+json",
        _ => "application/octet-stream",
    }
}

/// Recursively collect all file paths relative to `root`, deterministically sorted,
/// strictly excluding `metrics.json`.
pub fn collect_output_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_recursive(root, root, &mut files);
    files.sort();
    files
}

fn collect_recursive(base: &Path, current: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(current) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_recursive(base, &path, files);
        } else if path.is_file()
            && let Ok(rel) = path.strip_prefix(base)
        {
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            if rel_str != "metrics.json" {
                files.push(rel.to_path_buf());
            }
        }
    }
}

/// Identify the Frame-0 landing critical path assets from the root `index.html`.
pub fn identify_frame0_assets(out_dir: &Path, root_html_content: Option<&str>) -> HashSet<String> {
    let mut frame0 = HashSet::new();
    let index_file = out_dir.join("index.html");
    if !index_file.is_file() {
        return frame0;
    }
    frame0.insert("index.html".to_string());

    let content_storage;
    let content = match root_html_content {
        Some(s) => s,
        None => match std::fs::read_to_string(&index_file) {
            Ok(s) => {
                content_storage = s;
                &content_storage
            }
            Err(_) => return frame0,
        },
    };

    // Extract stylesheets: <link ... href="..." ... rel="stylesheet"> or rel="preload" as="style"
    let link_css_re = Regex::new(
        r#"(?i)<link\s+[^>]*href=["']([^"']+\.css(?:\?[^"']*)?)["'][^>]*rel=["'](?:preload|stylesheet)["']|<link\s+[^>]*rel=["'](?:preload|stylesheet)["'][^>]*href=["']([^"']+\.css(?:\?[^"']*)?)["']"#,
    ).unwrap();
    for cap in link_css_re.captures_iter(content) {
        let matched = cap
            .get(1)
            .or_else(|| cap.get(2))
            .map(|m| m.as_str())
            .unwrap_or("");
        let clean = clean_relative_path(matched);
        if out_dir.join(&clean).is_file() {
            frame0.insert(clean);
        }
    }

    // Extract scripts: <script ... src="...">
    let script_re =
        Regex::new(r#"(?i)<script\s+[^>]*src=["']([^"']+\.js(?:\?[^"']*)?)["']"#).unwrap();
    for cap in script_re.captures_iter(content) {
        let matched = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let clean = clean_relative_path(matched);
        if out_dir.join(&clean).is_file() {
            frame0.insert(clean);
        }
    }

    // Extract preloaded fonts: <link ... href="...woff2" ... as="font">
    let font_re = Regex::new(
        r#"(?i)<link\s+[^>]*href=["']([^"']+\.woff2?(?:\?[^"']*)?)["'][^>]*as=["']font["']|<link\s+[^>]*as=["']font["'][^>]*href=["']([^"']+\.woff2?(?:\?[^"']*)?)["']"#,
    ).unwrap();
    for cap in font_re.captures_iter(content) {
        let matched = cap
            .get(1)
            .or_else(|| cap.get(2))
            .map(|m| m.as_str())
            .unwrap_or("");
        let clean = clean_relative_path(matched);
        if out_dir.join(&clean).is_file() {
            frame0.insert(clean);
        }
    }

    // Extract favicon
    let icon_re = Regex::new(
        r#"(?i)<link\s+[^>]*rel=["'](?:shortcut )?icon["'][^>]*href=["']([^"']+)["']|<link\s+[^>]*href=["']([^"']+)["'][^>]*rel=["'](?:shortcut )?icon["']"#,
    ).unwrap();
    for cap in icon_re.captures_iter(content) {
        let matched = cap
            .get(1)
            .or_else(|| cap.get(2))
            .map(|m| m.as_str())
            .unwrap_or("");
        let clean = clean_relative_path(matched);
        if out_dir.join(&clean).is_file() {
            frame0.insert(clean);
        }
    }

    frame0
}

fn clean_relative_path(raw: &str) -> String {
    let stripped = raw
        .split('?')
        .next()
        .unwrap_or(raw)
        .split('#')
        .next()
        .unwrap_or(raw);
    stripped
        .trim_start_matches('/')
        .trim_start_matches("./")
        .replace('\\', "/")
}

/// Decode EVCT (search-catalog.bin) header and sections.
pub fn decode_catalog(bytes: &[u8]) -> Result<CatalogMetrics, String> {
    if bytes.len() < 32 {
        return Err(format!(
            "Catalog file is {} bytes, less than 32-byte header",
            bytes.len()
        ));
    }
    if &bytes[0..4] != b"EVCT" {
        return Err(format!("Invalid catalog magic bytes: {:?}", &bytes[0..4]));
    }
    let doc_count = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
    let prompt_count = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
    let strings_offset = u32::from_le_bytes(bytes[24..28].try_into().unwrap()) as usize;
    let string_pool_bytes = bytes.len().saturating_sub(strings_offset) as u32;

    let raw_bytes = bytes.len() as u64;
    let gzip_bytes = compress_gzip(bytes)
        .map(|b| b.len() as u64)
        .unwrap_or(raw_bytes);
    let brotli_bytes = compress_brotli(bytes)
        .map(|b| b.len() as u64)
        .unwrap_or(raw_bytes);

    Ok(CatalogMetrics {
        document_count: doc_count,
        prompt_count,
        string_pool_bytes,
        raw_bytes,
        gzip_bytes,
        brotli_bytes,
    })
}

/// Decode EVIX (search-index.bin) header, MPHF section, and Elias-Fano postings.
pub fn decode_index(bytes: &[u8]) -> Result<IndexMetrics, String> {
    if bytes.len() < 32 {
        return Err(format!(
            "Index file is {} bytes, less than 32-byte header",
            bytes.len()
        ));
    }
    if &bytes[0..4] != b"EVIX" {
        return Err(format!("Invalid index magic bytes: {:?}", &bytes[0..4]));
    }

    let term_count = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
    let mphf_offset = u32::from_le_bytes(bytes[16..20].try_into().unwrap()) as usize;
    let postings_offset = u32::from_le_bytes(bytes[20..24].try_into().unwrap()) as usize;
    let strings_offset = u32::from_le_bytes(bytes[24..28].try_into().unwrap()) as usize;

    if mphf_offset > bytes.len() || postings_offset > bytes.len() || strings_offset > bytes.len() {
        return Err("Header section offsets exceed file boundaries".to_string());
    }

    let bucket_count = if mphf_offset + 4 <= bytes.len() {
        u32::from_le_bytes(bytes[mphf_offset..mphf_offset + 4].try_into().unwrap())
    } else {
        0
    };

    let string_pool_bytes = bytes.len().saturating_sub(strings_offset) as u32;

    // Scan Elias-Fano postings section bounded by strings_offset
    let mut posting_count = 0u64;
    let mut elias_fano_low_bytes = 0u64;
    let mut elias_fano_high_bytes = 0u64;

    let mut pos = postings_offset;
    for _ in 0..term_count {
        if pos + 2 > strings_offset || pos + 2 > bytes.len() {
            break;
        }
        let count = u16::from_le_bytes(bytes[pos..pos + 2].try_into().unwrap()) as usize;
        if count == 0 {
            pos += 2;
            pos = (pos + 3) & !3; // 4-byte align
            continue;
        }
        if pos + 6 > strings_offset || pos + 6 > bytes.len() {
            break;
        }
        let low_bits_len = bytes[pos + 3] as usize;
        let high_bits_len =
            u16::from_le_bytes(bytes[pos + 4..pos + 6].try_into().unwrap()) as usize;
        let weights_len = count;
        let entry_len = 6 + low_bits_len + high_bits_len + weights_len;

        if pos + entry_len > strings_offset || pos + entry_len > bytes.len() {
            break;
        }

        posting_count += count as u64;
        elias_fano_low_bytes += low_bits_len as u64;
        elias_fano_high_bytes += high_bits_len as u64;

        pos += entry_len;
        pos = (pos + 3) & !3; // 4-byte align
    }

    let total_compressed_bits = (elias_fano_low_bytes + elias_fano_high_bytes) * 8;
    let bits_per_posting = if posting_count > 0 {
        total_compressed_bits as f64 / posting_count as f64
    } else {
        0.0
    };

    let raw_bytes = bytes.len() as u64;
    let gzip_bytes = compress_gzip(bytes)
        .map(|b| b.len() as u64)
        .unwrap_or(raw_bytes);
    let brotli_bytes = compress_brotli(bytes)
        .map(|b| b.len() as u64)
        .unwrap_or(raw_bytes);

    Ok(IndexMetrics {
        vocabulary_count: term_count,
        bucket_count,
        posting_count,
        string_pool_bytes,
        elias_fano_low_bytes,
        elias_fano_high_bytes,
        bits_per_posting,
        raw_bytes,
        gzip_bytes,
        brotli_bytes,
    })
}

/// Analyze SVG icon sprite symbol references and CSS font-face declarations.
pub fn analyze_tree_shaking(out_dir: &Path) -> TreeShakingMetrics {
    let mut defined_symbols = BTreeSet::new();
    let sprite_path = out_dir.join("assets/icons.svg");
    if let Ok(sprite_svg) = std::fs::read_to_string(&sprite_path) {
        let symbol_re = Regex::new(r#"<symbol\s+[^>]*id=["']([^"']+)["']"#).unwrap();
        for cap in symbol_re.captures_iter(&sprite_svg) {
            if let Some(id) = cap.get(1) {
                defined_symbols.insert(id.as_str().to_string());
            }
        }
    }

    let mut referenced_symbols = HashSet::new();
    let use_re = Regex::new(r#"#(icon-[a-zA-Z0-9_-]+)"#).unwrap();

    let mut active_faces = 0;
    let font_face_re = Regex::new(r#"@font-face\s*\{"#).unwrap();

    let mut preloaded_fonts = 0;
    let preload_re = Regex::new(r#"<link\s+[^>]*as=["']font["']|<link\s+[^>]*href=["'][^"']+\.woff2?["'][^>]*as=["']font["']"#).unwrap();

    // Traverse HTML, CSS, JS files to inspect references
    let files = collect_output_files(out_dir);
    for rel_path in &files {
        let ext = rel_path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let full = out_dir.join(rel_path);
        if matches!(ext, "html" | "js" | "css")
            && let Ok(content) = std::fs::read_to_string(&full)
        {
            for cap in use_re.captures_iter(&content) {
                if let Some(sym) = cap.get(1) {
                    referenced_symbols.insert(sym.as_str().to_string());
                }
            }
            if ext == "css" {
                active_faces += font_face_re.find_iter(&content).count();
            }
            if rel_path.to_string_lossy() == "index.html" {
                preloaded_fonts += preload_re.find_iter(&content).count();
            }
        }
    }

    let total_symbols = defined_symbols.len();
    let used_symbols = defined_symbols
        .iter()
        .filter(|sym| referenced_symbols.contains(sym.as_str()))
        .count();
    let unused_symbols: Vec<String> = defined_symbols
        .iter()
        .filter(|sym| !referenced_symbols.contains(sym.as_str()))
        .cloned()
        .collect();

    let total_fonts = files
        .iter()
        .filter(|p| {
            p.starts_with("assets/fonts")
                && matches!(
                    p.extension().and_then(|e| e.to_str()).unwrap_or(""),
                    "woff2" | "woff" | "ttf"
                )
        })
        .count();

    TreeShakingMetrics {
        icons: IconTreeShakingMetrics {
            total_symbols,
            used_symbols,
            unused_symbols,
        },
        fonts: FontTreeShakingMetrics {
            total_fonts,
            preloaded_fonts,
            active_faces,
        },
    }
}

/// Generate the comprehensive metrics report for the built documentation site.
pub fn generate_report(out_dir: &Path, config: &DocsConfig) -> Result<MetricsReport, String> {
    let files = collect_output_files(out_dir);
    let frame0_set = identify_frame0_assets(out_dir, None);

    use rayon::prelude::*;

    let mut assets: Vec<AssetMetric> = files
        .par_iter()
        .filter_map(|rel| {
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            let full_path = out_dir.join(rel);
            let bytes = std::fs::read(&full_path).ok()?;

            let raw_len = bytes.len() as u64;
            let gz_len = compress_gzip(&bytes)
                .map(|b| b.len() as u64)
                .unwrap_or(raw_len);
            let br_len = compress_brotli(&bytes)
                .map(|b| b.len() as u64)
                .unwrap_or(raw_len);

            let gz_savings = if raw_len > 0 {
                (1.0 - (gz_len as f64 / raw_len as f64)) * 100.0
            } else {
                0.0
            };
            let br_savings = if raw_len > 0 {
                (1.0 - (br_len as f64 / raw_len as f64)) * 100.0
            } else {
                0.0
            };

            let is_frame0 = frame0_set.contains(&rel_str);
            Some(AssetMetric {
                path: rel_str,
                mime: resolve_mime_type(&full_path).to_string(),
                raw_bytes: raw_len,
                gzip_bytes: gz_len,
                brotli_bytes: br_len,
                gzip_savings_pct: (gz_savings * 10.0).round() / 10.0,
                brotli_savings_pct: (br_savings * 10.0).round() / 10.0,
                frame0: is_frame0,
            })
        })
        .collect();

    // Preserve deterministic alphabetical sort order
    assets.sort_by(|a, b| a.path.cmp(&b.path));

    let mut total_raw = 0u64;
    let mut total_gzip = 0u64;
    let mut total_brotli = 0u64;
    let mut frame0_raw = 0u64;
    let mut frame0_gzip = 0u64;
    let mut frame0_brotli = 0u64;
    let mut frame0_files = Vec::new();

    for asset in &assets {
        total_raw += asset.raw_bytes;
        total_gzip += asset.gzip_bytes;
        total_brotli += asset.brotli_bytes;

        if asset.frame0 {
            frame0_raw += asset.raw_bytes;
            frame0_gzip += asset.gzip_bytes;
            frame0_brotli += asset.brotli_bytes;
            frame0_files.push(asset.path.clone());
        }
    }

    frame0_files.sort();

    // Budget determination
    let budget_kb = config.metrics_budget_kb;
    let budget_limit_bytes = budget_kb.map(|kb| (kb as u64) * 1024);
    let (budget_status, budget_used_pct) = match budget_limit_bytes {
        Some(limit) if limit > 0 => {
            let used_pct = (frame0_gzip as f64 / limit as f64) * 100.0;
            let status = if frame0_gzip <= limit {
                BudgetStatus::Pass
            } else {
                BudgetStatus::Fail
            };
            (status, Some((used_pct * 10.0).round() / 10.0))
        }
        _ => (BudgetStatus::NotConfigured, None),
    };

    let frame0 = Frame0Metrics {
        files: frame0_files,
        raw_bytes: frame0_raw,
        gzip_bytes: frame0_gzip,
        brotli_bytes: frame0_brotli,
        budget_limit_bytes,
        budget_used_pct,
    };

    // Search diagnostics and binary metrics
    let mut search_diagnostics = Vec::new();
    let catalog_path = out_dir.join("assets/search-catalog.bin");
    let catalog = if catalog_path.is_file() {
        match std::fs::read(&catalog_path) {
            Ok(bytes) => match decode_catalog(&bytes) {
                Ok(cat) => Some(cat),
                Err(err) => {
                    search_diagnostics.push(err);
                    None
                }
            },
            Err(e) => {
                search_diagnostics.push(format!("Could not read search-catalog.bin: {e}"));
                None
            }
        }
    } else {
        search_diagnostics.push("assets/search-catalog.bin not found".to_string());
        None
    };

    let index_path = out_dir.join("assets/search-index.bin");
    let index = if index_path.is_file() {
        match std::fs::read(&index_path) {
            Ok(bytes) => match decode_index(&bytes) {
                Ok(idx) => Some(idx),
                Err(err) => {
                    search_diagnostics.push(err);
                    None
                }
            },
            Err(e) => {
                search_diagnostics.push(format!("Could not read search-index.bin: {e}"));
                None
            }
        }
    } else {
        search_diagnostics.push("assets/search-index.bin not found".to_string());
        None
    };

    let search = Some(SearchEngineMetrics {
        catalog,
        index,
        diagnostics: search_diagnostics,
    });

    let tree_shaking = analyze_tree_shaking(out_dir);

    let totals = TotalsMetrics {
        file_count: assets.len(),
        raw_bytes: total_raw,
        gzip_bytes: total_gzip,
        brotli_bytes: total_brotli,
    };

    Ok(MetricsReport {
        generated_at: Utc::now().to_rfc3339(),
        budget_kb,
        budget_status,
        frame0,
        search,
        tree_shaking,
        assets,
        totals,
    })
}

/// Write `metrics.json` into `out_dir`.
pub fn write_report(out_dir: &Path, report: &MetricsReport) -> Result<PathBuf, std::io::Error> {
    let path = out_dir.join("metrics.json");
    let json = serde_json::to_string_pretty(report).map_err(std::io::Error::other)?;
    std::fs::write(&path, json)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compress_gzip_and_brotli_canonical() {
        let input = b"The quick brown fox jumps over the lazy dog. Repeat: The quick brown fox jumps over the lazy dog.";
        let gz = compress_gzip(input).expect("gzip compress");
        let br = compress_brotli(input).expect("brotli compress");

        assert!(!gz.is_empty());
        assert!(!br.is_empty());
        assert!(gz.len() < input.len());
        assert!(br.len() < input.len());
    }

    #[test]
    fn test_deterministic_traversal_excludes_metrics_json() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path();

        std::fs::create_dir_all(path.join("sub")).unwrap();
        std::fs::write(path.join("b.txt"), "hello").unwrap();
        std::fs::write(path.join("a.txt"), "world").unwrap();
        std::fs::write(path.join("sub/c.txt"), "nested").unwrap();
        std::fs::write(path.join("metrics.json"), "{}").unwrap();

        let files = collect_output_files(path);
        let paths: Vec<String> = files
            .into_iter()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .collect();

        assert_eq!(paths, vec!["a.txt", "b.txt", "sub/c.txt"]);
        assert!(!paths.contains(&"metrics.json".to_string()));
    }

    #[test]
    fn test_frame0_critical_path_resolution() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path();

        std::fs::create_dir_all(path.join("assets/fonts")).unwrap();
        std::fs::write(path.join("assets/theme.css"), "body {}").unwrap();
        std::fs::write(path.join("assets/app.js"), "console.log()").unwrap();
        std::fs::write(path.join("assets/fonts/IBMPlexMono-Regular.woff2"), "font").unwrap();
        std::fs::write(path.join("assets/search-catalog.bin"), "catalog").unwrap();

        let html = r#"<!DOCTYPE html>
<html>
<head>
    <link rel="stylesheet" href="assets/theme.css">
    <link rel="preload" href="assets/fonts/IBMPlexMono-Regular.woff2" as="font" type="font/woff2">
</head>
<body>
    <script src="assets/app.js" defer></script>
</body>
</html>"#;
        std::fs::write(path.join("index.html"), html).unwrap();

        let frame0 = identify_frame0_assets(path, Some(html));
        assert!(frame0.contains("index.html"));
        assert!(frame0.contains("assets/theme.css"));
        assert!(frame0.contains("assets/app.js"));
        assert!(frame0.contains("assets/fonts/IBMPlexMono-Regular.woff2"));
        // search-catalog.bin should NOT be in Frame-0
        assert!(!frame0.contains("assets/search-catalog.bin"));
    }

    #[test]
    fn test_evct_evix_binary_decoding() {
        use crate::v1::search_binary::BinarySearchIndexWriter;
        use std::collections::BTreeMap;

        let docs = vec![(
            "guide/start".to_string(),
            "Getting Started".to_string(),
            "Introduction".to_string(),
            "".to_string(),
            "REF-100".to_string(),
            0usize,
            1u8,
        )];
        let snippets = vec!["Evin build execution system".to_string()];
        let prompts = vec!["Getting started guide".to_string()];

        let mut index = BTreeMap::new();
        index.insert("execution".to_string(), vec![(0usize, 1u8, 2u8)]);

        let catalog_bytes = BinarySearchIndexWriter::serialize_catalog(&docs, &snippets, &prompts);
        let index_bytes = BinarySearchIndexWriter::serialize_index(docs.len(), &index);

        let cat_metrics = decode_catalog(&catalog_bytes).expect("decode catalog");
        assert_eq!(cat_metrics.document_count, 1);
        assert_eq!(cat_metrics.prompt_count, 1);
        assert!(cat_metrics.string_pool_bytes > 0);

        let idx_metrics = decode_index(&index_bytes).expect("decode index");
        assert_eq!(idx_metrics.vocabulary_count, 1);
        assert_eq!(idx_metrics.posting_count, 1);
        assert!(idx_metrics.bits_per_posting > 0.0);
    }

    #[test]
    fn test_malformed_and_truncated_search_degradation() {
        let truncated = b"EVCT-too-short";
        let res = decode_catalog(truncated);
        assert!(res.is_err());

        let bad_magic = b"WRNG0000000000000000000000000000";
        let res2 = decode_index(bad_magic);
        assert!(res2.is_err());
    }

    #[test]
    fn test_tree_shaking_counters() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path();

        std::fs::create_dir_all(path.join("assets/fonts")).unwrap();
        let icons_svg = r##"<svg xmlns="http://www.w3.org/2000/svg">
<symbol id="icon-check"><path/></symbol>
<symbol id="icon-search"><path/></symbol>
<symbol id="icon-unused"><path/></symbol>
</svg>"##;
        std::fs::write(path.join("assets/icons.svg"), icons_svg).unwrap();
        std::fs::write(path.join("assets/fonts/IBMPlexMono-Regular.woff2"), "font").unwrap();
        std::fs::write(
            path.join("index.html"),
            r##"<html><head><link rel="preload" href="assets/fonts/IBMPlexMono-Regular.woff2" as="font"></head><body><use href="#icon-check"></use><use href="#icon-search"></use></body></html>"##,
        ).unwrap();
        std::fs::write(
            path.join("assets/theme.css"),
            "@font-face { font-family: 'Mono'; }",
        )
        .unwrap();

        let ts = analyze_tree_shaking(path);
        assert_eq!(ts.icons.total_symbols, 3);
        assert_eq!(ts.icons.used_symbols, 2);
        assert_eq!(ts.icons.unused_symbols, vec!["icon-unused".to_string()]);
        assert_eq!(ts.fonts.total_fonts, 1);
        assert_eq!(ts.fonts.preloaded_fonts, 1);
        assert_eq!(ts.fonts.active_faces, 1);
    }
}
