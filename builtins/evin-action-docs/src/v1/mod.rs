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

pub mod assets;
pub mod ast;
pub mod css;
mod debug;
pub mod favicon;
pub mod fs;
pub mod icons;
pub mod js_minification;
pub mod lsp;
pub mod mermaid;
pub mod presentation;
pub mod reference;
pub mod renderer;
pub mod search;
pub mod search_binary;
pub mod seo;
pub mod server;
pub mod start_page;
pub mod syntax;
pub mod tape_executor;
pub mod tape_fonts;
pub mod templates;
pub mod topbar;
pub mod validator;

pub mod metrics;
pub mod resolver;

use ast::{DocFormat, DocSiteAst};
use css::CssOptimizer;
use debug::DocsDebugStats;
use evin_action_sdk::{InvocationFlags, RunnerContext};
use fs::EmbeddedFs;
use include_dir::{Dir, include_dir};
use presentation::Presentation;
use renderer::HtmlRenderer;
pub use resolver::{
    DocResolver, ResolvedFavicon, ResolvedLogo, ResolvedResources, ResourceResolver,
};
use search::SearchIndexBuilder;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use topbar::{TopbarSetting, resolve as resolve_topbar};
use validator::{DiagramValidator, LinkValidator, MathValidator};

pub(crate) static LAYOUTS_DIR: Dir<'_> = include_dir!("builtins/evin-action-docs/layouts");

fn default_source_dir() -> String {
    "docs".to_string()
}
fn default_out() -> String {
    "dist/docs".to_string()
}
fn default_asset_dir() -> String {
    "assets".to_string()
}
fn default_theme() -> String {
    "matter".to_string()
}
fn default_bundle() -> String {
    "separated".to_string()
}
fn default_validate_links() -> bool {
    true
}
fn default_extract_frontmatter() -> bool {
    true
}
fn default_port() -> u16 {
    3000
}
fn default_reference_prefix() -> String {
    "REF".to_string()
}
fn default_language() -> String {
    "en".to_string()
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DocsMode {
    #[default]
    Build,
    Serve,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DocsTarget {
    #[default]
    Web,
    Lsp,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocsConfig {
    pub site_url: String,
    #[serde(default)]
    pub target: DocsTarget,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default)]
    pub mode: DocsMode,
    #[serde(default = "default_source_dir")]
    pub source_dir: String,
    #[serde(default = "default_asset_dir")]
    pub asset_dir: String,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default = "default_extract_frontmatter")]
    pub extract_frontmatter: bool,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default = "default_out")]
    pub out: String,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default)]
    pub theme_overrides: HashMap<String, String>,
    #[serde(default = "default_bundle")]
    pub bundle: String,
    #[serde(default = "default_validate_links")]
    pub validate_links: bool,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub host: bool,
    #[serde(default)]
    pub layout_dir: Option<String>,
    #[serde(default)]
    pub favicon: Option<String>,
    #[serde(default)]
    pub logo: Option<String>,
    #[serde(default)]
    pub extensions_dir: Option<String>,
    #[serde(default)]
    pub theme_color: Option<String>,
    #[serde(default)]
    pub background_color: Option<String>,
    #[serde(default)]
    pub icon_dir: Option<String>,
    #[serde(default)]
    pub minify: Option<bool>,
    #[serde(default = "default_reference_prefix")]
    pub reference_prefix: String,
    #[serde(default)]
    pub strict_filenames: bool,
    #[serde(default)]
    pub topbar: Option<TopbarSetting>,
    #[serde(default)]
    pub presentation: Option<Presentation>,
    #[serde(default)]
    pub diagnostic_search: bool,
    #[serde(default)]
    pub metrics: bool,
    #[serde(default)]
    pub metrics_budget_kb: Option<u32>,
    #[serde(default)]
    pub social_image: Option<String>,
    #[serde(default)]
    pub lastmod_fallback: bool,
}

impl Default for DocsConfig {
    fn default() -> Self {
        Self {
            site_url: String::new(),
            target: DocsTarget::Web,
            language: default_language(),
            mode: DocsMode::Build,
            source_dir: default_source_dir(),
            asset_dir: default_asset_dir(),
            include: Vec::new(),
            exclude: Vec::new(),
            extract_frontmatter: default_extract_frontmatter(),
            title: None,
            out: default_out(),
            theme: default_theme(),
            theme_overrides: HashMap::new(),
            bundle: default_bundle(),
            validate_links: default_validate_links(),
            port: default_port(),
            host: false,
            layout_dir: None,
            favicon: None,
            logo: None,
            extensions_dir: None,
            theme_color: None,
            background_color: None,
            icon_dir: None,
            minify: None,
            reference_prefix: default_reference_prefix(),
            strict_filenames: false,
            topbar: None,
            presentation: None,
            diagnostic_search: false,
            metrics: false,
            metrics_budget_kb: None,
            social_image: None,
            lastmod_fallback: false,
        }
    }
}

fn resolve_serve_host(config: &DocsConfig, flags: &InvocationFlags) -> server::ServeHost {
    server::ServeHost::from_exposed(config.host || flags.contains("host"))
}

impl DocsConfig {
    fn normalized_reference_prefix(&self) -> Result<String, String> {
        let prefix = self.reference_prefix.trim().to_ascii_uppercase();
        if prefix.is_empty()
            || !prefix
                .chars()
                .all(|character| character.is_ascii_alphanumeric())
        {
            return Err("reference_prefix must contain only ASCII letters and digits".to_string());
        }
        Ok(prefix)
    }

    fn validate_topbar(&self) -> Result<(), String> {
        self.topbar
            .as_ref()
            .map(|setting| setting.validate(Some(&self.theme)))
            .unwrap_or(Ok(()))
    }

    fn validate_presentation(&self) -> Result<(), String> {
        self.presentation
            .as_ref()
            .map(|setting| setting.validate(Some(&self.theme)))
            .unwrap_or(Ok(()))
    }
}

pub fn resolve_layout_assets(
    workspace_root: &Path,
    theme: &str,
    custom_layout_dir: Option<&str>,
) -> (String, String, String, EmbeddedFs<'static>) {
    let config = DocsConfig {
        theme: theme.to_string(),
        layout_dir: custom_layout_dir.map(str::to_string),
        ..DocsConfig::default()
    };
    if let Ok(resolver) = ResourceResolver::new(workspace_root, &config)
        && let Ok(resources) = resolver.resolve_all()
    {
        return (
            resources.shell_html,
            resources.raw_css,
            resources.raw_search_js,
            resources.embedded_fs,
        );
    }
    let custom_path = custom_layout_dir
        .map(PathBuf::from)
        .map(|path| {
            if path.is_absolute() {
                path
            } else {
                workspace_root.join(path)
            }
        })
        .unwrap_or_else(|| workspace_root.join(".evin/layouts").join(theme));
    let read_custom = |names: &[&str]| {
        names
            .iter()
            .map(|name| custom_path.join(name))
            .find(|path| path.is_file())
            .and_then(|path| std::fs::read_to_string(path).ok())
    };
    let embedded = |names: &[&str]| {
        names
            .iter()
            .find_map(|name| {
                LAYOUTS_DIR
                    .get_file(format!("{theme}/{name}"))
                    .or_else(|| LAYOUTS_DIR.get_file(format!("matter/{name}")))
                    .and_then(|file| file.contents_utf8())
                    .map(str::to_string)
            })
            .unwrap_or_default()
    };
    let shell_html = read_custom(&["shell.html"]).unwrap_or_else(|| embedded(&["shell.html"]));
    let raw_css = read_custom(&["theme.scss", "theme.css"])
        .unwrap_or_else(|| embedded(&["theme.scss", "theme.css"]));
    let raw_search_js =
        read_custom(&["app.js", "search.js"]).unwrap_or_else(|| embedded(&["app.js", "search.js"]));
    let custom_layout_path = custom_path.exists().then_some(custom_path);
    (
        shell_html,
        raw_css,
        raw_search_js,
        EmbeddedFs::new(&LAYOUTS_DIR, theme, custom_layout_path),
    )
}

pub fn validate_config(
    with: Option<&evin_flow::SpannedValue>,
) -> Result<(), evin_flow::ConfigError> {
    let config: DocsConfig = evin_action_sdk::parse_config(with)?;
    let validation = config
        .normalized_reference_prefix()
        .and_then(|_| config.validate_topbar())
        .and_then(|_| config.validate_presentation())
        .and_then(|_| {
            if config.asset_dir.contains("${") {
                Ok(())
            } else {
                assets::validate_asset_dir(&config.asset_dir)
            }
        })
        .and_then(|_| {
            if config.target == DocsTarget::Lsp || config.site_url.contains("${") {
                Ok(())
            } else if config.site_url.trim().is_empty() {
                Err("site_url is required when target is web".to_string())
            } else {
                seo::validate_site_settings(&config.site_url, &config.language).map(|_| ())
            }
        })
        .and_then(|_| {
            if config.metrics_budget_kb.is_some() && !config.metrics {
                Err("metrics_budget_kb requires metrics to be enabled".to_string())
            } else {
                Ok(())
            }
        });
    validation.map_err(|message| evin_flow::ConfigError {
        message,
        span: None,
    })
}

const METRICS_PAGE_KIND: &str = "metrics";

fn metadata_bool(page: &ast::DocPage, key: &str) -> bool {
    page.metadata.get(key).and_then(|value| value.as_bool()) == Some(true)
}

pub(crate) fn is_metrics_page(page: &ast::DocPage) -> bool {
    page.metadata
        .get("page_kind")
        .and_then(|value| value.as_str())
        == Some(METRICS_PAGE_KIND)
}

fn is_dev_only_page(page: &ast::DocPage) -> bool {
    metadata_bool(page, "dev_only")
}

fn directive_is_metric(name: &str) -> bool {
    name.trim().replace('_', "-").starts_with("metrics-")
}

fn page_uses_metric_directive(nodes: &[evin_doc::ast::DocNode]) -> bool {
    use evin_doc::ast::DocNode;
    nodes.iter().any(|node| match node {
        DocNode::Directive { name, body, .. } => {
            directive_is_metric(name) || page_uses_metric_directive(body)
        }
        DocNode::Admonition { children, .. } | DocNode::LspSymbol { children, .. } => {
            page_uses_metric_directive(children)
        }
        DocNode::List { items, .. } => items.iter().any(|item| page_uses_metric_directive(item)),
        DocNode::TaskList { items } => items
            .iter()
            .any(|item| page_uses_metric_directive(&item.body)),
        DocNode::DefinitionList { items } => items
            .iter()
            .any(|item| page_uses_metric_directive(&item.body)),
        DocNode::OptionList { items } => items
            .iter()
            .any(|item| page_uses_metric_directive(&item.body)),
        DocNode::FieldList { items } => items
            .iter()
            .any(|item| page_uses_metric_directive(&item.body)),
        DocNode::Stepper { steps } => steps
            .iter()
            .any(|step| page_uses_metric_directive(&step.body)),
        _ => false,
    })
}

pub(crate) fn metrics_page_route(
    site: &DocSiteAst,
    config: &DocsConfig,
) -> Result<Option<String>, evin_diagnostic::Diagnostic> {
    let mut metrics_pages = site.pages.iter().filter(|page| is_metrics_page(page));
    let Some(page) = metrics_pages.next() else {
        return Ok(None);
    };
    if metrics_pages.next().is_some() {
        return Err(evin_diagnostic::Diagnostic::new(
            "Only one page may declare page_kind: metrics",
            evin_diagnostic::IssueCode::new("invalid-metrics-page"),
        ));
    }
    if !is_dev_only_page(page) {
        return Err(evin_diagnostic::Diagnostic::new(
            "page_kind: metrics requires dev_only: true",
            evin_diagnostic::IssueCode::new("invalid-metrics-page"),
        ));
    }
    if !config.metrics {
        return Err(evin_diagnostic::Diagnostic::new(
            "page_kind: metrics requires metrics: true",
            evin_diagnostic::IssueCode::new("invalid-metrics-page"),
        ));
    }
    if config.mode != DocsMode::Serve {
        return Ok(None);
    }
    Ok(Some(format!("/{}", page.slug.trim_matches('/'))))
}

fn validate_metrics_pages(
    site: &DocSiteAst,
    config: &DocsConfig,
) -> Result<(), evin_diagnostic::Diagnostic> {
    let route = metrics_page_route(site, config)?;
    for page in &site.pages {
        if page_uses_metric_directive(&page.ast)
            && config.mode == DocsMode::Build
            && !is_metrics_page(page)
        {
            return Err(evin_diagnostic::Diagnostic::new(
                format!(
                    "Metric component directives are development-only; '{}' must declare page_kind: metrics and dev_only: true",
                    page.source_path
                ),
                evin_diagnostic::IssueCode::new("invalid-metrics-page"),
            ));
        }
    }
    let _ = route;
    Ok(())
}

pub(crate) fn parse_docs(
    workspace_root: &Path,
    config: &DocsConfig,
    logger: &evin_action_sdk::Logger,
    stats: &mut DocsDebugStats,
) -> Result<DocSiteAst, evin_diagnostic::Diagnostic> {
    let source_root = PathBuf::from(&config.source_dir);
    let source_root = if source_root.is_absolute() {
        source_root
    } else {
        workspace_root.join(source_root)
    };
    if !source_root.exists() {
        evin_action_sdk::action_event!(
            logger,
            evin_action_sdk::activity::DocsSourceMissing {
                source: source_root.display().to_string(),
            }
        );
    }
    evin_action_sdk::action_event!(
        logger,
        evin_action_sdk::activity::DocsScanStarted {
            source: source_root.display().to_string(),
        }
    );
    stats.scans += 1;
    let mut read_failures = 0;
    let sources = DocResolver::resolve_all(&source_root, &config.include, &config.exclude)
        .map_err(doc_issue_to_diagnostic)?
        .into_iter()
        .filter_map(|resolved| {
            let content = match std::fs::read_to_string(&resolved.absolute_path) {
                Ok(content) => content,
                Err(error) => {
                    read_failures += 1;
                    evin_action_sdk::action_event!(
                        logger,
                        evin_action_sdk::activity::DocsReadFailed {
                            source: resolved.relative_path.display().to_string(),
                            error: error.to_string(),
                        }
                    );
                    return None;
                }
            };
            Some((resolved, content))
        })
        .collect::<Vec<_>>();
    let inputs = sources
        .iter()
        .map(|(resolved, content)| evin_doc::DocumentInput {
            slug: resolved.slug.clone(),
            source_path: resolved.relative_path.to_string_lossy().to_string(),
            format: if resolved.is_rst {
                DocFormat::ReStructuredText
            } else {
                DocFormat::Markdown
            },
            is_start_page: matches!(
                resolved.relative_path.to_str(),
                Some("index.md") | Some("index.rst")
            ),
            content,
        })
        .collect::<Vec<_>>();
    stats.read_failures += read_failures;
    let site = evin_doc::compile_site(
        inputs,
        evin_doc::CompileOptions {
            extract_frontmatter: config.extract_frontmatter,
            reference_prefix: config.normalized_reference_prefix().map_err(|message| {
                evin_diagnostic::Diagnostic::new(
                    message,
                    evin_diagnostic::IssueCode::new("invalid-config"),
                )
            })?,
            site_version: "1.0".to_string(),
        },
        config.title.clone(),
    )
    .map_err(doc_issue_to_diagnostic)?;
    if config.strict_filenames {
        validate_strict_filenames(&site).map_err(doc_issue_to_diagnostic)?;
    }
    validate_metrics_pages(&site, config)?;
    stats.pages = site.pages.len();
    stats.rst = site
        .pages
        .iter()
        .filter(|page| page.format == DocFormat::ReStructuredText)
        .count();
    stats.markdown = site.pages.len().saturating_sub(stats.rst);
    stats.references = 0;
    stats.emit_scan(logger);
    stats.references = site
        .pages
        .iter()
        .filter(|page| page.reference.is_some())
        .count();
    stats.emit_scan(logger);
    Ok(site)
}

fn doc_issue_to_diagnostic(issue: evin_doc::DocIssue) -> evin_diagnostic::Diagnostic {
    let mut diagnostic = evin_diagnostic::Diagnostic::new(
        issue.message,
        evin_diagnostic::IssueCode::new(issue.code),
    );
    if issue.severity == evin_doc::DocSeverity::Warning {
        diagnostic = diagnostic.with_severity(evin_diagnostic::Severity::Warning);
    }
    if let Some(source_path) = issue.source_path {
        diagnostic = diagnostic.with_file_location(source_path, None, None);
    }
    diagnostic
}

fn validate_strict_filenames(site: &DocSiteAst) -> Result<(), evin_doc::DocIssue> {
    let filename =
        regex::Regex::new(r"^([0-9]+)\.([a-z0-9]+(?:-[a-z0-9]+)*)(?:\.([1-9][0-9]{3,}))?$")
            .expect("valid strict documentation filename pattern");
    let segment = regex::Regex::new(r"^[a-z0-9]+(?:-[a-z0-9]+)*$")
        .expect("valid strict documentation section pattern");
    let mut orders = HashMap::<(String, u64), String>::new();

    for page in &site.pages {
        if page.source_path == "index.md" {
            if page.reference.is_some() {
                return Err(strict_filename_error(
                    &page.source_path,
                    "The root start page cannot be a formal reference",
                ));
            }
            continue;
        }

        let path = Path::new(&page.source_path);
        let extension = path.extension().and_then(|value| value.to_str());
        if !matches!(extension, Some("md") | Some("rst")) {
            return Err(strict_filename_error(
                &page.source_path,
                "Strict documentation filenames must use the .md or .rst extension",
            ));
        }
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty());
        if parent.is_none() && !(is_metrics_page(page) && is_dev_only_page(page)) {
            return Err(strict_filename_error(
                &page.source_path,
                "Only the root start page may live outside a documentation section",
            ));
        }
        let section = parent
            .map(|parent| parent.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        if let Some(parent) = parent {
            for component in parent.components() {
                let seg_str = component.as_os_str().to_string_lossy();
                let seg_stem = seg_str
                    .split_once('.')
                    .map(|(_, name)| name)
                    .unwrap_or(&seg_str);
                if !segment.is_match(seg_stem) {
                    return Err(strict_filename_error(
                        &page.source_path,
                        "Strict documentation sections must use lowercase kebab-case directory names",
                    ));
                }
            }
        }
        let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
            return Err(strict_filename_error(
                &page.source_path,
                "Documentation filename is not valid UTF-8",
            ));
        };
        let Some(captures) = filename.captures(stem) else {
            return Err(strict_filename_error(
                &page.source_path,
                "Use <order>.<kebab-case-slug>[.<reference-digits>].<md|rst>",
            ));
        };
        let order_text = &captures[1];
        let order = order_text.parse::<u64>().map_err(|_| {
            strict_filename_error(&page.source_path, "Navigation order is too large")
        })?;
        let slug = &captures[2];
        let reference_digits = captures.get(3).map(|capture| capture.as_str());
        let is_section_index = slug == "index";
        if is_section_index != (order == 0)
            || (is_section_index && (order_text != "00" || reference_digits.is_some()))
        {
            return Err(strict_filename_error(
                &page.source_path,
                "Section overviews must be named exactly 00.index.md or 00.index.rst; order zero is reserved for them",
            ));
        }
        if let Some(previous) = orders.insert((section.clone(), order), page.source_path.clone()) {
            return Err(strict_filename_error(
                &page.source_path,
                format!(
                    "Navigation order {order} in section '{section}' is already used by '{previous}'"
                ),
            ));
        }
        match (reference_digits, page.reference.as_ref()) {
            (Some(digits), Some(reference)) if digits == reference.digits => {}
            (Some(_), Some(_)) => {
                return Err(strict_filename_error(
                    &page.source_path,
                    "Formal reference filename digits do not match the compiled reference identifier",
                ));
            }
            (Some(_), None) => {
                return Err(strict_filename_error(
                    &page.source_path,
                    "A .NNNN filename suffix must compile as a formal reference",
                ));
            }
            (None, Some(_)) => {
                return Err(strict_filename_error(
                    &page.source_path,
                    "Formal references must end with .NNNN before the file extension",
                ));
            }
            (None, None) => {}
        }
    }
    Ok(())
}

fn strict_filename_error(source_path: &str, message: impl Into<String>) -> evin_doc::DocIssue {
    evin_doc::DocIssue::error("invalid-filename", message).with_source_path(source_path)
}

pub(crate) fn source_fingerprint(
    workspace_root: &Path,
    config: &DocsConfig,
) -> Vec<(PathBuf, Option<std::time::SystemTime>, u64)> {
    let source_root = PathBuf::from(&config.source_dir);
    let source_root = if source_root.is_absolute() {
        source_root
    } else {
        workspace_root.join(source_root)
    };
    let mut files: Vec<_> =
        DocResolver::resolve_all(&source_root, &config.include, &config.exclude)
            .unwrap_or_default()
            .into_iter()
            .map(|file| {
                let metadata = std::fs::metadata(&file.absolute_path).ok();
                (
                    file.relative_path,
                    metadata.as_ref().and_then(|meta| meta.modified().ok()),
                    metadata.map(|meta| meta.len()).unwrap_or_default(),
                )
            })
            .collect();
    files.extend(
        assets::fingerprint_files(&source_root, &config.asset_dir)
            .into_iter()
            .map(|absolute_path| {
                let metadata = std::fs::metadata(&absolute_path).ok();
                let relative_path = absolute_path
                    .strip_prefix(&source_root)
                    .unwrap_or(&absolute_path)
                    .to_path_buf();
                (
                    relative_path,
                    metadata.as_ref().and_then(|meta| meta.modified().ok()),
                    metadata.map(|meta| meta.len()).unwrap_or_default(),
                )
            }),
    );
    files.sort_by(|left, right| left.0.cmp(&right.0));
    files
}

fn default_icons() -> HashSet<String> {
    [
        "description",
        "folder_open",
        "search",
        "terminal",
        "code",
        "copy",
        "check",
        "warning",
        "info",
        "lightbulb",
        "alert-circle",
        "alert-octagon",
        "external_link",
        "layout-sidebar",
        "layout-sidebar-left-collapse",
        "layout-sidebar-left-expand",
        "x",
        "list-details",
        "brightness-up",
        "moon",
        "sun-moon",
        "arrow-left",
        "arrow-right",
        "heading",
        "link",
        "zoom-in",
        "zoom-out",
        "refresh",
        "download",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

pub(crate) fn render_docs(
    workspace_root: &Path,
    config: &DocsConfig,
    site_ast: &DocSiteAst,
    logger: &evin_action_sdk::Logger,
    stats: &mut DocsDebugStats,
) -> Result<PathBuf, evin_diagnostic::Diagnostic> {
    let static_site;
    let site_ast = if config.mode == DocsMode::Build {
        static_site = DocSiteAst {
            pages: site_ast
                .pages
                .iter()
                .filter(|page| !is_dev_only_page(page))
                .cloned()
                .collect(),
            ..site_ast.clone()
        };
        &static_site
    } else {
        site_ast
    };
    let write_failures_before = stats.write_failures;
    stats.renders += 1;
    stats.bundle = config.bundle.clone();
    stats.pages_written = 0;
    stats.assets_written = 0;
    let git_resolver = |source_path: &str| -> Option<String> {
        let rel_path = Path::new(&config.source_dir).join(source_path);
        let output = std::process::Command::new("git")
            .args(["log", "-1", "--format=%cs", "--"])
            .arg(&rel_path)
            .current_dir(workspace_root)
            .output()
            .ok()?;
        if output.status.success() {
            let date_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !date_str.is_empty() {
                return Some(date_str);
            }
        }
        None
    };
    let seo_options = seo::SeoBuildOptions {
        git_lastmod_resolver: Some(&git_resolver),
        lastmod_fallback: config.lastmod_fallback,
    };
    let seo = seo::SeoCatalog::build_with_options(
        site_ast,
        &config.site_url,
        &config.language,
        seo_options,
    )
    .inspect_err(|_error| {
        stats.emit_output(logger);
    })?;
    let site_index = evin_doc::SiteIndex::build(site_ast).map_err(doc_issue_to_diagnostic)?;
    let has_subpath_warning = config.mode == DocsMode::Build && seo.site_url.path() != "/";
    let discovery_files = seo.discovery_files(config.mode).inspect_err(|_error| {
        stats.emit_output(logger);
    })?;
    let source_root = PathBuf::from(&config.source_dir);
    let source_root = if source_root.is_absolute() {
        source_root
    } else {
        workspace_root.join(source_root)
    };
    let resolved_assets = assets::ResolvedAssets::build(&source_root, &config.asset_dir, site_ast)
        .inspect_err(|_error| {
            stats.emit_output(logger);
        })?;
    let mut rendered_site = site_ast.clone();
    tape_executor::compile_site(&mut rendered_site, workspace_root).map_err(|message| {
        evin_diagnostic::Diagnostic::new(
            message.clone(),
            evin_diagnostic::IssueCode::new("issue-docs-tape-failed").arg("error", message),
        )
    })?;
    mermaid::compile_site(&mut rendered_site).map_err(|message| {
        evin_diagnostic::Diagnostic::new(
            message.clone(),
            evin_diagnostic::IssueCode::new("issue-docs-mermaid-failed").arg("error", message),
        )
    })?;
    let mut start_page = start_page::transform_with_assets_and_index(
        &rendered_site,
        &site_index,
        &source_root,
        Some(&resolved_assets),
    )
    .inspect_err(|_error| {
        stats.emit_output(logger);
    })?;
    let presentation = start_page
        .presentation
        .as_ref()
        .or(config.presentation.as_ref());
    if let Some(setting) = presentation {
        setting.validate(Some(&config.theme)).map_err(|message| {
            evin_diagnostic::Diagnostic::new(
                format!("Invalid presentation configuration: {message}"),
                evin_diagnostic::IssueCode::new("invalid-config"),
            )
        })?;
    }
    let topbar_setting = start_page.topbar.as_ref().or(config.topbar.as_ref());
    if let Some(setting) = topbar_setting {
        setting.validate(Some(&config.theme)).map_err(|message| {
            evin_diagnostic::Diagnostic::new(
                format!("Invalid topbar configuration: {message}"),
                evin_diagnostic::IssueCode::new("invalid-config"),
            )
        })?;
    }
    let topbar = resolve_topbar(
        start_page.topbar.as_ref(),
        config.topbar.as_ref(),
        presentation,
    )
    .map_err(|message| {
        evin_diagnostic::Diagnostic::new(
            format!("Invalid topbar configuration: {message}"),
            evin_diagnostic::IssueCode::new("invalid-config"),
        )
    })?;
    if let Some(topbar) = &topbar {
        topbar.validate(Some(&config.theme)).map_err(|message| {
            evin_diagnostic::Diagnostic::new(
                format!("Invalid topbar configuration: {message}"),
                evin_diagnostic::IssueCode::new("invalid-config"),
            )
        })?;
    }
    if let Some(spotlight) = &start_page.spotlight {
        let content = spotlight
            .content
            .resolve_variant(presentation)
            .map_err(|message| {
                evin_diagnostic::Diagnostic::new(
                    format!("Invalid spotlight configuration: {message}"),
                    evin_diagnostic::IssueCode::new("invalid-config"),
                )
            })?;
        content.validate(Some(&config.theme)).map_err(|message| {
            evin_diagnostic::Diagnostic::new(
                format!("Invalid spotlight configuration: {message}"),
                evin_diagnostic::IssueCode::new("invalid-config"),
            )
        })?;
        start_page.spotlight = Some(start_page::StartPageSpotlight {
            after_sections: spotlight.after_sections,
            content,
        });
    }
    if config.validate_links {
        evin_action_sdk::action_event!(logger, evin_action_sdk::activity::DocsAuditStarted {});
        let audit =
            LinkValidator::validate_with_assets_and_index(site_ast, &site_index, &resolved_assets);
        stats.link_audits += 1;
        stats.links = audit.links_checked;
        stats.anchors = audit.anchors_indexed;
        stats.link_issues = audit.diagnostics.len();
        for diagnostic in audit.diagnostics {
            evin_action_sdk::action_event!(
                logger,
                evin_action_sdk::activity::DocsLinkIssue {
                    issue: diagnostic.title,
                }
            );
        }
    } else {
        stats.links = 0;
        stats.anchors = 0;
        stats.link_issues = 0;
        let content_diagnostics = MathValidator::validate(site_ast)
            .into_iter()
            .chain(DiagramValidator::validate(site_ast));
        for diagnostic in content_diagnostics {
            evin_action_sdk::action_event!(
                logger,
                evin_action_sdk::activity::DocsLinkIssue {
                    issue: diagnostic.title,
                }
            );
        }
    }
    stats.emit_links(logger, config.validate_links);
    let sitemap_files = discovery_files
        .keys()
        .filter(|name| name.starts_with("sitemap") && name.ends_with(".xml"))
        .count();
    let reported_indexable = if config.mode == DocsMode::Serve {
        0
    } else {
        seo.indexable_pages
    };
    let warning_count = seo.quality.total() + usize::from(has_subpath_warning);
    stats.seo_audits += 1;
    stats.indexable = reported_indexable;
    stats.excluded = site_ast.pages.len().saturating_sub(reported_indexable);
    stats.sitemap_files = sitemap_files;
    stats.missing_descriptions = seo.quality.missing_descriptions;
    stats.long_titles = seo.quality.long_titles;
    stats.description_length = seo.quality.description_length;
    stats.duplicate_titles = seo.quality.duplicate_titles;
    stats.duplicate_descriptions = seo.quality.duplicate_descriptions;
    stats.deployment_warnings = usize::from(has_subpath_warning);
    debug_assert_eq!(
        warning_count,
        seo.quality.total() + stats.deployment_warnings
    );
    stats.emit_discovery(logger);
    let out = PathBuf::from(&config.out);
    let out_dir = if out.is_absolute() {
        out
    } else {
        workspace_root.join(out)
    };
    std::fs::create_dir_all(&out_dir).map_err(|error| {
        stats.write_failures += 1;
        stats.emit_output(logger);
        evin_diagnostic::Diagnostic::new(
            format!(
                "Failed to create output directory '{}': {error}",
                out_dir.display()
            ),
            evin_diagnostic::IssueCode::new("io-error"),
        )
    })?;
    let resolver = ResourceResolver::new(workspace_root, config)?;
    let resources = resolver.resolve_all()?;
    for diag in &resources.diagnostics {
        evin_action_sdk::action_event!(
            logger,
            evin_action_sdk::activity::DocsCssFallback {
                error: diag.title.clone(),
            }
        );
    }
    let shell_html = resources.shell_html;
    let raw_css = resources.raw_css;
    let raw_search_js = resources.raw_search_js;
    let embedded_fs = resources.embedded_fs;
    renderer::validate_topbar_shell(&shell_html, topbar.is_some()).map_err(|message| {
        evin_diagnostic::Diagnostic::new(
            format!("Invalid topbar layout: {message}"),
            evin_diagnostic::IssueCode::new("invalid-layout"),
        )
    })?;
    let registry = templates::ComponentRegistry::load_from_layout(
        &LAYOUTS_DIR,
        &config.theme,
        resources.layout_dir.as_deref(),
    );
    let mut site_used_components =
        HtmlRenderer::collect_site_used_components(&rendered_site, Some(&start_page));
    if topbar.is_some() || start_page.spotlight.is_some() {
        site_used_components.insert("presentation".to_string());
    }
    let active_components_scss = registry.collect_css(&site_used_components);
    let active_components_js = registry.collect_js(&site_used_components);
    let mut presentation_variants = BTreeSet::new();
    if let Some(topbar) = &topbar {
        presentation_variants.insert(topbar.variant.clone());
    }
    if let Some(spotlight) = &start_page.spotlight {
        presentation_variants.insert(spotlight.content.variant.clone());
    }
    let raw_css = topbar::configure_matter_scss(
        &raw_css,
        &config.theme,
        &presentation_variants,
        topbar.is_some(),
        start_page.spotlight.is_some(),
    );

    let is_production = config.minify.unwrap_or(config.mode == DocsMode::Build);
    let single_file = config.bundle == "single-file";
    let mut processed_css = CssOptimizer::process_with_components(
        &raw_css,
        &active_components_scss,
        &config.theme_overrides,
        &embedded_fs,
        is_production,
    )
    .unwrap_or_else(|error| {
        evin_action_sdk::action_event!(
            logger,
            evin_action_sdk::activity::DocsCssFallback {
                error: error.to_string(),
            }
        );
        raw_css.clone()
    });
    processed_css.push_str(&tape_fonts::css(single_file));
    let serve_enabled = config.mode == DocsMode::Serve;
    let site_title = site_ast.title.as_deref().unwrap_or("Documentation");
    let (favicon_assets, _) = favicon::FaviconPipeline::process(
        Some(&resources.favicon),
        site_title,
        config.theme_color.as_deref(),
        config.background_color.as_deref(),
        "./",
    );
    let favicon_asset_count = favicon_assets.len();
    for (path, contents) in favicon_assets {
        let destination = out_dir.join(path);
        if let Some(parent) = destination.parent()
            && std::fs::create_dir_all(parent).is_err()
        {
            stats.write_failures += 1;
        }
        if std::fs::write(destination, contents).is_err() {
            stats.write_failures += 1;
        }
    }
    let mut requested_icons = default_icons();
    if site_used_components.contains("tape") {
        requested_icons.extend(["player-play", "player-pause"].map(str::to_string));
    }
    let sprite_svg = icons::IconSpriteBuilder::build_sprite_sheet(
        &requested_icons,
        resources.icons_dir.as_deref(),
        None,
    );
    let sprite_dest = out_dir.join("assets/icons.svg");
    if let Some(parent) = sprite_dest.parent()
        && std::fs::create_dir_all(parent).is_err()
    {
        stats.write_failures += 1;
    }
    if std::fs::write(sprite_dest, &sprite_svg).is_err() {
        stats.write_failures += 1;
    }
    let raw_combined_js = if active_components_js.is_empty() {
        raw_search_js
    } else {
        format!("{raw_search_js}\n{active_components_js}")
    };
    let app_js = if is_production {
        js_minification::minify_js(&raw_combined_js)
    } else {
        raw_combined_js
    };
    let render_context = renderer::PreparedRenderContext::new(
        &rendered_site,
        site_index,
        &resolved_assets,
        &shell_html,
        &processed_css,
        &app_js,
        &sprite_svg,
        &registry,
        single_file,
        serve_enabled,
        Some(&resources.logo),
        Some(resources.favicon),
        config.theme_color.as_deref(),
        config.background_color.as_deref(),
        Some(&start_page),
        topbar.as_ref(),
        &seo,
        config.social_image.as_deref(),
    );
    let rendered_pages = HtmlRenderer::render_prepared(&render_context);
    prune_stale_page_outputs(&out_dir, &rendered_pages)?;
    for (path, contents) in rendered_pages {
        let destination = out_dir.join(path);
        if let Some(parent) = destination.parent()
            && std::fs::create_dir_all(parent).is_err()
        {
            stats.write_failures += 1;
        }
        std::fs::write(&destination, contents).map_err(|error| {
            stats.write_failures += 1;
            stats.emit_output(logger);
            evin_diagnostic::Diagnostic::new(
                format!("Failed to write page '{}': {error}", destination.display()),
                evin_diagnostic::IssueCode::new("io-error"),
            )
        })?;
    }
    if !single_file {
        let assets = out_dir.join("assets");
        if std::fs::create_dir_all(&assets).is_err() {
            stats.write_failures += 1;
        }
        if std::fs::write(assets.join("theme.css"), processed_css).is_err() {
            stats.write_failures += 1;
        }
        if tape_fonts::publish(&out_dir).is_err() {
            stats.write_failures += 1;
        }
        if std::fs::write(assets.join("app.js"), app_js).is_err() {
            stats.write_failures += 1;
        }
        let search_payload = SearchIndexBuilder::build(site_ast, start_page.search_prompts.clone());
        let _ = std::fs::remove_file(assets.join("search.bin"));
        if std::fs::write(
            assets.join("search-catalog.bin"),
            search_payload.to_catalog_binary(),
        )
        .is_err()
        {
            stats.write_failures += 1;
        }
        if std::fs::write(
            assets.join("search-index.bin"),
            search_payload.to_index_binary(),
        )
        .is_err()
        {
            stats.write_failures += 1;
        }
        if config.diagnostic_search {
            if let Ok(json) = serde_json::to_string(&search_payload)
                && std::fs::write(out_dir.join("search.json"), json).is_err()
            {
                stats.write_failures += 1;
            }
        } else {
            let _ = std::fs::remove_file(out_dir.join("search.json"));
        }
    }
    if single_file && tape_fonts::remove_published(&out_dir).is_err() {
        stats.write_failures += 1;
    }
    if let Ok(entries) = std::fs::read_dir(&out_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if (name == "sitemap.xml" || name.starts_with("sitemap-") && name.ends_with(".xml"))
                && !discovery_files.contains_key(name.as_ref())
            {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    for (name, contents) in &discovery_files {
        std::fs::write(out_dir.join(name), contents).map_err(|error| {
            stats.write_failures += 1;
            stats.emit_output(logger);
            evin_diagnostic::Diagnostic::new(
                format!("Failed to write SEO artifact '{}': {error}", name),
                evin_diagnostic::IssueCode::new("io-error"),
            )
        })?;
    }
    let published_media = resolved_assets.write_to(&out_dir).inspect_err(|_error| {
        stats.write_failures += 1;
        stats.emit_output(logger);
    })?;
    stats.pages_written = site_ast.pages.len();
    let expected_assets = favicon_asset_count
        + 1
        + if single_file { 0 } else { 4 }
        + if !single_file {
            tape_fonts::FONTS.len()
        } else {
            0
        }
        + discovery_files.len()
        + published_media;
    stats.assets_written =
        expected_assets.saturating_sub(stats.write_failures.saturating_sub(write_failures_before));
    stats.emit_output(logger);
    if config.mode == DocsMode::Serve {
        evin_action_sdk::action_event!(logger, evin_action_sdk::activity::DocsReady {});
    } else {
        evin_action_sdk::action_event!(
            logger,
            evin_action_sdk::activity::DocsGenerated {
                output: out_dir.display().to_string(),
            }
        );
    }
    Ok(out_dir)
}

fn prune_stale_page_outputs(
    out_dir: &Path,
    rendered_pages: &HashMap<String, String>,
) -> Result<(), evin_diagnostic::Diagnostic> {
    let expected = rendered_pages
        .keys()
        .map(|path| out_dir.join(path))
        .collect::<HashSet<_>>();
    let mut directories = vec![out_dir.to_path_buf()];
    let mut visited_directories = Vec::new();

    while let Some(directory) = directories.pop() {
        visited_directories.push(directory.clone());
        let entries = std::fs::read_dir(&directory).map_err(|error| {
            evin_diagnostic::Diagnostic::new(
                format!(
                    "Failed to read documentation output '{}': {error}",
                    directory.display()
                ),
                evin_diagnostic::IssueCode::new("io-error"),
            )
        })?;
        for entry in entries {
            let entry = entry.map_err(|error| {
                evin_diagnostic::Diagnostic::new(
                    format!("Failed to read documentation output entry: {error}"),
                    evin_diagnostic::IssueCode::new("io-error"),
                )
            })?;
            let path = entry.path();
            if entry
                .file_type()
                .map(|file_type| file_type.is_dir())
                .unwrap_or(false)
            {
                directories.push(path);
            } else if path.file_name().is_some_and(|name| name == "index.html")
                && !expected.contains(&path)
            {
                std::fs::remove_file(&path).map_err(|error| {
                    evin_diagnostic::Diagnostic::new(
                        format!("Failed to prune stale page '{}': {error}", path.display()),
                        evin_diagnostic::IssueCode::new("io-error"),
                    )
                })?;
            }
        }
    }
    visited_directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for directory in visited_directories {
        if directory != out_dir {
            let _ = std::fs::remove_dir(&directory);
        }
    }
    Ok(())
}

pub async fn execute(
    ctx: &RunnerContext,
    with: Option<evin_flow::SpannedValue>,
) -> Result<(), evin_diagnostic::Diagnostic> {
    execute_with_invocation(ctx, with, &InvocationFlags::default()).await
}

pub async fn execute_with_invocation(
    ctx: &RunnerContext,
    with: Option<evin_flow::SpannedValue>,
    invocation_flags: &InvocationFlags,
) -> Result<(), evin_diagnostic::Diagnostic> {
    let started = std::time::Instant::now();
    let config: DocsConfig = ctx.load_config(with.as_ref()).map_err(|error| {
        evin_diagnostic::Diagnostic::new(
            format!("Invalid docs configuration: {}", error.message),
            evin_diagnostic::IssueCode::new("invalid-config"),
        )
    })?;
    let workspace_root = ctx.fs.workspace_root();
    let mut stats = DocsDebugStats::default();
    let site_ast = parse_docs(workspace_root, &config, &ctx.log, &mut stats)?;
    if config.target == DocsTarget::Lsp {
        let out_file = lsp::build_lsp_catalog(&ctx.fs, &config, &site_ast, &ctx.log)?;
        ctx.state
            .set_output("output_file", &out_file.to_string_lossy());
        ctx.state
            .set_output("symbol_count", &site_ast.pages.len().to_string());
        return Ok(());
    }
    if ctx.mode == evin_action_sdk::PipelineMode::DryRun {
        let out_dir = if Path::new(&config.out).is_absolute() {
            PathBuf::from(&config.out)
        } else {
            workspace_root.join(&config.out)
        };
        ctx.log.plan(evin_action_sdk::activity::DocsPlanBuild {
            output: out_dir.display().to_string(),
        });
        if config.mode == DocsMode::Serve {
            ctx.log.plan(evin_action_sdk::activity::DocsPlanServer {});
        }
        ctx.state
            .set_output("output_dir", &out_dir.to_string_lossy());
        ctx.state
            .set_output("page_count", &site_ast.pages.len().to_string());
        ctx.mark_unverified("Documentation rendering was skipped in dry-run mode");
        return Ok(());
    }
    let out_dir = render_docs(workspace_root, &config, &site_ast, &ctx.log, &mut stats)?;
    let metrics_route = metrics_page_route(&site_ast, &config)?;
    ctx.state
        .set_output("output_dir", &out_dir.to_string_lossy());
    ctx.state
        .set_output("page_count", &site_ast.pages.len().to_string());
    if config.metrics && config.mode == DocsMode::Build {
        let report = metrics::generate_report(&out_dir, &config).map_err(|e| {
            evin_diagnostic::Diagnostic::new(
                format!("Failed to collect documentation metrics: {e}"),
                evin_diagnostic::IssueCode::new("issue-docs-metrics-generation-failed"),
            )
        })?;
        metrics::write_report(&out_dir, &report).map_err(|e| {
            evin_diagnostic::Diagnostic::new(
                format!("Failed to write metrics.json: {e}"),
                evin_diagnostic::IssueCode::new("issue-docs-metrics-write-failed"),
            )
        })?;
        if report.budget_status == metrics::BudgetStatus::Fail {
            let limit = report.frame0.budget_limit_bytes.unwrap_or(0);
            return Err(evin_diagnostic::Diagnostic::new(
                format!(
                    "Documentation Frame-0 critical landing path size ({} B) exceeded configured budget ({} B)",
                    report.frame0.gzip_bytes, limit
                ),
                evin_diagnostic::IssueCode::new("issue-docs-metrics-budget-exceeded"),
            ));
        }
    }
    if config.mode == DocsMode::Serve {
        let host = resolve_serve_host(&config, invocation_flags);
        let binding = server::DevServer::bind(host, config.port).await?;
        ctx.state.set_output("server_url", &binding.url());
        server::DevServer::start(
            binding,
            out_dir,
            workspace_root.to_path_buf(),
            config,
            ctx.log.clone(),
            ctx.cancel_token.clone(),
            started,
            stats,
            metrics_route,
        )
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
