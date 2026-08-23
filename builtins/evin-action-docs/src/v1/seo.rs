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

use crate::v1::DocsMode;
use crate::v1::ast::{DocPage, DocSiteAst};
use chrono::{DateTime, NaiveDate};
use evin_diagnostic::{Diagnostic, IssueCode};
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use unic_langid::LanguageIdentifier;
use url::Url;

const MAX_SITEMAP_URLS: usize = 50_000;
const MAX_SITEMAP_BYTES: usize = 50 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct SeoPage {
    pub canonical_url: String,
    pub language: String,
    pub indexable: bool,
    pub self_canonical: bool,
    pub updated: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SeoCatalog {
    pub site_url: Url,
    pub pages: HashMap<String, SeoPage>,
    pub quality: SeoQualityReport,
    pub indexable_pages: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SeoQualityReport {
    pub missing_descriptions: usize,
    pub long_titles: usize,
    pub description_length: usize,
    pub duplicate_titles: usize,
    pub duplicate_descriptions: usize,
}

impl SeoQualityReport {
    pub fn total(&self) -> usize {
        self.missing_descriptions
            + self.long_titles
            + self.description_length
            + self.duplicate_titles
            + self.duplicate_descriptions
    }
}

pub fn validate_site_settings(site_url: &str, language: &str) -> Result<Url, String> {
    let mut url = parse_http_url(site_url, "site_url")?;
    if url.query().is_some() || url.fragment().is_some() {
        return Err("site_url must not contain a query or fragment".to_string());
    }
    if !url.path().ends_with('/') {
        let normalized = format!("{}/", url.path());
        url.set_path(&normalized);
    }
    validate_language(language, "language")?;
    Ok(url)
}

impl SeoCatalog {
    pub fn build(site: &DocSiteAst, site_url: &str, language: &str) -> Result<Self, Diagnostic> {
        let site_url = validate_site_settings(site_url, language).map_err(invalid_seo)?;
        let site_language = normalize_language(language).map_err(invalid_seo)?;
        let default_urls: HashSet<String> = site
            .pages
            .iter()
            .map(|page| canonical_for_slug(&site_url, &page.slug))
            .collect::<Result<_, _>>()
            .map_err(invalid_seo)?;

        let mut pages = HashMap::new();
        let mut quality = SeoQualityReport::default();
        let mut titles = HashMap::<String, Vec<String>>::new();
        let mut descriptions = HashMap::<String, Vec<String>>::new();
        for page in &site.pages {
            let title = metadata_string(page, "title").unwrap_or(&page.slug);
            let description = metadata_string(page, "description").unwrap_or("");
            if title.chars().count() > 60 {
                quality.long_titles += 1;
            }
            if description.is_empty() {
                quality.missing_descriptions += 1;
            } else {
                let length = description.chars().count();
                if !(50..=160).contains(&length) {
                    quality.description_length += 1;
                }
                descriptions
                    .entry(description.to_string())
                    .or_default()
                    .push(page.source_path.clone());
            }
            titles
                .entry(title.to_string())
                .or_default()
                .push(page.source_path.clone());

            let default_canonical =
                canonical_for_slug(&site_url, &page.slug).map_err(invalid_seo)?;
            let canonical_url = match seo_metadata(page, "canonical")? {
                Some(value) => {
                    let canonical = parse_http_url(value, "canonical").map_err(|message| {
                        invalid_seo(format!("{} in '{}'", message, page.source_path))
                    })?;
                    if canonical.fragment().is_some() {
                        return Err(invalid_seo(format!(
                            "canonical must not contain a fragment in '{}'",
                            page.source_path
                        )));
                    }
                    let normalized = canonical.to_string();
                    if normalized.starts_with(site_url.as_str())
                        && !default_urls.contains(&normalized)
                    {
                        return Err(invalid_seo(format!(
                            "canonical URL '{}' in '{}' does not identify a generated route",
                            normalized, page.source_path
                        )));
                    }
                    normalized
                }
                None => default_canonical.clone(),
            };
            let indexable = match seo_metadata(page, "robots")? {
                None | Some("index") => true,
                Some("noindex") => false,
                Some(value) => {
                    return Err(invalid_seo(format!(
                        "robots must be 'index' or 'noindex' in '{}', found '{}'",
                        page.source_path, value
                    )));
                }
            };
            let page_language = seo_metadata(page, "language")?
                .map(|value| {
                    normalize_language(value).map_err(|message| {
                        invalid_seo(format!("{} in '{}'", message, page.source_path))
                    })
                })
                .transpose()?
                .unwrap_or_else(|| site_language.clone());
            let updated = seo_metadata(page, "updated")?
                .map(|value| {
                    normalize_updated(value).map_err(|message| {
                        invalid_seo(format!("{} in '{}'", message, page.source_path))
                    })
                })
                .transpose()?;
            let self_canonical = canonical_url == default_canonical;
            pages.insert(
                page.slug.clone(),
                SeoPage {
                    canonical_url,
                    language: page_language,
                    indexable,
                    self_canonical,
                    updated,
                },
            );
        }
        quality.duplicate_titles = duplicate_count(titles);
        quality.duplicate_descriptions = duplicate_count(descriptions);
        let indexable_pages = pages
            .values()
            .filter(|page| page.indexable && page.self_canonical)
            .count();
        Ok(Self {
            site_url,
            pages,
            quality,
            indexable_pages,
        })
    }

    pub fn page(&self, slug: &str) -> &SeoPage {
        self.pages.get(slug).expect("SEO catalog covers every page")
    }

    pub fn discovery_files(&self, mode: DocsMode) -> Result<HashMap<String, String>, Diagnostic> {
        self.discovery_files_with_limits(mode, MAX_SITEMAP_URLS, MAX_SITEMAP_BYTES)
    }

    fn discovery_files_with_limits(
        &self,
        mode: DocsMode,
        max_urls: usize,
        max_bytes: usize,
    ) -> Result<HashMap<String, String>, Diagnostic> {
        let mut files = HashMap::new();
        if mode == DocsMode::Serve {
            files.insert(
                "robots.txt".to_string(),
                "User-agent: *\nDisallow: /\n".to_string(),
            );
            return Ok(files);
        }
        let mut entries: Vec<_> = self
            .pages
            .values()
            .filter(|page| page.indexable && page.self_canonical)
            .map(sitemap_entry)
            .collect();
        entries.sort();
        let chunks = split_sitemap_entries(entries, max_urls, max_bytes)?;
        if chunks.len() == 1 {
            files.insert("sitemap.xml".to_string(), sitemap_document(&chunks[0]));
        } else {
            if chunks.len() > 50_000 {
                return Err(invalid_seo(
                    "generated sitemap index would exceed 50,000 entries",
                ));
            }
            let mut index_entries = String::new();
            for (index, chunk) in chunks.iter().enumerate() {
                let name = format!("sitemap-{}.xml", index + 1);
                files.insert(name.clone(), sitemap_document(chunk));
                index_entries.push_str("  <sitemap><loc>");
                let sitemap_url = self
                    .site_url
                    .join(&name)
                    .map_err(|error| invalid_seo(error.to_string()))?;
                index_entries.push_str(&escape_xml(sitemap_url.as_str()));
                index_entries.push_str("</loc></sitemap>\n");
            }
            files.insert("sitemap.xml".to_string(), format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<sitemapindex xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n{index_entries}</sitemapindex>\n"));
        }
        let allow_path = if self.site_url.path().is_empty() {
            "/"
        } else {
            self.site_url.path()
        };
        files.insert(
            "robots.txt".to_string(),
            format!(
                "User-agent: *\nAllow: {allow_path}\nSitemap: {}\n",
                self.site_url
                    .join("sitemap.xml")
                    .map_err(|error| invalid_seo(error.to_string()))?
            ),
        );
        Ok(files)
    }
}

fn parse_http_url(value: &str, field: &str) -> Result<Url, String> {
    if value.trim().is_empty() {
        return Err(format!("{field} is required"));
    }
    let url = Url::parse(value)
        .map_err(|error| format!("{field} must be an absolute HTTP(S) URL: {error}"))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(format!("{field} must use HTTP or HTTPS and include a host"));
    }
    Ok(url)
}

fn normalize_language(value: &str) -> Result<String, String> {
    let language = LanguageIdentifier::from_str(value.trim())
        .map_err(|_| format!("language '{}' is not a valid language tag", value))?;
    Ok(language.to_string())
}

fn validate_language(value: &str, field: &str) -> Result<(), String> {
    normalize_language(value)
        .map(|_| ())
        .map_err(|error| format!("{field}: {error}"))
}

fn normalize_updated(value: &str) -> Result<String, String> {
    if let Ok(date) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        return Ok(date.format("%Y-%m-%d").to_string());
    }
    DateTime::parse_from_rfc3339(value)
        .map(|date| date.to_rfc3339())
        .map_err(|_| format!("updated '{}' must be YYYY-MM-DD or RFC 3339", value))
}

fn canonical_for_slug(site_url: &Url, slug: &str) -> Result<String, String> {
    if slug == "index" {
        return Ok(site_url.to_string());
    }
    let mut url = site_url.clone();
    let mut segments = url
        .path_segments_mut()
        .map_err(|_| "site_url cannot be used as a hierarchical URL".to_string())?;
    segments.pop_if_empty();
    for segment in slug.split('/') {
        segments.push(segment);
    }
    segments.push("");
    drop(segments);
    Ok(url.to_string())
}

fn metadata_string<'a>(page: &'a DocPage, key: &str) -> Option<&'a str> {
    page.metadata
        .get(key)
        .and_then(|value| value.as_str())
        .map(str::trim)
}

fn seo_metadata<'a>(page: &'a DocPage, key: &str) -> Result<Option<&'a str>, Diagnostic> {
    match page.metadata.get(key) {
        None => Ok(None),
        Some(serde_json::Value::String(value)) => Ok(Some(value.trim())),
        Some(_) => Err(invalid_seo(format!(
            "{} must be a string in '{}'",
            key, page.source_path
        ))),
    }
}

fn duplicate_count(values: HashMap<String, Vec<String>>) -> usize {
    values
        .into_values()
        .filter(|sources| sources.len() > 1)
        .count()
}

fn sitemap_entry(page: &SeoPage) -> String {
    let mut entry = format!("  <url><loc>{}</loc>", escape_xml(&page.canonical_url));
    if let Some(updated) = &page.updated {
        entry.push_str("<lastmod>");
        entry.push_str(&escape_xml(updated));
        entry.push_str("</lastmod>");
    }
    entry.push_str("</url>\n");
    entry
}

fn sitemap_document(entries: &[String]) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n{}</urlset>\n",
        entries.concat()
    )
}

fn split_sitemap_entries(
    entries: Vec<String>,
    max_urls: usize,
    max_bytes: usize,
) -> Result<Vec<Vec<String>>, Diagnostic> {
    let overhead = sitemap_document(&[]).len();
    let mut chunks = vec![Vec::new()];
    let mut bytes = overhead;
    for entry in entries {
        if entry.len() + overhead > max_bytes {
            return Err(invalid_seo(
                "one sitemap URL entry exceeds the 50 MB sitemap limit",
            ));
        }
        let current = chunks.last_mut().unwrap();
        if !current.is_empty() && (current.len() >= max_urls || bytes + entry.len() > max_bytes) {
            chunks.push(Vec::new());
            bytes = overhead;
        }
        bytes += entry.len();
        chunks.last_mut().unwrap().push(entry);
    }
    Ok(chunks)
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn invalid_seo(message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(message.into(), IssueCode::new("invalid-seo"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v1::ast::{DocFormat, DocPage};

    fn page(slug: &str, title: &str, metadata: &[(&str, &str)]) -> DocPage {
        let mut values = HashMap::from([
            (
                "title".to_string(),
                serde_json::Value::String(title.to_string()),
            ),
            (
                "description".to_string(),
                serde_json::Value::String(
                    "A practical description long enough to satisfy the SEO quality guidance."
                        .to_string(),
                ),
            ),
        ]);
        for (key, value) in metadata {
            values.insert(
                (*key).to_string(),
                serde_json::Value::String((*value).to_string()),
            );
        }
        DocPage {
            slug: slug.to_string(),
            source_path: format!("{slug}.md"),
            format: DocFormat::Markdown,
            metadata: values,
            reference: None,
            toc: vec![],
            ast: vec![],
        }
    }

    fn site(pages: Vec<DocPage>) -> DocSiteAst {
        DocSiteAst {
            version: "1.0".to_string(),
            title: Some("Docs".to_string()),
            reference_prefix: "REF".to_string(),
            pages,
        }
    }

    #[test]
    fn normalizes_site_urls_languages_and_page_metadata() {
        let catalog = SeoCatalog::build(
            &site(vec![page(
                "guide/hello world#draft",
                "Guide",
                &[("language", "pt-br"), ("updated", "2026-08-13")],
            )]),
            "https://example.com/project",
            "en-us",
        )
        .unwrap();
        assert_eq!(catalog.site_url.as_str(), "https://example.com/project/");
        let page = catalog.page("guide/hello world#draft");
        assert_eq!(
            page.canonical_url,
            "https://example.com/project/guide/hello%20world%23draft/"
        );
        assert_eq!(page.language, "pt-BR");
        assert_eq!(page.updated.as_deref(), Some("2026-08-13"));
    }

    #[test]
    fn rejects_invalid_site_and_page_declarations() {
        for site_url in [
            "",
            "ftp://example.com",
            "https://example.com/?draft=1",
            "https://example.com/#top",
        ] {
            assert!(validate_site_settings(site_url, "en").is_err());
        }
        assert!(validate_site_settings("https://example.com", "not_a_language").is_err());
        for (key, value) in [
            ("robots", "hidden"),
            ("updated", "August 13"),
            ("canonical", "guide/other"),
            ("canonical", "https://example.com/docs/missing/"),
        ] {
            assert!(
                SeoCatalog::build(
                    &site(vec![page("guide", "Guide", &[(key, value)])]),
                    "https://example.com/docs/",
                    "en",
                )
                .is_err(),
                "{key}={value} should fail"
            );
        }
        let mut invalid_type = page("guide", "Guide", &[]);
        invalid_type
            .metadata
            .insert("robots".to_string(), serde_json::Value::Bool(false));
        assert!(
            SeoCatalog::build(&site(vec![invalid_type]), "https://example.com/docs/", "en",)
                .is_err()
        );
    }

    #[test]
    fn sitemap_filters_pages_escapes_xml_and_uses_explicit_lastmod() {
        let catalog = SeoCatalog::build(
            &site(vec![
                page("index", "Home", &[("updated", "2026-08-13")]),
                page("private", "Private", &[("robots", "noindex")]),
                page(
                    "duplicate",
                    "Duplicate",
                    &[("canonical", "https://elsewhere.example/guide/")],
                ),
                page("a&b", "Escaped", &[]),
            ]),
            "https://example.com/docs/",
            "en",
        )
        .unwrap();
        let files = catalog.discovery_files(DocsMode::Build).unwrap();
        let sitemap = &files["sitemap.xml"];
        assert!(
            sitemap.contains("<loc>https://example.com/docs/</loc><lastmod>2026-08-13</lastmod>")
        );
        assert!(sitemap.contains("a&amp;b/"));
        assert!(!sitemap.contains("private"));
        assert!(!sitemap.contains("elsewhere"));
        assert_eq!(
            files["robots.txt"],
            "User-agent: *\nAllow: /docs/\nSitemap: https://example.com/docs/sitemap.xml\n"
        );
    }

    #[test]
    fn serve_mode_disallows_crawling_and_omits_sitemap() {
        let catalog = SeoCatalog::build(
            &site(vec![page("index", "Home", &[])]),
            "https://example.com/",
            "en",
        )
        .unwrap();
        let files = catalog.discovery_files(DocsMode::Serve).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files["robots.txt"], "User-agent: *\nDisallow: /\n");
    }

    #[test]
    fn splits_sitemaps_by_url_count_and_byte_size() {
        let catalog = SeoCatalog::build(
            &site(vec![
                page("index", "Home", &[]),
                page("guide", "Guide", &[]),
            ]),
            "https://example.com/",
            "en",
        )
        .unwrap();
        let by_count = catalog
            .discovery_files_with_limits(DocsMode::Build, 1, MAX_SITEMAP_BYTES)
            .unwrap();
        assert!(by_count["sitemap.xml"].contains("<sitemapindex"));
        assert!(by_count.contains_key("sitemap-1.xml"));
        assert!(by_count.contains_key("sitemap-2.xml"));

        let one_entry_size = ["index", "guide"]
            .into_iter()
            .map(|slug| sitemap_document(&[sitemap_entry(catalog.page(slug))]).len())
            .max()
            .unwrap();
        let by_size = catalog
            .discovery_files_with_limits(DocsMode::Build, MAX_SITEMAP_URLS, one_entry_size)
            .unwrap();
        assert!(by_size["sitemap.xml"].contains("<sitemapindex"));
    }

    #[test]
    fn reports_quality_warnings_without_rejecting_the_site() {
        let short = page("one", &"T".repeat(61), &[("description", "Short")]);
        let duplicate = page("two", &"T".repeat(61), &[("description", "Short")]);
        let catalog =
            SeoCatalog::build(&site(vec![short, duplicate]), "https://example.com/", "en").unwrap();
        assert_eq!(catalog.quality.long_titles, 2);
        assert_eq!(catalog.quality.description_length, 2);
        assert_eq!(catalog.quality.duplicate_titles, 1);
        assert_eq!(catalog.quality.duplicate_descriptions, 1);
        assert_eq!(catalog.quality.total(), 6);
    }

    #[test]
    fn markdown_and_rst_expose_equivalent_seo_metadata() {
        let markdown = evin_doc::MarkdownParser::parse(
            "---\nlanguage: pt-BR\nrobots: noindex\nupdated: 2026-08-13\n---\n# Guide\n\nA useful guide.",
            true,
        );
        let rst = evin_doc::RstParser::parse(
            "Guide\n=====\n\n:language: pt-BR\n:robots: noindex\n:updated: 2026-08-13\n\nA useful guide.",
        );
        for key in ["language", "robots", "updated"] {
            assert_eq!(markdown.metadata.get(key), rst.metadata.get(key));
        }
    }

    #[test]
    fn inferred_descriptions_are_whitespace_normalized_and_unicode_safe() {
        let source = format!("# Guide\n\n{}   final", "文".repeat(170));
        let parsed = evin_doc::MarkdownParser::parse(&source, true);
        let description = parsed.metadata["description"].as_str().unwrap();
        assert_eq!(description.chars().count(), 160);
        assert!(description.ends_with("..."));
        assert!(!description.contains("   "));
    }
}
