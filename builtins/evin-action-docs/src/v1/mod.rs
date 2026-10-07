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
    with: Option<&evin_yaml::SpannedValue>,
) -> Result<(), evin_yaml::ConfigError> {
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
    validation.map_err(|message| evin_yaml::ConfigError {
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
    with: Option<evin_yaml::SpannedValue>,
) -> Result<(), evin_diagnostic::Diagnostic> {
    execute_with_invocation(ctx, with, &InvocationFlags::default()).await
}

pub async fn execute_with_invocation(
    ctx: &RunnerContext,
    with: Option<evin_yaml::SpannedValue>,
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
mod tests {
    use super::*;

    #[test]
    fn defaults_are_build_friendly() {
        let config = DocsConfig::default();
        assert_eq!(config.mode, DocsMode::Build);
        assert!(!config.host);
        assert_eq!(config.source_dir, "docs");
        assert_eq!(config.asset_dir, "assets");
        assert_eq!(config.out, "dist/docs");
        assert_eq!(config.theme, "matter");
        assert!(!config.strict_filenames);
        assert!(config.minify.unwrap_or(config.mode == DocsMode::Build));
    }

    #[test]
    fn strict_filenames_require_the_public_documentation_contract() {
        let page = |source_path: &str, reference: Option<evin_doc::FormalReference>| ast::DocPage {
            slug: source_path.to_string(),
            source_path: source_path.to_string(),
            format: DocFormat::Markdown,
            metadata: HashMap::new(),
            reference,
            toc: vec![],
            ast: vec![],
        };
        let reference = evin_doc::FormalReference {
            id: "REF-4500".to_string(),
            digits: "4500".to_string(),
            number: 4500,
            depends_on: vec![],
            referenced_by: vec![],
        };
        let valid = DocSiteAst {
            version: "1.0".to_string(),
            title: None,
            reference_prefix: "REF".to_string(),
            pages: vec![
                page("index.md", None),
                page("guides/00.index.md", None),
                page("guides/10.workspaces.md", None),
                page(
                    "reference/20.workspace-configuration.4500.md",
                    Some(reference),
                ),
            ],
        };
        assert!(validate_strict_filenames(&valid).is_ok());

        let duplicate = DocSiteAst {
            pages: vec![
                page("guides/00.index.md", None),
                page("guides/01.first.md", None),
                page("guides/1.second.md", None),
            ],
            ..valid.clone()
        };
        assert_eq!(
            validate_strict_filenames(&duplicate).unwrap_err().code,
            "invalid-filename"
        );

        let legacy_reference = DocSiteAst {
            pages: vec![page(
                "reference/20.workspace-configuration-4500.md",
                Some(evin_doc::FormalReference {
                    id: "REF-4500".to_string(),
                    digits: "4500".to_string(),
                    number: 4500,
                    depends_on: vec![],
                    referenced_by: vec![],
                }),
            )],
            ..valid
        };
        assert_eq!(
            validate_strict_filenames(&legacy_reference)
                .unwrap_err()
                .code,
            "invalid-filename"
        );
    }

    #[test]
    fn site_url_is_required_and_site_settings_are_validated() {
        let missing = evin_yaml::parse_yaml("title: Docs\n").unwrap();
        assert!(evin_yaml::de::from_spanned_value::<DocsConfig>(missing).is_err());

        let invalid =
            evin_yaml::parse_yaml("site_url: ftp://example.com/docs\nlanguage: not_a_language\n")
                .unwrap();
        let error = validate_config(Some(&invalid)).unwrap_err();
        assert!(error.message.contains("site_url"));
    }

    #[test]
    fn site_url_template_is_validated_after_value_resolution() {
        let templated = evin_yaml::parse_yaml("site_url: ${docs_site_url}\n").unwrap();
        assert!(validate_config(Some(&templated)).is_ok());
    }

    #[test]
    fn serve_mode_defaults_to_unminified_assets() {
        let config = DocsConfig {
            mode: DocsMode::Serve,
            ..DocsConfig::default()
        };
        assert!(!config.minify.unwrap_or(config.mode == DocsMode::Build));
    }

    #[test]
    fn serve_host_accepts_yaml_and_routed_invocation_override() {
        let yaml = "site_url: https://example.com/\nhost: true\n";
        let value = evin_yaml::parse_yaml(yaml).unwrap();
        let configured = evin_yaml::de::from_spanned_value::<DocsConfig>(value).unwrap();
        assert!(configured.host);
        assert_eq!(
            resolve_serve_host(&configured, &InvocationFlags::default()),
            server::ServeHost::AllInterfaces
        );

        let default = DocsConfig::default();
        assert_eq!(
            resolve_serve_host(&default, &InvocationFlags::default()),
            server::ServeHost::Loopback
        );
        let mut host_flag = InvocationFlags::default();
        host_flag.insert("host");
        assert_eq!(
            resolve_serve_host(&default, &host_flag),
            server::ServeHost::AllInterfaces
        );
        assert_eq!(
            resolve_serve_host(&default, &InvocationFlags::default()),
            server::ServeHost::Loopback
        );

        let invalid =
            evin_yaml::parse_yaml("site_url: https://example.com/\nhost: 127.0.0.1\n").unwrap();
        assert!(evin_yaml::de::from_spanned_value::<DocsConfig>(invalid).is_err());
    }

    #[test]
    fn reference_prefix_defaults_normalizes_and_validates() {
        assert_eq!(
            DocsConfig::default().normalized_reference_prefix().unwrap(),
            "REF"
        );

        let value =
            evin_yaml::parse_yaml("site_url: https://example.com/docs/\nreference_prefix: rfc\n")
                .unwrap();
        let config = evin_yaml::de::from_spanned_value::<DocsConfig>(value).unwrap();
        assert_eq!(config.normalized_reference_prefix().unwrap(), "RFC");

        for invalid in ["", "RFC-DOC", "RFC DOC"] {
            let config = DocsConfig {
                reference_prefix: invalid.to_string(),
                ..DocsConfig::default()
            };
            assert!(config.normalized_reference_prefix().is_err());
        }
    }

    #[test]
    fn asset_directory_must_stay_inside_the_documentation_source() {
        let invalid =
            evin_yaml::parse_yaml("site_url: https://example.com/\nasset_dir: ../outside\n")
                .unwrap();
        let error = validate_config(Some(&invalid)).unwrap_err();
        assert!(error.message.contains("asset_dir"));
    }

    #[test]
    fn asset_directory_can_be_resolved_from_a_template() {
        let templated =
            evin_yaml::parse_yaml("site_url: ${docs_site_url}\nasset_dir: ${docs_asset_dir}\n")
                .unwrap();
        assert!(validate_config(Some(&templated)).is_ok());
    }

    #[test]
    fn bundled_search_prioritizes_and_displays_formal_reference_ids() {
        let (_, _, javascript, _) = resolve_layout_assets(Path::new("."), "matter", None);
        assert!(javascript.contains("EXACT_REFERENCE: 240"));
        assert!(javascript.contains("reference_id"));
        assert!(javascript.contains("m3-reference-badge"));
    }

    #[test]
    fn legacy_inputs_are_rejected() {
        let yaml = "ast: old.json\nserve: true\n";
        let value = evin_yaml::parse_yaml(yaml).unwrap();
        let error = evin_yaml::de::from_spanned_value::<DocsConfig>(value).unwrap_err();
        assert!(error.message.contains("unknown field"));
    }

    #[test]
    fn source_fingerprint_tracks_docs_and_assets_but_not_layouts() {
        let temp = tempfile::tempdir().unwrap();
        let docs = temp.path().join("docs");
        let layouts = temp.path().join("layouts");
        std::fs::create_dir_all(&docs).unwrap();
        std::fs::create_dir_all(&layouts).unwrap();
        std::fs::write(docs.join("index.md"), "# One").unwrap();
        let config = DocsConfig {
            source_dir: docs.to_string_lossy().to_string(),
            layout_dir: Some(layouts.to_string_lossy().to_string()),
            ..DocsConfig::default()
        };

        let original = source_fingerprint(temp.path(), &config);
        std::fs::write(layouts.join("theme.css"), "body {}").unwrap();
        assert_eq!(source_fingerprint(temp.path(), &config), original);

        std::fs::write(docs.join("guide.rst"), "Guide\n=====").unwrap();
        assert_ne!(source_fingerprint(temp.path(), &config), original);
        std::fs::remove_file(docs.join("guide.rst")).unwrap();
        assert_eq!(source_fingerprint(temp.path(), &config), original);

        let asset = docs.join("assets/diagram.png");
        std::fs::create_dir_all(asset.parent().unwrap()).unwrap();
        std::fs::write(&asset, "image").unwrap();
        assert_ne!(source_fingerprint(temp.path(), &config), original);
        std::fs::remove_file(asset).unwrap();
        assert_eq!(source_fingerprint(temp.path(), &config), original);
    }

    #[test]
    fn bundled_themes_support_component_tree_shaking() {
        {
            let theme = "matter";
            let (_, raw_css, _, embedded_fs) = resolve_layout_assets(Path::new("."), theme, None);
            let registry =
                templates::ComponentRegistry::load_from_layout(&LAYOUTS_DIR, theme, None);

            // 1. Base theme without components has 0 component dead weight
            let base_css = CssOptimizer::process(&raw_css, &HashMap::new(), &embedded_fs, false)
                .unwrap_or_else(|error| panic!("{theme} theme did not compile: {error}"));

            assert!(!base_css.contains(".start-hero"));
            assert!(!base_css.contains(".docs-diagram-container"));
            assert!(!base_css.contains(".docs-math-block"));
            assert!(!base_css.contains(".presentation-surface"));
            assert!(!base_css.contains(".docs-topbar"));
            assert!(base_css.contains("--ui-ref-space-6"));
            assert!(base_css.contains("--ui-color-background"));
            assert!(base_css.contains("--ui-layout-sidebar-width"));
            assert!(base_css.contains("#F8F6F2"));
            assert!(base_css.contains("#141210"));

            // 2. Activating components injects only the active component styles
            let mut used = HashSet::new();
            used.insert("start_page".to_string());
            used.insert("diagram_mermaid".to_string());
            used.insert("math".to_string());
            let comp_scss = registry.collect_css(&used);
            let full_css = CssOptimizer::process_with_components(
                &raw_css,
                &comp_scss,
                &HashMap::new(),
                &embedded_fs,
                false,
            )
            .unwrap();

            assert!(full_css.contains(".start-hero"));
            assert!(full_css.contains(".start-card-grid"));
            assert!(full_css.contains(".mermaid-svg-target"));
            assert!(full_css.contains(".docs-diagram-figure"));
            assert!(full_css.contains(".docs-math-block"));
        }
    }

    #[test]
    fn matter_topbar_compiles_only_the_selected_variant() {
        let (_, raw_css, _, embedded_fs) = resolve_layout_assets(Path::new("."), "matter", None);
        let registry = templates::ComponentRegistry::load_from_layout(&LAYOUTS_DIR, "matter", None);
        let presentation_scss = registry.collect_css(&HashSet::from(["presentation".to_string()]));
        let selected = topbar::Topbar {
            id: None,
            variant: "genome".to_string(),
            badge: None,
            message: "A release is coming.".to_string(),
            action: None,
            dismissible: false,
        };
        let selected_variants = BTreeSet::from([selected.variant.clone()]);
        let selected_css = CssOptimizer::process_with_components(
            &topbar::configure_matter_scss(&raw_css, "matter", &selected_variants, true, false),
            &presentation_scss,
            &HashMap::new(),
            &embedded_fs,
            false,
        )
        .unwrap();
        assert!(selected_css.contains(".topbar--genome"));
        assert!(selected_css.contains(".presentation-surface"));
        assert!(selected_css.contains(".docs-topbar"));
        assert!(!selected_css.contains(".start-spotlight-content"));
        assert!(selected_css.contains("--ui-topbar-genome-surface"));
        assert!(!selected_css.contains(".topbar--aurora"));
        assert!(!selected_css.contains("--ui-topbar-aurora-surface"));

        let shared_css = CssOptimizer::process_with_components(
            &topbar::configure_matter_scss(
                &raw_css,
                "matter",
                &BTreeSet::from(["holo".to_string(), "geo".to_string()]),
                true,
                true,
            ),
            &presentation_scss,
            &HashMap::new(),
            &embedded_fs,
            false,
        )
        .unwrap();
        assert!(shared_css.contains(".topbar--holo"));
        assert!(shared_css.contains(".topbar--geo"));
        assert!(!shared_css.contains(".topbar--genome"));

        let spotlight_only_css = CssOptimizer::process_with_components(
            &topbar::configure_matter_scss(
                &raw_css,
                "matter",
                &BTreeSet::from(["holo".to_string()]),
                false,
                true,
            ),
            &presentation_scss,
            &HashMap::new(),
            &embedded_fs,
            false,
        )
        .unwrap();
        assert!(spotlight_only_css.contains(".start-spotlight-content"));
        assert!(spotlight_only_css.contains(".topbar--holo"));
        assert!(!spotlight_only_css.contains(".docs-topbar"));
        assert!(!spotlight_only_css.contains(".topbar-content"));

        for variant in topbar::MATTER_VARIANTS {
            let mut variant_topbar = selected.clone();
            variant_topbar.variant = variant.to_string();
            let variants = BTreeSet::from([variant_topbar.variant.clone()]);
            let css = CssOptimizer::process_with_components(
                &topbar::configure_matter_scss(&raw_css, "matter", &variants, true, false),
                &presentation_scss,
                &HashMap::new(),
                &embedded_fs,
                false,
            )
            .unwrap();
            assert!(css.contains(&format!(".topbar--{variant}")));
            assert!(css.contains("--topbar-surface"));
            assert!(css.contains(".presentation-layer-one"));
            if *variant != "genome" {
                assert!(!css.contains(".topbar--genome"));
            }
        }

        let disabled_css = CssOptimizer::process_with_components(
            &topbar::configure_matter_scss(&raw_css, "matter", &BTreeSet::new(), false, false),
            "",
            &HashMap::new(),
            &embedded_fs,
            false,
        )
        .unwrap();
        assert!(!disabled_css.contains(".docs-topbar"));
        assert!(!disabled_css.contains(".presentation-surface"));
        assert!(!disabled_css.contains("--ui-topbar-"));
    }

    #[test]
    fn test_presentation_surfaces_decoupled_from_document_theme() {
        let (_, raw_css, _, embedded_fs) = resolve_layout_assets(Path::new("."), "matter", None);
        let registry = templates::ComponentRegistry::load_from_layout(&LAYOUTS_DIR, "matter", None);
        let presentation_scss = registry.collect_css(&HashSet::from(["presentation".to_string()]));

        for variant in topbar::MATTER_VARIANTS {
            let variants = BTreeSet::from([variant.to_string()]);
            let css = CssOptimizer::process_with_components(
                &topbar::configure_matter_scss(&raw_css, "matter", &variants, true, true),
                &presentation_scss,
                &HashMap::new(),
                &embedded_fs,
                false,
            )
            .unwrap();

            assert!(
                css.contains(&format!(".presentation-surface--{variant}")),
                "Missing presentation surface class for {variant}"
            );
            assert!(
                css.contains("--presentation-surface"),
                "Missing --presentation-surface token for {variant}"
            );
            assert!(
                css.contains("--presentation-ink"),
                "Missing --presentation-ink token for {variant}"
            );
            assert!(
                css.contains("--presentation-action-ink"),
                "Missing --presentation-action-ink token for {variant}"
            );
            assert!(
                css.contains("--presentation-badge-ink"),
                "Missing --presentation-badge-ink token for {variant}"
            );
            assert!(
                css.contains("--topbar-action-ink-hover"),
                "Missing --topbar-action-ink-hover token for {variant}"
            );
            assert!(
                css.contains("--topbar-action-font-size"),
                "Missing --topbar-action-font-size token for {variant}"
            );
            assert!(
                css.contains("--spotlight-action-font-size"),
                "Missing --spotlight-action-font-size token for {variant}"
            );
        }

        // Verify spotlight-only compilation contains complete presentation variables without topbar layout
        let spotlight_css = CssOptimizer::process_with_components(
            &topbar::configure_matter_scss(
                &raw_css,
                "matter",
                &BTreeSet::from(["brutalist".to_string()]),
                false,
                true,
            ),
            &presentation_scss,
            &HashMap::new(),
            &embedded_fs,
            false,
        )
        .unwrap();

        assert!(spotlight_css.contains(".presentation-surface--brutalist"));
        assert!(spotlight_css.contains("--presentation-surface"));
        assert!(spotlight_css.contains(".start-spotlight"));
        assert!(spotlight_css.contains("--spotlight-action-font-size"));
        assert!(spotlight_css.contains(".presentation-scale-feature"));
        assert!(!spotlight_css.contains(".docs-topbar"));
        assert!(!spotlight_css.contains(".topbar-content"));
    }

    #[test]
    fn bundled_themes_use_only_material_window_width_boundaries() {
        let width_query =
            regex::Regex::new(r"@media\s*\(\s*(?:min|max)-width:\s*(\d+)px\s*\)").unwrap();
        let allowed = [600_u16, 840, 1200, 1600];

        {
            let theme = "matter";
            let (_, raw_css, _, embedded_fs) = resolve_layout_assets(Path::new("."), theme, None);
            let css = CssOptimizer::process(&raw_css, &HashMap::new(), &embedded_fs, false)
                .unwrap_or_else(|error| panic!("{theme} theme did not compile: {error}"));
            let boundaries: Vec<u16> = width_query
                .captures_iter(&css)
                .map(|capture| capture[1].parse::<u16>().unwrap())
                .collect();

            assert!(!boundaries.is_empty(), "{theme} emitted no width classes");
            assert!(
                boundaries.iter().all(|boundary| allowed.contains(boundary)),
                "{theme} emitted a non-canonical width boundary: {boundaries:?}"
            );
            assert!(css.contains("--ui-layout-window-width-class: compact"));
            assert!(css.contains("--ui-layout-window-height-class: compact"));
        }
    }

    #[test]
    fn theme_overrides_win_and_shorthand_names_target_semantic_tokens() {
        let (_, raw_css, _, embedded_fs) = resolve_layout_assets(Path::new("."), "matter", None);
        let overrides = HashMap::from([
            ("ui_bg".to_string(), "#222222".to_string()),
            ("ui_color_primary".to_string(), "#eeeeee".to_string()),
        ]);
        let css = CssOptimizer::process(&raw_css, &overrides, &embedded_fs, false).unwrap();

        let override_start = css.rfind(":root {").unwrap();
        assert!(css[override_start..].contains("--ui-color-background: #222222"));
        assert!(css[override_start..].contains("--ui-color-primary: #eeeeee"));
        assert!(override_start > css.rfind("[data-theme=dark]").unwrap());
    }

    #[test]
    fn matter_emits_and_uses_configurable_font_tokens() {
        let (_, raw_css, _, embedded_fs) = resolve_layout_assets(Path::new("."), "matter", None);
        let css = CssOptimizer::process(&raw_css, &HashMap::new(), &embedded_fs, false).unwrap();

        assert!(css.contains("--ui-typescale-font-sans: system-ui"));
        assert!(css.contains("--ui-typescale-font-monospace: \"IBM Plex Mono\", ui-monospace"));
        assert!(css.contains("font-family: var(--ui-typescale-font-sans)"));
        assert!(css.contains("font-family: var(--ui-typescale-font-monospace)"));
    }

    #[test]
    fn matter_sidebar_backdrop_preserves_fixed_positioning_and_excludes_ripple() {
        let (_, raw_css, app_js, embedded_fs) =
            resolve_layout_assets(Path::new("."), "matter", None);
        let css = CssOptimizer::process(&raw_css, &HashMap::new(), &embedded_fs, false).unwrap();

        assert!(
            css.contains(".sidebar-backdrop") && css.contains("position: fixed !important"),
            "sidebar-backdrop must enforce fixed !important positioning to survive cascade overrides"
        );
        assert!(
            app_js.contains("button:not(.sidebar-backdrop)"),
            "ripple target selector must explicitly exclude sidebar-backdrop"
        );
        assert!(
            app_js.contains("button.classList.contains('sidebar-backdrop')"),
            "ripple effect function must guard against sidebar-backdrop"
        );
        assert!(
            !app_js.contains("$('#sidebarBackdrop')?.addEventListener('click'"),
            "sidebarBackdrop should not have redundant click listener"
        );
    }

    #[test]
    fn bundled_styles_keep_ui_colors_in_the_token_layer() {
        fn inspect(
            dir: &include_dir::Dir<'_>,
            hex_color: &regex::Regex,
            violations: &mut Vec<String>,
        ) {
            for file in dir.files() {
                let path = file.path().to_string_lossy();
                let is_layout_source =
                    path.ends_with(".scss") || path.ends_with(".html") || path.ends_with(".js");
                if !is_layout_source
                    || path.ends_with("_tokens.scss")
                    || path.ends_with("shell.html")
                {
                    continue;
                }
                let Some(contents) = file.contents_utf8() else {
                    continue;
                };
                for (index, line) in contents.lines().enumerate() {
                    let trimmed = line.trim_start();
                    let is_syntax_rule = trimmed.starts_with(".syn-");
                    let has_literal_color =
                        hex_color.is_match(line) || line.contains("rgb(") || line.contains("rgba(");
                    if has_literal_color && !is_syntax_rule {
                        violations.push(format!("{}:{}: {}", path, index + 1, line.trim()));
                    }
                }
            }
            for child in dir.dirs() {
                inspect(child, hex_color, violations);
            }
        }

        let mut violations = Vec::new();
        let hex_color = regex::Regex::new(r"(?i)#[0-9a-f]{3,8}\b").unwrap();
        inspect(&LAYOUTS_DIR, &hex_color, &mut violations);
        assert!(
            violations.is_empty(),
            "hard-coded UI colors must live in _tokens.scss:\n{}",
            violations.join("\n")
        );
    }

    #[test]
    fn lsp_target_builds_catalog_json() {
        let temp_dir = tempfile::tempdir().unwrap();
        let docs_dir = temp_dir.path().join("docs");
        std::fs::create_dir_all(&docs_dir).unwrap();

        let echo_rst = "echo\n====\n\n:synopsis: echo [args...]\n:summary: Print text\n:category: flow\n:handler: builtins::echo\n\nPrints strings.\n";
        std::fs::write(docs_dir.join("echo.rst"), echo_rst).unwrap();

        let out_json = temp_dir.path().join("assets/builtins-catalog.json");
        let config = DocsConfig {
            target: DocsTarget::Lsp,
            source_dir: docs_dir.to_str().unwrap().to_string(),
            out: out_json.to_str().unwrap().to_string(),
            ..DocsConfig::default()
        };

        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let secrets = std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::new(
            temp_dir.path(),
        ));
        let logger = evin_action_sdk::Logger::new("docs".to_string(), tx, secrets);
        let mut stats = DocsDebugStats::default();
        let site_ast = parse_docs(temp_dir.path(), &config, &logger, &mut stats).unwrap();
        let fs = evin_action_sdk::fs::WorkspaceFs::new(
            temp_dir.path().to_path_buf(),
            temp_dir.path().to_path_buf(),
            evin_action_sdk::PipelineMode::Normal,
            logger.clone(),
        );
        let out_path = lsp::build_lsp_catalog(&fs, &config, &site_ast, &logger).unwrap();

        assert!(out_path.exists());
        let raw = std::fs::read_to_string(&out_path).unwrap();
        let catalog: evin_catalog::CatalogExport = serde_json::from_str(&raw).unwrap();
        assert_eq!(catalog.version, 1);
        assert!(catalog.symbols.contains_key("echo"));
        let echo_meta = &catalog.symbols["echo"];
        assert_eq!(echo_meta.synopsis, "echo [args...]");
        assert_eq!(echo_meta.summary, "Print text");
        assert_eq!(echo_meta.binding, "builtins::echo");
        assert!(echo_meta.hover_markdown.contains("Prints strings."));
        assert!(echo_meta.terminal_ansi.contains("Prints strings."));

        let first = std::fs::read_to_string(&out_path).unwrap();
        lsp::build_lsp_catalog(&fs, &config, &site_ast, &logger).unwrap();
        assert_eq!(std::fs::read_to_string(&out_path).unwrap(), first);
    }

    #[test]
    fn lsp_target_builds_flow_catalog_from_configuration_references() {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
        let docs_dir = workspace_root.join("docs/reference");
        let temp_dir = tempfile::tempdir().unwrap();
        let out_json = temp_dir.path().join("catalog/flow.json");
        let config = DocsConfig {
            target: DocsTarget::Lsp,
            source_dir: docs_dir.to_string_lossy().to_string(),
            include: vec![
                "20.workspace-configuration.4500.md".into(),
                "30.module-configuration.4510.md".into(),
                "35.namespace-configuration.4515.md".into(),
                "40.environment-configuration.4520.md".into(),
                "50.git-rules-configuration.4530.md".into(),
            ],
            out: out_json.to_string_lossy().to_string(),
            ..DocsConfig::default()
        };

        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let secrets = std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::new(
            workspace_root,
        ));
        let logger = evin_action_sdk::Logger::new("docs".to_string(), tx, secrets);
        let mut stats = DocsDebugStats::default();
        let site_ast = parse_docs(workspace_root, &config, &logger, &mut stats).unwrap();
        let fs = evin_action_sdk::fs::WorkspaceFs::new(
            temp_dir.path().to_path_buf(),
            temp_dir.path().to_path_buf(),
            evin_action_sdk::PipelineMode::Normal,
            logger.clone(),
        );

        let out_path = lsp::build_lsp_catalog(&fs, &config, &site_ast, &logger).unwrap();

        assert_eq!(out_path, out_json);
        let raw = std::fs::read_to_string(&out_path).unwrap();
        let catalog = evin_catalog::CatalogExport::from_json(&raw).unwrap();
        let tasks = &catalog.symbols["workspace.tasks"];
        assert!(tasks.binding.is_empty());
        assert!(tasks.hover_markdown.starts_with("```flow\ntasks:"));
        assert!(
            tasks
                .hover_markdown
                .contains("**Task definitions** (`workspace.tasks`):")
        );
        assert!(catalog.symbols.contains_key("GitRules.releases"));
        assert!(out_path.parent().unwrap().is_dir());
    }

    #[test]
    fn lsp_catalog_dry_run_never_writes_the_physical_output() {
        let temp_dir = tempfile::tempdir().unwrap();
        let docs_dir = temp_dir.path().join("docs");
        std::fs::create_dir_all(&docs_dir).unwrap();
        std::fs::write(
            docs_dir.join("echo.rst"),
            "echo\n====\n\n:synopsis: echo\n:summary: Print text\n:category: flow\n:handler: builtins::echo\n",
        )
        .unwrap();
        let config = DocsConfig {
            target: DocsTarget::Lsp,
            source_dir: docs_dir.to_string_lossy().to_string(),
            out: "assets/catalog.json".to_string(),
            ..DocsConfig::default()
        };
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let secrets = std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::new(
            temp_dir.path(),
        ));
        let logger = evin_action_sdk::Logger::new("docs".to_string(), tx, secrets);
        let fs = evin_action_sdk::fs::WorkspaceFs::new(
            temp_dir.path().to_path_buf(),
            temp_dir.path().to_path_buf(),
            evin_action_sdk::PipelineMode::DryRun,
            logger.clone(),
        );
        let mut stats = DocsDebugStats::default();
        let site_ast = parse_docs(temp_dir.path(), &config, &logger, &mut stats).unwrap();

        let output = lsp::build_lsp_catalog(&fs, &config, &site_ast, &logger).unwrap();

        assert!(!output.exists());
        assert!(fs.exists("assets/catalog.json"));
    }

    #[test]
    fn generate_esh_builtins_catalog_json_from_markdown_reference() {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let workspace_root = manifest_dir.parent().unwrap().parent().unwrap();
        let esh_dir = workspace_root.join("docs/reference/esh");
        let temp_dir = tempfile::tempdir().unwrap();

        let out_json = temp_dir.path().join("builtins-catalog.json");
        let config = DocsConfig {
            target: DocsTarget::Lsp,
            source_dir: esh_dir.to_str().unwrap().to_string(),
            include: vec!["100.runtime-reference.3010.md".to_string()],
            out: out_json.to_str().unwrap().to_string(),
            ..DocsConfig::default()
        };

        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let secrets = std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::new(
            workspace_root,
        ));
        let logger = evin_action_sdk::Logger::new("docs".to_string(), tx, secrets);
        let mut stats = DocsDebugStats::default();
        let site_ast = parse_docs(workspace_root, &config, &logger, &mut stats).unwrap();
        let fs = evin_action_sdk::fs::WorkspaceFs::new(
            temp_dir.path().to_path_buf(),
            temp_dir.path().to_path_buf(),
            evin_action_sdk::PipelineMode::Normal,
            logger.clone(),
        );
        let out_path = lsp::build_lsp_catalog(&fs, &config, &site_ast, &logger).unwrap();

        assert!(out_path.exists());
        let raw = std::fs::read_to_string(&out_path).unwrap();
        let catalog: evin_catalog::CatalogExport = serde_json::from_str(&raw).unwrap();
        assert_eq!(catalog.version, 1);
        assert!(
            catalog.symbols.len() >= 60,
            "Expected >=60 symbols, found {}",
            catalog.symbols.len()
        );
        assert!(
            catalog.keywords.len() >= 10,
            "Expected >=10 keywords, found {}",
            catalog.keywords.len()
        );
    }

    #[test]
    fn visual_gallery_fixture_parses_validates_and_renders_markdown_components() {
        let temp_dir = tempfile::tempdir().unwrap();
        let docs_dir = temp_dir.path().join("docs");
        std::fs::create_dir_all(&docs_dir).unwrap();
        std::fs::write(
            docs_dir.join("index.md"),
            "# Test docs\n\nA valid start page.\n\n## Explore\n\n- [Gallery](/gallery) — Render the component fixture.\n",
        )
        .unwrap();
        std::fs::write(
            docs_dir.join("gallery.md"),
            include_str!("../../testdata/visual-gallery.md"),
        )
        .unwrap();

        let config = DocsConfig {
            source_dir: docs_dir.to_string_lossy().to_string(),
            out: temp_dir.path().join("site").to_string_lossy().to_string(),
            site_url: "https://example.com/".to_string(),
            ..DocsConfig::default()
        };
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let secrets = std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::new(
            temp_dir.path(),
        ));
        let logger = evin_action_sdk::Logger::new("docs".to_string(), tx, secrets);
        let mut stats = DocsDebugStats::default();
        let site = parse_docs(temp_dir.path(), &config, &logger, &mut stats).unwrap();
        let output = render_docs(temp_dir.path(), &config, &site, &logger, &mut stats).unwrap();
        let html = std::fs::read_to_string(output.join("gallery/index.html")).unwrap();

        for expected in [
            "doc-svgbob",
            "doc-mermaid",
            "math-block",
            "doc-list-stepper",
            "doc-list-task",
            "doc-list-def",
            "doc-list-fields",
            "docs-callout",
        ] {
            assert!(html.contains(expected), "fixture did not render {expected}");
        }
        assert!(html.matches("class=\"mermaid-svg-target\"").count() >= 1);
        for forbidden in [
            ["mermaid", ".esm"].concat(),
            ["window", ".mermaid"].concat(),
            ["js", "delivr"].concat(),
        ] {
            assert!(!html.contains(&forbidden));
        }
    }

    #[test]
    fn malformed_mermaid_fails_even_when_link_validation_is_disabled() {
        let temp_dir = tempfile::tempdir().unwrap();
        let docs_dir = temp_dir.path().join("docs");
        std::fs::create_dir_all(&docs_dir).unwrap();
        std::fs::write(
            docs_dir.join("index.md"),
            "# Test docs\n\nA valid start page.\n\n## Explore\n\n- [Broken](/broken) — Broken diagram.\n",
        )
        .unwrap();
        std::fs::write(
            docs_dir.join("broken.md"),
            "# Broken\n\n```mermaid \"Bad graph\"\nflowchart LR\n  A --\n```\n",
        )
        .unwrap();

        let output = temp_dir.path().join("site");
        let config = DocsConfig {
            source_dir: docs_dir.to_string_lossy().to_string(),
            out: output.to_string_lossy().to_string(),
            site_url: "https://example.com/".to_string(),
            validate_links: false,
            ..DocsConfig::default()
        };
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let secrets = std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::new(
            temp_dir.path(),
        ));
        let logger = evin_action_sdk::Logger::new("docs".to_string(), tx, secrets);
        let mut stats = DocsDebugStats::default();
        let site = parse_docs(temp_dir.path(), &config, &logger, &mut stats).unwrap();
        let error = render_docs(temp_dir.path(), &config, &site, &logger, &mut stats).unwrap_err();
        let error = format!("{error:?}");

        assert!(error.contains("issue-docs-mermaid-failed"));
        assert!(error.contains("broken.md"));
        assert!(error.contains("Bad graph"));
        assert!(!output.exists());
    }

    #[test]
    fn mermaid_is_offline_in_separated_and_single_file_bundles() {
        let temp_dir = tempfile::tempdir().unwrap();
        let docs_dir = temp_dir.path().join("docs");
        std::fs::create_dir_all(&docs_dir).unwrap();
        std::fs::write(
            docs_dir.join("index.md"),
            "# Test docs\n\nA valid start page.\n\n## Explore\n\n- [Graph](/graph) — Compiled diagram.\n",
        )
        .unwrap();
        std::fs::write(
            docs_dir.join("graph.md"),
            "# Graph\n\n```mermaid \"Offline graph\"\nflowchart LR\n  A --> B\n```\n",
        )
        .unwrap();

        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let secrets = std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::new(
            temp_dir.path(),
        ));
        let logger = evin_action_sdk::Logger::new("docs".to_string(), tx, secrets);

        for bundle in ["separated", "single-file"] {
            let config = DocsConfig {
                source_dir: docs_dir.to_string_lossy().to_string(),
                out: temp_dir.path().join(bundle).to_string_lossy().to_string(),
                site_url: "https://example.com/".to_string(),
                bundle: bundle.to_string(),
                ..DocsConfig::default()
            };
            let mut stats = DocsDebugStats::default();
            let site = parse_docs(temp_dir.path(), &config, &logger, &mut stats).unwrap();
            let output = render_docs(temp_dir.path(), &config, &site, &logger, &mut stats).unwrap();
            let html = std::fs::read_to_string(output.join("graph/index.html")).unwrap();

            assert_eq!(html.matches("class=\"mermaid-svg-target\"").count(), 1);
            assert!(html.contains("class=\"mermaid-source\" hidden>flowchart LR"));
            assert!(html.contains("<svg id=\"diagram-graph-mermaid-1\""));
            for forbidden in [
                ["mermaid", ".esm"].concat(),
                ["window", ".mermaid"].concat(),
                ["js", "delivr"].concat(),
            ] {
                assert!(!html.contains(&forbidden));
            }
        }
    }

    #[test]
    fn docs_build_executes_and_renders_terminal_tapes() {
        let temp_dir = tempfile::tempdir().unwrap();
        let docs_dir = temp_dir.path().join("docs");
        std::fs::create_dir_all(&docs_dir).unwrap();
        std::fs::write(
            docs_dir.join("index.md"),
            "# Test docs\n\nA valid start page.\n\n## Explore\n\n- [Tape](/tape) — Run a terminal tape.\n",
        )
        .unwrap();
        std::fs::write(
            docs_dir.join("tape.md"),
            "# Tape\n\n```tape title=\"Build proof\" rows=4 cols=40\ntype \"pwd\"\nenter\nwait\n```\n",
        )
        .unwrap();

        let config = DocsConfig {
            source_dir: docs_dir.to_string_lossy().to_string(),
            out: temp_dir.path().join("site").to_string_lossy().to_string(),
            site_url: "https://example.com/".to_string(),
            ..DocsConfig::default()
        };
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let secrets = std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::new(
            temp_dir.path(),
        ));
        let logger = evin_action_sdk::Logger::new("docs".to_string(), tx, secrets);
        let mut stats = DocsDebugStats::default();
        let site = parse_docs(temp_dir.path(), &config, &logger, &mut stats).unwrap();
        let output = render_docs(temp_dir.path(), &config, &site, &logger, &mut stats).unwrap();
        let page = std::fs::read_to_string(output.join("tape/index.html")).unwrap();
        let css = std::fs::read_to_string(output.join("assets/theme.css")).unwrap();

        assert!(page.contains("terminal-tape-svg"));
        assert!(page.contains("prefers-reduced-motion"));
        assert!(page.contains("tape-heading"));
        assert!(page.contains("Build proof"));
        assert!(page.contains("ESH"));
        assert!(page.contains("data-action=\"tape:toggle-playback\""));
        assert!(page.contains("data-action=\"tape:copy\""));
        assert!(page.contains("data-tape-duration-ms"));
        assert!(page.contains("requestAnimationFrame"));
        assert!(page.contains("icon-player-pause"));
        assert!(
            page.contains("rel=\"preload\" href=\"../assets/fonts/IBMPlexMono-Regular.woff2\"")
        );
        assert!(page.contains("<template class=\"tape-copy-script\">pwd\n</template>"));
        assert!(!page.contains("tape-window-controls"));
        assert!(css.contains(".terminal-tape"));
        assert!(css.contains("font-family:'IBM Plex Mono'"));
        for font in &tape_fonts::FONTS {
            assert!(output.join("assets/fonts").join(font.filename).is_file());
        }
        assert!(!default_icons().contains("player-pause"));
    }

    #[test]
    fn rebuilding_docs_prunes_removed_page_routes() {
        let temp_dir = tempfile::tempdir().unwrap();
        let docs_dir = temp_dir.path().join("docs");
        std::fs::create_dir_all(&docs_dir).unwrap();
        std::fs::write(
            docs_dir.join("index.md"),
            "# Test docs\n\nA valid start page.\n\n## Explore\n\n- [Legacy](/legacy) — Render the legacy page.\n",
        )
        .unwrap();
        std::fs::write(docs_dir.join("legacy.md"), "# Legacy\n").unwrap();

        let config = DocsConfig {
            source_dir: docs_dir.to_string_lossy().to_string(),
            out: temp_dir.path().join("site").to_string_lossy().to_string(),
            site_url: "https://example.com/".to_string(),
            ..DocsConfig::default()
        };
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let secrets = std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::new(
            temp_dir.path(),
        ));
        let logger = evin_action_sdk::Logger::new("docs".to_string(), tx, secrets);

        let mut first_stats = DocsDebugStats::default();
        let first_site = parse_docs(temp_dir.path(), &config, &logger, &mut first_stats).unwrap();
        let output = render_docs(
            temp_dir.path(),
            &config,
            &first_site,
            &logger,
            &mut first_stats,
        )
        .unwrap();
        assert!(output.join("legacy/index.html").exists());

        std::fs::remove_file(docs_dir.join("legacy.md")).unwrap();
        std::fs::write(docs_dir.join("current.md"), "# Current\n").unwrap();
        std::fs::write(
            docs_dir.join("index.md"),
            "# Test docs\n\nA valid start page.\n\n## Explore\n\n- [Current](/current) — Render the current page.\n",
        )
        .unwrap();
        let mut second_stats = DocsDebugStats::default();
        let second_site = parse_docs(temp_dir.path(), &config, &logger, &mut second_stats).unwrap();
        render_docs(
            temp_dir.path(),
            &config,
            &second_site,
            &logger,
            &mut second_stats,
        )
        .unwrap();

        assert!(!output.join("legacy/index.html").exists());
        assert!(output.join("current/index.html").exists());
    }

    #[test]
    fn brandless_docs_build_uses_neutral_defaults_without_hardcoded_brand() {
        let temp_dir = tempfile::tempdir().unwrap();
        let docs_dir = temp_dir.path().join("docs");
        std::fs::create_dir_all(&docs_dir).unwrap();
        std::fs::write(
            docs_dir.join("index.md"),
            "# Acme Corp Guide\n\nWelcome to Acme Corp documentation.\n\n## Section 1\n\n- [Page 1](/page-one) — Subpage\n",
        )
        .unwrap();
        std::fs::write(docs_dir.join("page-one.md"), "# Page 1\n\nDetails here.\n").unwrap();

        let config = DocsConfig {
            source_dir: docs_dir.to_string_lossy().to_string(),
            out: temp_dir.path().join("site").to_string_lossy().to_string(),
            site_url: "https://example.com/".to_string(),
            ..DocsConfig::default()
        };
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let secrets = std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::new(
            temp_dir.path(),
        ));
        let logger = evin_action_sdk::Logger::new("docs".to_string(), tx, secrets);
        let mut stats = DocsDebugStats::default();
        let site = parse_docs(temp_dir.path(), &config, &logger, &mut stats).unwrap();
        let output = render_docs(temp_dir.path(), &config, &site, &logger, &mut stats).unwrap();

        let index_html = std::fs::read_to_string(output.join("index.html")).unwrap();
        let page_html = std::fs::read_to_string(output.join("page-one/index.html")).unwrap();

        // 1. Check neutral glyph is rendered
        assert!(
            index_html.contains("brand-fallback-glyph"),
            "index.html should have brand-fallback-glyph"
        );
        assert!(
            index_html.contains('🕮'),
            "index.html should contain neutral glyph"
        );
        assert!(
            page_html.contains("brand-fallback-glyph"),
            "page-one should have brand-fallback-glyph"
        );

        // 2. Check no hardcoded vendor branding leaked
        assert!(
            !index_html.contains("EVIN documentation footer"),
            "footer aria label should be neutral"
        );
        assert!(
            !index_html.to_lowercase().contains("dukasn"),
            "should not leak author/repo handles"
        );

        // 3. No footer section in index.md means no <footer> tag in output
        assert!(
            !index_html.contains("<footer"),
            "no footer should be rendered when not authored"
        );

        // 4. Neutral favicon was generated
        let favicon_svg =
            std::fs::read_to_string(output.join("assets/favicons/favicon.svg")).unwrap();
        assert!(favicon_svg.contains("<svg"), "favicon.svg should exist");
        assert!(
            !favicon_svg.contains("#1ED4BE"),
            "favicon must not contain EVIN brand teal"
        );
    }

    #[test]
    fn custom_extension_brand_assets_render_in_output() {
        let temp_dir = tempfile::tempdir().unwrap();
        let docs_dir = temp_dir.path().join("docs");
        std::fs::create_dir_all(&docs_dir).unwrap();
        std::fs::write(
            docs_dir.join("index.md"),
            "# Acme Corp Guide\n\nWelcome to Acme Corp.\n\n## Section\n\n- [P1](/p1) — P1\n\n## Footer\n\nAcme Copyright 2026\n\n### Links\n\n- [Acme](https://acme.org)\n",
        )
        .unwrap();
        std::fs::write(docs_dir.join("p1.md"), "# P1\n").unwrap();

        // Set up conventional extension assets under .evin/actions/docs
        let ext_dir = temp_dir.path().join(".evin/actions/docs");
        std::fs::create_dir_all(&ext_dir).unwrap();
        std::fs::write(
            ext_dir.join("logo.svg"),
            "<svg id=\"acme-logo\"><circle r=\"10\"/></svg>",
        )
        .unwrap();
        std::fs::write(
            ext_dir.join("favicon.svg"),
            "<svg id=\"acme-favicon\"></svg>",
        )
        .unwrap();

        let config = DocsConfig {
            source_dir: docs_dir.to_string_lossy().to_string(),
            out: temp_dir.path().join("site").to_string_lossy().to_string(),
            site_url: "https://example.com/".to_string(),
            ..DocsConfig::default()
        };
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let secrets = std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::new(
            temp_dir.path(),
        ));
        let logger = evin_action_sdk::Logger::new("docs".to_string(), tx, secrets);
        let mut stats = DocsDebugStats::default();
        let site = parse_docs(temp_dir.path(), &config, &logger, &mut stats).unwrap();
        let output = render_docs(temp_dir.path(), &config, &site, &logger, &mut stats).unwrap();

        let index_html = std::fs::read_to_string(output.join("index.html")).unwrap();

        // Custom logo rendered in topbar and start footer
        assert!(
            index_html.contains("id=\"acme-logo\""),
            "custom logo SVG must be rendered in index.html"
        );
        assert!(
            index_html.contains("brand-logo-custom"),
            "custom logo class must be present"
        );
        assert!(
            index_html.contains("<footer"),
            "footer must be rendered when authored"
        );
        assert!(
            index_html.contains("start-footer-logo brand-logo-custom"),
            "footer logo must use custom logo"
        );

        // Custom favicon was used
        let favicon_svg =
            std::fs::read_to_string(output.join("assets/favicons/favicon.svg")).unwrap();
        assert!(
            favicon_svg.contains("acme-favicon"),
            "favicon should match custom extension favicon"
        );
    }

    #[test]
    fn test_metrics_config_validation() {
        let yaml_bad = "metrics: false\nmetrics_budget_kb: 110\nsite_url: 'https://example.com'";
        let val_bad = evin_yaml::parse_yaml(yaml_bad).unwrap();
        let err = validate_config(Some(&val_bad));
        assert!(err.is_err());
        assert!(
            err.unwrap_err()
                .message
                .contains("metrics_budget_kb requires metrics")
        );

        let yaml_good = "metrics: true\nmetrics_budget_kb: 110\nsite_url: 'https://example.com'";
        let val_good = evin_yaml::parse_yaml(yaml_good).unwrap();
        assert!(validate_config(Some(&val_good)).is_ok());

        let yaml_no_budget = "metrics: true\nsite_url: 'https://example.com'";
        let val_no_budget = evin_yaml::parse_yaml(yaml_no_budget).unwrap();
        assert!(validate_config(Some(&val_no_budget)).is_ok());
    }

    #[test]
    fn declared_metrics_page_requires_dev_metadata_and_uses_its_slug() {
        let metrics_page = |dev_only: bool| ast::DocPage {
            slug: "metrics".to_string(),
            source_path: "90.metrics.md".to_string(),
            format: DocFormat::Markdown,
            metadata: HashMap::from([
                ("page_kind".to_string(), serde_json::json!("metrics")),
                ("dev_only".to_string(), serde_json::json!(dev_only)),
            ]),
            reference: None,
            toc: vec![],
            ast: vec![],
        };
        let site = DocSiteAst {
            version: "1.0".to_string(),
            title: Some("Docs".to_string()),
            reference_prefix: "REF".to_string(),
            pages: vec![metrics_page(true)],
        };
        let config = DocsConfig {
            mode: DocsMode::Serve,
            metrics: true,
            ..DocsConfig::default()
        };
        assert_eq!(
            metrics_page_route(&site, &config).unwrap(),
            Some("/metrics".to_string())
        );

        let no_metrics = DocsConfig {
            mode: DocsMode::Serve,
            ..DocsConfig::default()
        };
        assert!(metrics_page_route(&site, &no_metrics).is_err());

        let invalid = DocSiteAst {
            pages: vec![metrics_page(false)],
            ..site.clone()
        };
        assert!(metrics_page_route(&invalid, &config).is_err());

        let static_config = DocsConfig {
            mode: DocsMode::Build,
            metrics: true,
            ..DocsConfig::default()
        };
        assert_eq!(metrics_page_route(&site, &static_config).unwrap(), None);

        let public_component = DocSiteAst {
            pages: vec![ast::DocPage {
                slug: "public".to_string(),
                source_path: "guide/10.public.md".to_string(),
                format: DocFormat::Markdown,
                metadata: HashMap::new(),
                reference: None,
                toc: vec![],
                ast: vec![evin_doc::ast::DocNode::Directive {
                    name: "metrics-budget".to_string(),
                    args: vec![],
                    body: vec![],
                }],
            }],
            ..site
        };
        assert!(validate_metrics_pages(&public_component, &static_config).is_err());
    }

    #[tokio::test]
    async fn test_metrics_build_emission_and_budget_gate() {
        let temp_dir = tempfile::tempdir().unwrap();
        let docs_dir = temp_dir.path().join("docs");
        std::fs::create_dir_all(&docs_dir).unwrap();
        std::fs::write(
            docs_dir.join("index.md"),
            "# Test docs\n\nA valid start page.\n\n## Explore\n\n- [Hello](/hello) — Hello page.\n",
        )
        .unwrap();
        std::fs::write(docs_dir.join("hello.md"), "# Hello\n\nHello content.\n").unwrap();

        // 1. Build with metrics enabled and generous budget -> passes and writes metrics.json
        let config_pass = DocsConfig {
            source_dir: docs_dir.to_string_lossy().to_string(),
            out: temp_dir
                .path()
                .join("out_pass")
                .to_string_lossy()
                .to_string(),
            site_url: "https://example.com/".to_string(),
            metrics: true,
            metrics_budget_kb: Some(500),
            ..DocsConfig::default()
        };
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let secrets = std::sync::Arc::new(evin_action_sdk::secrets::SecretsProvider::new(
            temp_dir.path(),
        ));
        let logger = evin_action_sdk::Logger::new("docs".to_string(), tx.clone(), secrets.clone());
        let mut stats = DocsDebugStats::default();
        let site = parse_docs(temp_dir.path(), &config_pass, &logger, &mut stats).unwrap();
        let out_pass =
            render_docs(temp_dir.path(), &config_pass, &site, &logger, &mut stats).unwrap();

        let report = metrics::generate_report(&out_pass, &config_pass).unwrap();
        let written = metrics::write_report(&out_pass, &report).unwrap();
        assert!(written.is_file());
        assert_eq!(report.budget_status, metrics::BudgetStatus::Pass);

        // 2. Report with tiny budget (e.g. 1 KB) -> reports BudgetStatus::Fail
        let config_fail = DocsConfig {
            metrics: true,
            metrics_budget_kb: Some(1),
            ..config_pass.clone()
        };
        let report_fail = metrics::generate_report(&out_pass, &config_fail).unwrap();
        assert_eq!(report_fail.budget_status, metrics::BudgetStatus::Fail);
    }
}
