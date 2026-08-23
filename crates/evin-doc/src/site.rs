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

//! Site assembly, formal references, and immutable lookup indexes.

use crate::analysis::{MutVisitor, walk_nodes_mut};
use crate::ast::{DocFormat, DocPage, DocSiteAst, FormalReference, InlineNode};
use crate::parser::{MarkdownParser, RstParser};
use crate::source::SourceResolver;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocIssue {
    pub code: &'static str,
    pub severity: DocSeverity,
    pub source_path: Option<String>,
    pub message: String,
}

impl DocIssue {
    pub fn error(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            severity: DocSeverity::Error,
            source_path: None,
            message: message.into(),
        }
    }
    pub fn with_source_path(mut self, source_path: impl Into<String>) -> Self {
        self.source_path = Some(source_path.into());
        self
    }
}

impl std::fmt::Display for DocIssue {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}
impl std::error::Error for DocIssue {}

#[derive(Debug, Clone)]
pub struct CompileOptions {
    pub extract_frontmatter: bool,
    pub reference_prefix: String,
    pub site_version: String,
}

impl Default for CompileOptions {
    fn default() -> Self {
        Self {
            extract_frontmatter: true,
            reference_prefix: "REF".to_string(),
            site_version: "1.0".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DocumentInput<'a> {
    pub source_path: String,
    pub slug: String,
    pub format: DocFormat,
    pub content: &'a str,
    pub is_start_page: bool,
}

pub fn compile_site<'a>(
    inputs: impl IntoIterator<Item = DocumentInput<'a>>,
    options: CompileOptions,
    title: Option<String>,
) -> Result<DocSiteAst, DocIssue> {
    let reference_prefix = normalize_reference_prefix(&options.reference_prefix)?;
    let pages = inputs
        .into_iter()
        .map(|input| {
            let (metadata, toc, ast) = match input.format {
                DocFormat::Markdown => {
                    let parsed = MarkdownParser::parse(input.content, options.extract_frontmatter);
                    (parsed.metadata, parsed.toc, parsed.ast)
                }
                DocFormat::ReStructuredText => {
                    let parsed = if input.is_start_page {
                        RstParser::parse_start_page(input.content)
                    } else {
                        RstParser::parse(input.content)
                    };
                    (parsed.metadata, parsed.toc, parsed.ast)
                }
            };
            DocPage {
                slug: input.slug,
                source_path: input.source_path,
                format: input.format,
                metadata,
                reference: None,
                toc,
                ast,
            }
        })
        .collect();
    let mut site = DocSiteAst {
        version: options.site_version,
        title,
        reference_prefix,
        pages,
    };
    ReferenceCatalog::prepare(&mut site)?;
    Ok(site)
}

pub fn normalize_reference_prefix(prefix: &str) -> Result<String, DocIssue> {
    let prefix = prefix.trim().to_ascii_uppercase();
    if prefix.is_empty()
        || !prefix
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
    {
        return Err(DocIssue::error(
            "invalid-config",
            "reference_prefix must contain only ASCII letters and digits",
        ));
    }
    Ok(prefix)
}

pub struct ReferenceCatalog;

impl ReferenceCatalog {
    pub fn prepare(site: &mut DocSiteAst) -> Result<(), DocIssue> {
        let prefix = site.reference_prefix.clone();
        let mut numbers = HashMap::<u64, String>::new();
        for page in &mut site.pages {
            let Some(detected) = detect_reference(&page.source_path)? else {
                if page.metadata.contains_key("depends_on") {
                    return Err(reference_error(format!(
                        "Page '{}' declares depends_on but is not a formal reference",
                        page.source_path
                    )));
                }
                continue;
            };
            if let Some(previous) = numbers.insert(detected.number, page.source_path.clone()) {
                return Err(reference_error(format!(
                    "Duplicate formal reference number {} in '{}' and '{}'",
                    detected.number, previous, page.source_path
                )));
            }
            page.slug = descriptive_slug(&page.source_path, &detected.descriptive_stem)?;
            page.reference = Some(FormalReference {
                id: format!("{}-{}", prefix, detected.digits),
                digits: detected.digits,
                number: detected.number,
                depends_on: Vec::new(),
                referenced_by: Vec::new(),
            });
        }
        validate_route_collisions(&site.pages)?;
        site.pages.sort_by(compare_pages);
        let id_to_index: HashMap<String, usize> = site
            .pages
            .iter()
            .enumerate()
            .filter_map(|(index, page)| {
                page.reference
                    .as_ref()
                    .map(|reference| (reference.id.to_ascii_uppercase(), index))
            })
            .collect();
        for index in 0..site.pages.len() {
            if site.pages[index].reference.is_none() {
                continue;
            }
            let id = site.pages[index]
                .reference
                .as_ref()
                .expect("reference checked")
                .id
                .clone();
            let dependencies = parse_dependencies(&site.pages[index], &prefix)?;
            let mut seen = HashSet::new();
            for dependency in &dependencies {
                if dependency.eq_ignore_ascii_case(&id) {
                    return Err(reference_error(format!(
                        "Formal reference {id} cannot depend on itself"
                    )));
                }
                if !seen.insert(dependency.to_ascii_uppercase()) {
                    return Err(reference_error(format!(
                        "Formal reference {id} declares duplicate dependency {dependency}"
                    )));
                }
                if !id_to_index.contains_key(&dependency.to_ascii_uppercase()) {
                    return Err(reference_error(format!(
                        "Formal reference {id} depends on unknown reference {dependency}"
                    )));
                }
            }
            let mut dependencies = dependencies;
            dependencies.sort_by_key(|dependency| {
                id_to_index
                    .get(&dependency.to_ascii_uppercase())
                    .and_then(|target| site.pages[*target].reference.as_ref())
                    .map(|reference| reference.number)
                    .unwrap_or(u64::MAX)
            });
            site.pages[index]
                .reference
                .as_mut()
                .expect("reference checked")
                .depends_on = dependencies;
        }
        validate_acyclic(site, &id_to_index)?;
        let edges: Vec<(String, String)> = site
            .pages
            .iter()
            .filter_map(|page| page.reference.as_ref())
            .flat_map(|reference| {
                reference
                    .depends_on
                    .iter()
                    .cloned()
                    .map(move |dependency| (reference.id.clone(), dependency))
            })
            .collect();
        for (source, target) in edges {
            let target_index = id_to_index[&target.to_ascii_uppercase()];
            site.pages[target_index]
                .reference
                .as_mut()
                .expect("index targets reference")
                .referenced_by
                .push(source);
        }
        let id_to_number: HashMap<String, u64> = site
            .pages
            .iter()
            .filter_map(|page| {
                page.reference
                    .as_ref()
                    .map(|reference| (reference.id.to_ascii_uppercase(), reference.number))
            })
            .collect();
        for page in &mut site.pages {
            if let Some(reference) = &mut page.reference {
                reference.referenced_by.sort_by_key(|source| {
                    id_to_number
                        .get(&source.to_ascii_uppercase())
                        .copied()
                        .unwrap_or(u64::MAX)
                });
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct SiteIndex {
    slug_to_page: HashMap<String, usize>,
    alias_to_route: HashMap<String, String>,
    source_to_page: HashMap<String, usize>,
    anchors_by_route: HashMap<String, HashSet<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AliasTarget<'a> {
    pub slug: &'a str,
    pub anchor: Option<&'a str>,
}

impl SiteIndex {
    pub fn build(site: &DocSiteAst) -> Result<Self, DocIssue> {
        let mut index = Self {
            slug_to_page: HashMap::with_capacity(site.pages.len()),
            alias_to_route: HashMap::new(),
            source_to_page: HashMap::with_capacity(site.pages.len()),
            anchors_by_route: HashMap::with_capacity(site.pages.len()),
        };
        for (position, page) in site.pages.iter().enumerate() {
            if index
                .slug_to_page
                .insert(page.slug.clone(), position)
                .is_some()
            {
                return Err(reference_error(format!(
                    "Documentation route '{}' is duplicated",
                    page.slug
                )));
            }
            index
                .source_to_page
                .insert(page.source_path.clone(), position);
            if let Some(reference) = &page.reference {
                index
                    .alias_to_route
                    .insert(reference.id.to_ascii_uppercase(), page.slug.clone());
            }
            let anchors = index.anchors_by_route.entry(page.slug.clone()).or_default();
            anchors.extend(page.toc.iter().map(|toc| toc.anchor.clone()));
        }
        Ok(index)
    }
    pub fn page<'a>(&self, site: &'a DocSiteAst, slug: &str) -> Option<&'a DocPage> {
        self.slug_to_page
            .get(slug)
            .and_then(|index| site.pages.get(*index))
    }
    pub fn page_position(&self, slug: &str) -> Option<usize> {
        self.slug_to_page.get(slug).copied()
    }
    pub fn contains_route(&self, slug: &str) -> bool {
        self.slug_to_page.contains_key(slug)
    }
    pub fn page_by_source<'a>(
        &self,
        site: &'a DocSiteAst,
        source_path: &str,
    ) -> Option<&'a DocPage> {
        self.source_to_page
            .get(source_path)
            .and_then(|index| site.pages.get(*index))
    }
    pub fn route_for_alias(&self, alias: &str) -> Option<&str> {
        self.alias_to_route
            .get(alias)
            .map(String::as_str)
            .or_else(|| {
                self.alias_to_route
                    .iter()
                    .find(|(known, _)| known.eq_ignore_ascii_case(alias))
                    .map(|(_, route)| route.as_str())
            })
    }
    pub fn has_anchor(&self, slug: &str, anchor: &str) -> bool {
        self.anchors_by_route
            .get(slug)
            .is_some_and(|anchors| anchors.contains(anchor))
    }
    pub fn resolve_alias<'a>(&'a self, href: &'a str) -> Option<AliasTarget<'a>> {
        let (alias, anchor) = href
            .split_once('#')
            .map_or((href, None), |(alias, anchor)| (alias, Some(anchor)));
        self.route_for_alias(alias)
            .map(|slug| AliasTarget { slug, anchor })
    }
}

pub fn resolve_alias(site: &DocSiteAst, href: &str) -> Option<(String, Option<String>)> {
    let (target, anchor) = href
        .split_once('#')
        .map_or((href, None), |(target, anchor)| (target, Some(anchor)));
    site.pages
        .iter()
        .find(|page| {
            page.reference
                .as_ref()
                .is_some_and(|reference| reference.id.eq_ignore_ascii_case(target))
        })
        .map(|page| (page.slug.clone(), anchor.map(str::to_string)))
}

pub fn looks_like_alias(site: &DocSiteAst, href: &str) -> bool {
    let target = href.split_once('#').map_or(href, |(target, _)| target);
    let Some((prefix, digits)) = target.split_once('-') else {
        return false;
    };
    prefix.eq_ignore_ascii_case(&site.reference_prefix)
        && digits.len() >= 4
        && digits.chars().all(|character| character.is_ascii_digit())
}

pub fn rewrite_aliases_to_routes(site: &mut DocSiteAst) {
    let aliases = site
        .pages
        .iter()
        .filter_map(|page| {
            page.reference
                .as_ref()
                .map(|reference| (reference.id.to_ascii_uppercase(), page.slug.clone()))
        })
        .collect();
    struct AliasRewriter<'a> {
        aliases: &'a HashMap<String, String>,
    }
    impl MutVisitor for AliasRewriter<'_> {
        fn visit_inline(&mut self, inline: &mut InlineNode) {
            if let InlineNode::Link { href, .. } = inline {
                let (target, anchor) = href
                    .split_once('#')
                    .map_or((href.as_str(), None), |(target, anchor)| {
                        (target, Some(anchor))
                    });
                if let Some(slug) = self.aliases.get(&target.to_ascii_uppercase()) {
                    *href =
                        anchor.map_or_else(|| slug.clone(), |anchor| format!("{slug}#{anchor}"));
                }
            }
        }
    }
    let mut rewriter = AliasRewriter { aliases: &aliases };
    for page in &mut site.pages {
        walk_nodes_mut(&mut page.ast, &mut rewriter);
    }
}

struct DetectedReference {
    digits: String,
    number: u64,
    descriptive_stem: String,
}

fn detect_reference(source_path: &str) -> Result<Option<DetectedReference>, DocIssue> {
    static PREFIX: OnceLock<regex::Regex> = OnceLock::new();
    static SUFFIX: OnceLock<regex::Regex> = OnceLock::new();
    static DOT_SUFFIX: OnceLock<regex::Regex> = OnceLock::new();
    let prefix =
        PREFIX.get_or_init(|| regex::Regex::new(r"^([0-9]{4,})[-_](.+)$").expect("valid regex"));
    let suffix =
        SUFFIX.get_or_init(|| regex::Regex::new(r"^(.+)[-_]([0-9]{4,})$").expect("valid regex"));
    let dot_suffix =
        DOT_SUFFIX.get_or_init(|| regex::Regex::new(r"^(.+)\.([0-9]{4,})$").expect("valid regex"));
    let stem = Path::new(source_path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| {
            reference_error(format!(
                "Documentation filename '{source_path}' is not valid UTF-8"
            ))
        })?;
    let prefix_match = prefix.captures(stem);
    let suffix_match = suffix.captures(stem);
    let dot_suffix_match = dot_suffix.captures(stem);
    if [
        prefix_match.is_some(),
        suffix_match.is_some(),
        dot_suffix_match.is_some(),
    ]
    .into_iter()
    .filter(|matched| *matched)
    .count()
        > 1
    {
        return Err(reference_error(format!(
            "Formal reference filename '{source_path}' is ambiguous: use one numeric prefix or suffix"
        )));
    }
    let (digits, descriptive_stem) = if let Some(captures) = prefix_match {
        (captures[1].to_string(), captures[2].to_string())
    } else if let Some(captures) = suffix_match {
        (captures[2].to_string(), captures[1].to_string())
    } else if let Some(captures) = dot_suffix_match {
        (captures[2].to_string(), captures[1].to_string())
    } else {
        let starts_numeric = stem
            .chars()
            .take_while(|character| character.is_ascii_digit())
            .count()
            >= 4;
        let ends_numeric = stem
            .chars()
            .rev()
            .take_while(|character| character.is_ascii_digit())
            .count()
            >= 4;
        if starts_numeric || ends_numeric {
            return Err(reference_error(format!(
                "Formal reference filename '{source_path}' must separate its number with '-', '_', or '.'"
            )));
        }
        return Ok(None);
    };
    let number = digits.parse::<u64>().map_err(|_| {
        reference_error(format!(
            "Formal reference number '{digits}' in '{source_path}' is too large"
        ))
    })?;
    if number == 0 {
        return Err(reference_error(format!(
            "Formal reference number in '{source_path}' must be greater than zero"
        )));
    }
    Ok(Some(DetectedReference {
        digits,
        number,
        descriptive_stem,
    }))
}

fn descriptive_slug(source_path: &str, descriptive_stem: &str) -> Result<String, DocIssue> {
    let path = Path::new(source_path);
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let mut rewritten = PathBuf::from(path);
    rewritten.set_file_name(format!("{descriptive_stem}.{extension}"));
    Ok(SourceResolver::compute_slug(&rewritten))
}

fn validate_route_collisions(pages: &[DocPage]) -> Result<(), DocIssue> {
    let mut routes = HashMap::<String, String>::new();
    for page in pages {
        if let Some(previous) = routes.insert(page.slug.clone(), page.source_path.clone()) {
            return Err(reference_error(format!(
                "Documentation route '{}' is produced by both '{}' and '{}'",
                page.slug, previous, page.source_path
            )));
        }
    }
    Ok(())
}

fn parse_dependencies(page: &DocPage, prefix: &str) -> Result<Vec<String>, DocIssue> {
    let Some(value) = page.metadata.get("depends_on") else {
        return Ok(Vec::new());
    };
    let values: Vec<&str> = match value {
        serde_json::Value::String(value) => value
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .collect(),
        serde_json::Value::Array(values) => values
            .iter()
            .map(|value| value.as_str())
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| {
                reference_error(format!(
                    "depends_on in '{}' must contain only reference identifiers",
                    page.source_path
                ))
            })?,
        _ => {
            return Err(reference_error(format!(
                "depends_on in '{}' must be a comma-separated string or string list",
                page.source_path
            )));
        }
    };
    values
        .into_iter()
        .map(|value| {
            let normalized = value.trim().to_ascii_uppercase();
            let digits = normalized
                .strip_prefix(&format!("{prefix}-"))
                .ok_or_else(|| {
                    reference_error(format!(
                        "Dependency '{value}' in '{}' must use the {prefix}-<digits> form",
                        page.source_path
                    ))
                })?;
            if digits.len() < 4 || !digits.chars().all(|character| character.is_ascii_digit()) {
                return Err(reference_error(format!(
                    "Dependency '{value}' in '{}' must contain at least four digits",
                    page.source_path
                )));
            }
            Ok(normalized)
        })
        .collect()
}

fn validate_acyclic(
    site: &DocSiteAst,
    id_to_index: &HashMap<String, usize>,
) -> Result<(), DocIssue> {
    fn visit(
        id: &str,
        site: &DocSiteAst,
        id_to_index: &HashMap<String, usize>,
        visiting: &mut HashSet<String>,
        visited: &mut HashSet<String>,
    ) -> Result<(), DocIssue> {
        if visited.contains(id) {
            return Ok(());
        }
        if !visiting.insert(id.to_string()) {
            return Err(reference_error(format!(
                "Formal reference dependency cycle includes {id}"
            )));
        }
        for dependency in &site.pages[id_to_index[id]]
            .reference
            .as_ref()
            .expect("index targets reference")
            .depends_on
        {
            visit(
                &dependency.to_ascii_uppercase(),
                site,
                id_to_index,
                visiting,
                visited,
            )?;
        }
        visiting.remove(id);
        visited.insert(id.to_string());
        Ok(())
    }
    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();
    for id in id_to_index.keys() {
        visit(id, site, id_to_index, &mut visiting, &mut visited)?;
    }
    Ok(())
}

fn compare_pages(left: &DocPage, right: &DocPage) -> std::cmp::Ordering {
    let left_group = left.slug.split('/').next().unwrap_or("");
    let right_group = right.slug.split('/').next().unwrap_or("");
    match left_group.cmp(right_group) {
        std::cmp::Ordering::Equal => match (left.slug == left_group, right.slug == right_group) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => compare_navigation_order(left, right).then_with(|| {
                match (&left.reference, &right.reference) {
                    (Some(left), Some(right)) => left
                        .number
                        .cmp(&right.number)
                        .then_with(|| left.id.cmp(&right.id)),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => page_title(left)
                        .cmp(page_title(right))
                        .then_with(|| left.slug.cmp(&right.slug)),
                }
            }),
        },
        order => order,
    }
}

fn compare_navigation_order(left: &DocPage, right: &DocPage) -> std::cmp::Ordering {
    let left_order = SourceResolver::navigation_order(Path::new(&left.source_path))
        .ok()
        .flatten();
    let right_order = SourceResolver::navigation_order(Path::new(&right.source_path))
        .ok()
        .flatten();
    match (left_order, right_order) {
        (Some(left), Some(right)) => left.cmp(&right),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}
fn page_title(page: &DocPage) -> &str {
    page.metadata
        .get("title")
        .and_then(|value| value.as_str())
        .unwrap_or(&page.slug)
}
fn reference_error(message: String) -> DocIssue {
    DocIssue::error("formal-reference-invalid", message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(source_path: &str) -> DocPage {
        DocPage {
            slug: SourceResolver::compute_slug(Path::new(source_path)),
            source_path: source_path.to_owned(),
            format: DocFormat::ReStructuredText,
            metadata: HashMap::new(),
            reference: None,
            toc: Vec::new(),
            ast: Vec::new(),
        }
    }

    #[test]
    fn prepares_references_and_indexes_aliases() {
        let mut site = DocSiteAst {
            version: "1.0".to_owned(),
            title: None,
            reference_prefix: "REF".to_owned(),
            pages: vec![
                page("reference/cli-1000.rst"),
                page("reference/workspace-2000.rst"),
            ],
        };
        site.pages[1]
            .metadata
            .insert("depends_on".to_owned(), serde_json::json!("REF-1000"));
        ReferenceCatalog::prepare(&mut site).unwrap();
        let index = SiteIndex::build(&site).unwrap();
        assert_eq!(index.route_for_alias("ref-1000"), Some("reference/cli"));
        assert_eq!(
            site.pages[0].reference.as_ref().unwrap().referenced_by,
            ["REF-2000"]
        );
    }

    #[test]
    fn site_index_resolves_routes_sources_anchors_and_borrowed_alias_parts() {
        let mut site = DocSiteAst {
            version: "1.0".to_owned(),
            title: None,
            reference_prefix: "REF".to_owned(),
            pages: vec![page("reference/proposal-1000.rst"), page("guide.rst")],
        };
        ReferenceCatalog::prepare(&mut site).unwrap();
        let proposal_position = site
            .pages
            .iter()
            .position(|page| page.slug == "reference/proposal")
            .expect("formal-reference page exists");
        site.pages[proposal_position]
            .toc
            .push(crate::ast::TocEntry {
                level: 2,
                text: "Details".to_owned(),
                anchor: "details".to_owned(),
            });
        let index = SiteIndex::build(&site).unwrap();

        assert_eq!(
            index.page_position("reference/proposal"),
            Some(proposal_position)
        );
        assert!(index.contains_route("reference/proposal"));
        assert_eq!(
            index
                .page_by_source(&site, "reference/proposal-1000.rst")
                .map(|page| page.slug.as_str()),
            Some("reference/proposal")
        );
        assert!(index.has_anchor("reference/proposal", "details"));
        assert_eq!(
            index.resolve_alias("ref-1000#details"),
            Some(AliasTarget {
                slug: "reference/proposal",
                anchor: Some("details"),
            })
        );
    }

    #[test]
    fn compiler_parses_and_normalizes_reference_prefix() {
        let site = compile_site(
            [DocumentInput {
                source_path: "guide-1000.md".to_owned(),
                slug: "guide-1000".to_owned(),
                format: DocFormat::Markdown,
                content: "# Guide",
                is_start_page: false,
            }],
            CompileOptions {
                reference_prefix: " rfc ".to_owned(),
                ..CompileOptions::default()
            },
            None,
        )
        .unwrap();
        assert_eq!(site.reference_prefix, "RFC");
        assert_eq!(site.pages[0].reference.as_ref().unwrap().id, "RFC-1000");
    }

    #[test]
    fn ordered_formal_reference_keeps_its_clean_route_and_identity() {
        let site = compile_site(
            [DocumentInput {
                source_path: "esh/90.language-reference.3000.md".to_owned(),
                slug: SourceResolver::compute_slug(Path::new("esh/90.language-reference.3000.md")),
                format: DocFormat::Markdown,
                content: "# Language Reference",
                is_start_page: false,
            }],
            CompileOptions::default(),
            None,
        )
        .unwrap();
        assert_eq!(site.pages[0].slug, "esh/language-reference");
        assert_eq!(site.pages[0].reference.as_ref().unwrap().id, "REF-3000");
    }

    #[test]
    fn filename_order_precedes_unprefixed_pages_in_section_pagination_order() {
        let mut site = DocSiteAst {
            version: "1.0".to_owned(),
            title: None,
            reference_prefix: "REF".to_owned(),
            pages: vec![
                page("guides/unprefixed.md"),
                page("guides/20.second.md"),
                page("guides/00.index.md"),
                page("guides/10.first.md"),
            ],
        };
        ReferenceCatalog::prepare(&mut site).unwrap();
        assert_eq!(
            site.pages
                .iter()
                .map(|page| page.slug.as_str())
                .collect::<Vec<_>>(),
            [
                "guides",
                "guides/first",
                "guides/second",
                "guides/unprefixed"
            ]
        );
    }

    #[test]
    fn rejects_reference_cycles() {
        let mut first = page("first-1000.rst");
        first
            .metadata
            .insert("depends_on".to_owned(), serde_json::json!("REF-2000"));
        let mut second = page("second-2000.rst");
        second
            .metadata
            .insert("depends_on".to_owned(), serde_json::json!("REF-1000"));
        let mut site = DocSiteAst {
            version: "1.0".to_owned(),
            title: None,
            reference_prefix: "REF".to_owned(),
            pages: vec![first, second],
        };
        assert_eq!(
            ReferenceCatalog::prepare(&mut site).unwrap_err().code,
            "formal-reference-invalid"
        );
    }

    #[test]
    fn rewrites_aliases_in_nested_content() {
        let mut site = DocSiteAst {
            version: "1.0".to_owned(),
            title: None,
            reference_prefix: "REF".to_owned(),
            pages: vec![page("proposal-1000.rst"), page("guide.rst")],
        };
        ReferenceCatalog::prepare(&mut site).unwrap();
        site.pages[0].ast = vec![crate::ast::DocNode::Admonition {
            kind: "note".to_owned(),
            title: None,
            children: vec![crate::ast::DocNode::Paragraph {
                children: vec![InlineNode::Link {
                    href: "REF-1000#details".to_owned(),
                    title: None,
                    content: "proposal".to_owned(),
                }],
            }],
        }];
        rewrite_aliases_to_routes(&mut site);
        let crate::ast::DocNode::Admonition { children, .. } = &site.pages[0].ast[0] else {
            panic!("expected admonition")
        };
        let crate::ast::DocNode::Paragraph { children } = &children[0] else {
            panic!("expected paragraph")
        };
        let InlineNode::Link { href, .. } = &children[0] else {
            panic!("expected link")
        };
        assert_eq!(href, "proposal#details");
    }
}
