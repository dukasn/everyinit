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
pub mod i18n;
pub mod icons;
pub mod js_minification;
pub mod mermaid;
pub mod reference;
pub mod renderer;
pub mod search;
pub mod search_binary;
pub mod seo;
pub mod server;
pub mod start_page;
pub mod syntax;
pub mod templates;
pub mod topbar;
pub mod validator;

pub mod resolver;

use ast::{DocFormat, DocSiteAst};
use css::CssOptimizer;
use debug::DocsDebugStats;
use evin_action_sdk::{InvocationFlags, RunnerContext};
use fs::EmbeddedFs;
use include_dir::{Dir, include_dir};
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

fn default_source_dir() -> PathBuf {
    PathBuf::from("docs")
}
fn default_out() -> PathBuf {
    PathBuf::from("dist/docs")
}
fn default_asset_dir() -> String {
    "assets".to_string()
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
fn default_title() -> String {
    "Documentation".to_string()
}
fn default_manifest() -> PathBuf {
    PathBuf::from("versions.json")
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DocsMode {
    #[default]
    Build,
    Serve,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum BundleMode {
    #[default]
    Separated,
    SingleFile,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub struct HexColor(String);

impl TryFrom<String> for HexColor {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.len() == 7
            && value.starts_with('#')
            && value[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            Ok(Self(value.to_ascii_lowercase()))
        } else {
            Err("color must use #RRGGBB format".to_string())
        }
    }
}

impl HexColor {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppearanceConfig {
    #[serde(default)]
    pub accent: Option<HexColor>,
    #[serde(default)]
    pub logo: Option<PathBuf>,
    #[serde(default)]
    pub favicon: Option<PathBuf>,
    #[serde(default)]
    pub font_sans: Option<String>,
    #[serde(default)]
    pub font_mono: Option<String>,
    #[serde(default)]
    pub custom_css: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VersionConfig {
    pub current: String,
    pub latest: String,
    #[serde(default = "default_manifest")]
    pub manifest: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct I18nConfig {
    pub default_locale: String,
    pub locales: Vec<String>,
    #[serde(default)]
    pub dictionaries_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServeConfig {
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub host: bool,
}

impl Default for ServeConfig {
    fn default() -> Self {
        Self {
            port: default_port(),
            host: false,
        }
    }
}

impl BundleMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Separated => "separated",
            Self::SingleFile => "single-file",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocsConfig {
    pub site_url: url::Url,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default)]
    pub mode: DocsMode,
    #[serde(default = "default_source_dir")]
    pub source_dir: PathBuf,
    #[serde(default = "default_asset_dir")]
    pub asset_dir: String,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default = "default_extract_frontmatter")]
    pub extract_frontmatter: bool,
    #[serde(default = "default_title")]
    pub title: String,
    #[serde(default = "default_out")]
    pub out: PathBuf,
    #[serde(skip, default = "default_layout")]
    pub theme: String,
    #[serde(default)]
    pub bundle: BundleMode,
    #[serde(default = "default_validate_links")]
    pub validate_links: bool,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub host: bool,
    #[serde(default)]
    pub layout_dir: Option<String>,
    #[serde(default)]
    pub appearance: AppearanceConfig,
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
    pub diagnostic_search: bool,
    #[serde(default)]
    pub social_image: Option<String>,
    #[serde(default)]
    pub lastmod_fallback: bool,
    #[serde(default)]
    pub version: Option<VersionConfig>,
    #[serde(default)]
    pub i18n: Option<I18nConfig>,
    #[serde(default)]
    pub grammars: Vec<PathBuf>,
    #[serde(default)]
    pub serve: ServeConfig,
}

impl Default for DocsConfig {
    fn default() -> Self {
        Self {
            site_url: url::Url::parse("https://example.invalid/").expect("valid default URL"),
            language: default_language(),
            mode: DocsMode::Build,
            source_dir: default_source_dir(),
            asset_dir: default_asset_dir(),
            include: Vec::new(),
            exclude: Vec::new(),
            extract_frontmatter: default_extract_frontmatter(),
            title: default_title(),
            out: default_out(),
            theme: default_layout(),
            bundle: BundleMode::default(),
            validate_links: default_validate_links(),
            port: default_port(),
            host: false,
            layout_dir: None,
            appearance: AppearanceConfig::default(),
            extensions_dir: None,
            theme_color: None,
            background_color: None,
            icon_dir: None,
            minify: None,
            reference_prefix: default_reference_prefix(),
            strict_filenames: false,
            topbar: None,
            diagnostic_search: false,
            social_image: None,
            lastmod_fallback: false,
            version: None,
            i18n: None,
            grammars: Vec::new(),
            serve: ServeConfig::default(),
        }
    }
}

fn default_layout() -> String {
    "matter".to_string()
}

fn resolve_serve_host(config: &DocsConfig, flags: &InvocationFlags) -> server::ServeHost {
    server::ServeHost::from_exposed(config.serve.host || flags.contains("host"))
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
            .map(TopbarSetting::validate)
            .unwrap_or(Ok(()))
    }

    fn validate_version(&self) -> Result<(), String> {
        let Some(version) = &self.version else {
            return Ok(());
        };
        for (name, value) in [
            ("version.current", &version.current),
            ("version.latest", &version.latest),
        ] {
            if value.is_empty()
                || !value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
            {
                return Err(format!("{name} must be a non-empty path-safe identifier"));
            }
        }
        if version.manifest.is_absolute()
            || version
                .manifest
                .components()
                .any(|component| !matches!(component, std::path::Component::Normal(_)))
        {
            return Err(
                "version.manifest must be a relative path inside the output directory".to_string(),
            );
        }
        Ok(())
    }

    fn validate_i18n(&self) -> Result<(), String> {
        let Some(i18n) = &self.i18n else {
            return Ok(());
        };
        if i18n.locales.is_empty() {
            return Err("i18n.locales must contain at least one locale".to_string());
        }
        let mut locales = HashSet::new();
        for locale in &i18n.locales {
            let parsed = locale
                .parse::<unic_langid::LanguageIdentifier>()
                .map_err(|_| format!("i18n locale '{locale}' is not a valid language tag"))?;
            if !locales.insert(parsed.to_string()) {
                return Err(format!("i18n.locales contains duplicate locale '{locale}'"));
            }
        }
        let default = i18n
            .default_locale
            .parse::<unic_langid::LanguageIdentifier>()
            .map_err(|_| "i18n.default_locale is not a valid language tag".to_string())?;
        if !locales.contains(&default.to_string()) {
            return Err("i18n.default_locale must be included in i18n.locales".to_string());
        }
        Ok(())
    }

    fn validate_appearance(&self) -> Result<(), String> {
        for (name, value) in [
            ("appearance.font_sans", self.appearance.font_sans.as_deref()),
            ("appearance.font_mono", self.appearance.font_mono.as_deref()),
        ] {
            if let Some(value) = value
                && (value.trim().is_empty()
                    || !value.chars().all(|character| {
                        character.is_ascii_alphanumeric()
                            || character.is_ascii_whitespace()
                            || matches!(character, ',' | '-' | '_' | '\'' | '"')
                    }))
            {
                return Err(format!("{name} contains unsupported CSS characters"));
            }
        }
        Ok(())
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
        .and_then(|_| config.validate_appearance())
        .and_then(|_| config.validate_version())
        .and_then(|_| config.validate_i18n())
        .and_then(|_| config.validate_topbar())
        .and_then(|_| {
            if config.asset_dir.contains("${") {
                Ok(())
            } else {
                assets::validate_asset_dir(&config.asset_dir)
            }
        })
        .and_then(|_| {
            seo::validate_site_settings(config.site_url.as_str(), &config.language).map(|_| ())
        });
    validation.map_err(|message| evin_flow::ConfigError {
        message,
        span: None,
    })
}

pub(crate) fn parse_docs(
    workspace_root: &Path,
    config: &DocsConfig,
    logger: &evin_action_sdk::Logger,
    stats: &mut DocsDebugStats,
) -> Result<DocSiteAst, evin_diagnostic::Diagnostic> {
    let source_root = config.source_dir.clone();
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
        Some(config.title.clone()),
    )
    .map_err(doc_issue_to_diagnostic)?;
    if config.strict_filenames {
        validate_strict_filenames(&site).map_err(doc_issue_to_diagnostic)?;
    }
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

fn parse_configured_site(
    workspace_root: &Path,
    config: &DocsConfig,
    logger: &evin_action_sdk::Logger,
    stats: &mut DocsDebugStats,
) -> Result<DocSiteAst, evin_diagnostic::Diagnostic> {
    let mut source_config = config.clone();
    if let Some(i18n) = &config.i18n {
        source_config.source_dir = config.source_dir.join(&i18n.default_locale);
    }
    parse_docs(workspace_root, &source_config, logger, stats)
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
        if parent.is_none() {
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
    let source_root = config.source_dir.clone();
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
    if config.i18n.is_none() && config.version.is_none() {
        return render_docs_single(
            workspace_root,
            config,
            site_ast,
            &i18n::UiDictionary::default(),
            logger,
            stats,
        );
    };
    render_localized_docs(workspace_root, config, site_ast, logger, stats)
}

fn render_localized_docs(
    workspace_root: &Path,
    config: &DocsConfig,
    default_site: &DocSiteAst,
    logger: &evin_action_sdk::Logger,
    stats: &mut DocsDebugStats,
) -> Result<PathBuf, evin_diagnostic::Diagnostic> {
    let out_dir = if config.out.is_absolute() {
        config.out.clone()
    } else {
        workspace_root.join(&config.out)
    };
    let default_locale = config
        .i18n
        .as_ref()
        .map(|settings| settings.default_locale.as_str())
        .unwrap_or(&config.language)
        .to_string();
    let locales = config
        .i18n
        .as_ref()
        .map(|settings| settings.locales.clone())
        .unwrap_or_else(|| vec![default_locale.clone()]);
    let version_dir = config
        .version
        .as_ref()
        .map(|version| out_dir.join(&version.current))
        .unwrap_or_else(|| out_dir.clone());
    let mut routes = BTreeSet::new();
    let mut titles = HashMap::new();

    for locale in &locales {
        let dictionary_dir = config
            .i18n
            .as_ref()
            .and_then(|settings| settings.dictionaries_dir.as_deref());
        let dictionary = i18n::UiDictionary::load(workspace_root, locale, dictionary_dir).map_err(
            |message| {
                evin_diagnostic::Diagnostic::new(
                    message.clone(),
                    evin_diagnostic::IssueCode::new("invalid-i18n-dictionary")
                        .arg("error", message),
                )
            },
        )?;
        let translated_site = if locale == &default_locale {
            default_site.clone()
        } else if config.i18n.is_some() {
            let mut source_config = config.clone();
            source_config.source_dir = config.source_dir.join(locale);
            source_config.i18n = None;
            source_config.version = None;
            parse_docs(workspace_root, &source_config, logger, stats)?
        } else {
            default_site.clone()
        };
        let (mut localized_site, fallback_pages) = if locale == &default_locale {
            (translated_site, HashMap::new())
        } else {
            merge_locale_sites(default_site, &translated_site, &default_locale)
        };
        if config.i18n.is_some() {
            prefix_site_source_paths(&mut localized_site, locale, &fallback_pages);
        }

        for page in &localized_site.pages {
            let route = if page.slug == "index" {
                "/".to_string()
            } else {
                format!("/{}", page.slug)
            };
            let title = page
                .metadata
                .get("title")
                .and_then(|value| value.as_str())
                .unwrap_or(&page.slug);
            routes.insert(route.clone());
            titles.insert(route, title.to_string());
        }

        let mut localized_config = config.clone();
        localized_config.i18n = None;
        localized_config.version = None;
        localized_config.language = locale.clone();
        localized_config.source_dir = config.source_dir.clone();
        localized_config.out = if config.version.is_some() || config.i18n.is_some() {
            version_dir.join(locale)
        } else {
            out_dir.clone()
        };
        localized_config.site_url = match &config.version {
            Some(version) => append_url_segments(
                &config.site_url,
                &[version.current.as_str(), locale.as_str()],
            ),
            None if config.i18n.is_some() => append_url_segments(&config.site_url, &[locale]),
            None => config.site_url.clone(),
        };
        render_docs_single(
            workspace_root,
            &localized_config,
            &localized_site,
            &dictionary,
            logger,
            stats,
        )?;

        let locale_dir = localized_config.out.as_path();
        if config
            .version
            .as_ref()
            .is_some_and(|version| version.current != version.latest)
        {
            let version = config.version.as_ref().expect("checked above");
            let latest_url = append_url_segments(
                &config.site_url,
                &[version.latest.as_str(), locale.as_str()],
            );
            inject_version_banner(
                locale_dir,
                &version.current,
                &version.latest,
                latest_url.as_str(),
                &dictionary,
            )?;
        }
        if !fallback_pages.is_empty() {
            render_fallback_notices(
                config,
                locale,
                &default_locale,
                &fallback_pages,
                locale_dir,
                &dictionary,
            )?;
        }
        if let Some(version) = &config.version {
            inject_version_switcher(
                locale_dir,
                &version.current,
                locale,
                config.site_url.as_str(),
                &version.manifest,
            )?;
        }
    }

    if let Some(version) = &config.version {
        let routes = routes.into_iter().collect::<Vec<_>>();
        let manifest = serde_json::json!({
            "version": version.current,
            "locales": locales,
            "routes": routes,
            "titles": titles,
        });
        write_json_artifact(&version_dir.join("manifest.json"), &manifest)?;

        let registry_path = out_dir.join(&version.manifest);
        let mut registry = if registry_path.is_file() {
            let bytes = std::fs::read(&registry_path)
                .map_err(|error| io_diagnostic(&registry_path, error))?;
            serde_json::from_slice::<serde_json::Value>(&bytes).map_err(|error| {
                evin_diagnostic::Diagnostic::new(
                    format!(
                        "Invalid versions manifest '{}': {error}",
                        registry_path.display()
                    ),
                    evin_diagnostic::IssueCode::new("invalid-manifest"),
                )
            })?
        } else {
            serde_json::json!({ "versions": [] })
        };
        let entries = registry
            .get_mut("versions")
            .and_then(serde_json::Value::as_array_mut)
            .ok_or_else(|| {
                evin_diagnostic::Diagnostic::new(
                    "versions manifest must contain a `versions` array",
                    evin_diagnostic::IssueCode::new("invalid-manifest"),
                )
            })?;
        entries.retain(|entry| {
            entry.get("version").and_then(|value| value.as_str()) != Some(&version.current)
        });
        entries.push(serde_json::json!({
            "version": version.current,
            "locales": locales,
            "manifest": format!("{}/manifest.json", version.current),
        }));
        entries.sort_by(|left, right| {
            left.get("version")
                .and_then(|value| value.as_str())
                .cmp(&right.get("version").and_then(|value| value.as_str()))
        });
        registry["latest"] = serde_json::Value::String(version.latest.clone());
        write_json_artifact(&registry_path, &registry)?;
    }

    if config.version.is_some() || config.i18n.is_some() {
        let target_segments = config.version.as_ref().map_or_else(
            || vec![default_locale.as_str()],
            |version| vec![version.latest.as_str(), default_locale.as_str()],
        );
        let redirect_target = append_url_segments(&config.site_url, &target_segments);
        let root_index = out_dir.join("index.html");
        write_text_artifact(
            &root_index,
            &format!(
                "<!doctype html><html lang=\"{}\"><head><meta charset=\"utf-8\"><link rel=\"canonical\" href=\"{}\"><meta http-equiv=\"refresh\" content=\"0; url={}\"><title>{}</title></head><body><a href=\"{}\">Continue to the documentation</a></body></html>\n",
                escape_html(&default_locale),
                escape_html(redirect_target.as_str()),
                escape_html(redirect_target.as_str()),
                escape_html(&config.title),
                escape_html(redirect_target.as_str())
            ),
        )?;
    }
    Ok(out_dir)
}

fn merge_locale_sites(
    default_site: &DocSiteAst,
    localized_site: &DocSiteAst,
    default_locale: &str,
) -> (DocSiteAst, HashMap<String, String>) {
    let translated = localized_site
        .pages
        .iter()
        .map(|page| (page.slug.as_str(), page))
        .collect::<HashMap<_, _>>();
    let mut used = HashSet::new();
    let mut fallbacks = HashMap::new();
    let mut pages = Vec::new();
    for source_page in &default_site.pages {
        if let Some(page) = translated.get(source_page.slug.as_str()) {
            pages.push((*page).clone());
            used.insert(page.slug.clone());
        } else {
            let mut page = source_page.clone();
            page.metadata.insert(
                "__evin_source_locale".to_string(),
                serde_json::Value::String(default_locale.to_string()),
            );
            fallbacks.insert(page.slug.clone(), default_locale.to_string());
            pages.push(page);
        }
    }
    pages.extend(
        localized_site
            .pages
            .iter()
            .filter(|page| !used.contains(&page.slug))
            .cloned(),
    );
    (
        DocSiteAst {
            pages,
            ..localized_site.clone()
        },
        fallbacks,
    )
}

fn prefix_site_source_paths(
    site: &mut DocSiteAst,
    locale: &str,
    fallbacks: &HashMap<String, String>,
) {
    for page in &mut site.pages {
        let source_locale = fallbacks
            .get(&page.slug)
            .map(String::as_str)
            .unwrap_or(locale);
        page.source_path = format!(
            "{source_locale}/{}",
            page.source_path.trim_start_matches('/')
        );
    }
}

fn render_fallback_notices(
    config: &DocsConfig,
    locale: &str,
    default_locale: &str,
    fallback_pages: &HashMap<String, String>,
    locale_dir: &Path,
    dictionary: &i18n::UiDictionary,
) -> Result<(), evin_diagnostic::Diagnostic> {
    for slug in fallback_pages.keys() {
        let path = if slug == "index" {
            locale_dir.join("index.html")
        } else {
            locale_dir.join(slug).join("index.html")
        };
        let mut html =
            std::fs::read_to_string(&path).map_err(|error| io_diagnostic(&path, error))?;
        let localized_base = match &config.version {
            Some(version) => {
                append_url_segments(&config.site_url, &[version.current.as_str(), locale])
            }
            None => append_url_segments(&config.site_url, &[locale]),
        };
        let default_base = match &config.version {
            Some(version) => append_url_segments(
                &config.site_url,
                &[version.current.as_str(), default_locale],
            ),
            None => append_url_segments(&config.site_url, &[default_locale]),
        };
        let localized_page_url = page_url(&localized_base, slug);
        let default_page_url = page_url(&default_base, slug);
        html = html.replace(localized_page_url.as_str(), default_page_url.as_str());
        html = html.replace(
            &format!("lang=\"{}\"", escape_html(locale)),
            &format!("lang=\"{}\"", escape_html(default_locale)),
        );
        let Some(article) = html.find("<article") else {
            continue;
        };
        let Some(open_end) = html[article..].find('>') else {
            continue;
        };
        let insert_at = article + open_end + 1;
        let notice = format!(
            "<p class=\"untranslated-notice\">{}</p>",
            escape_html(&dictionary.untranslated_notice(default_locale))
        );
        html.insert_str(insert_at, &notice);
        write_text_artifact(&path, &html)?;
    }
    Ok(())
}

fn localize_html_chrome(
    directory: &Path,
    dictionary: &i18n::UiDictionary,
) -> Result<(), evin_diagnostic::Diagnostic> {
    let mut files = Vec::new();
    let mut pending = vec![directory.to_path_buf()];
    while let Some(current_dir) = pending.pop() {
        for entry in
            std::fs::read_dir(&current_dir).map_err(|error| io_diagnostic(&current_dir, error))?
        {
            let entry = entry.map_err(|error| io_diagnostic(&current_dir, error))?;
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path
                .extension()
                .is_some_and(|extension| extension == "html")
            {
                files.push(path);
            }
        }
    }
    files.sort();
    for path in files {
        let mut html =
            std::fs::read_to_string(&path).map_err(|error| io_diagnostic(&path, error))?;
        let replacements = [
            (
                ">On this page<",
                &format!(">{}<", escape_html(&dictionary.on_this_page)),
            ),
            (
                ">Previous</span>",
                &format!(">{}</span>", escape_html(&dictionary.previous)),
            ),
            (
                ">Next</span>",
                &format!(">{}</span>", escape_html(&dictionary.next)),
            ),
            (
                "placeholder=\"Search documentation...\"",
                &format!(
                    "placeholder=\"{}\"",
                    escape_html(&dictionary.search_placeholder)
                ),
            ),
            (
                "aria-label=\"Search documentation\"",
                &format!(
                    "aria-label=\"{}\"",
                    escape_html(&dictionary.search_placeholder)
                ),
            ),
        ];
        for (from, to) in replacements {
            html = html.replace(from, to);
        }
        write_text_artifact(&path, &html)?;
    }
    Ok(())
}

fn page_url(base: &url::Url, slug: &str) -> url::Url {
    if slug == "index" {
        base.clone()
    } else {
        base.join(&format!("{}/", slug.trim_matches('/')))
            .unwrap_or_else(|_| base.clone())
    }
}

fn inject_version_switcher(
    directory: &Path,
    current: &str,
    locale: &str,
    base_url: &str,
    registry_path: &Path,
) -> Result<(), evin_diagnostic::Diagnostic> {
    let mut files = Vec::new();
    let mut pending = vec![directory.to_path_buf()];
    while let Some(current_dir) = pending.pop() {
        for entry in
            std::fs::read_dir(&current_dir).map_err(|error| io_diagnostic(&current_dir, error))?
        {
            let entry = entry.map_err(|error| io_diagnostic(&current_dir, error))?;
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.file_name().is_some_and(|name| name == "index.html") {
                files.push(path);
            }
        }
    }
    files.sort();
    for path in files {
        let mut html =
            std::fs::read_to_string(&path).map_err(|error| io_diagnostic(&path, error))?;
        let route = path
            .parent()
            .and_then(|parent| parent.strip_prefix(directory).ok())
            .map(|parent| parent.to_string_lossy().replace('\\', "/"))
            .filter(|route| !route.is_empty())
            .map(|route| format!("/{route}"))
            .unwrap_or_else(|| "/".to_string());
        let markup = format!(
            "<label class=\"docs-version-switcher\"><span class=\"sr-only\">Version</span><select data-docs-version-switcher hidden data-current=\"{}\" data-locale=\"{}\" data-route=\"{}\" data-base-url=\"{}\" data-registry=\"{}\" aria-label=\"Documentation version\"><option value=\"{}\">{}</option></select></label>",
            escape_html(current),
            escape_html(locale),
            escape_html(&route),
            escape_html(base_url),
            escape_html(&registry_path.to_string_lossy()),
            escape_html(current),
            escape_html(current)
        );
        if let Some(header) = html.find("</header>") {
            html.insert_str(header, &markup);
        } else if let Some(body) = html.find("<body")
            && let Some(open_end) = html[body..].find('>')
        {
            html.insert_str(body + open_end + 1, &markup);
        }
        write_text_artifact(&path, &html)?;
    }
    Ok(())
}

fn render_docs_single(
    workspace_root: &Path,
    config: &DocsConfig,
    site_ast: &DocSiteAst,
    dictionary: &i18n::UiDictionary,
    logger: &evin_action_sdk::Logger,
    stats: &mut DocsDebugStats,
) -> Result<PathBuf, evin_diagnostic::Diagnostic> {
    let static_site;
    let site_ast = if config.mode == DocsMode::Build {
        static_site = DocSiteAst {
            pages: site_ast.pages.to_vec(),
            ..site_ast.clone()
        };
        &static_site
    } else {
        site_ast
    };
    let write_failures_before = stats.write_failures;
    stats.renders += 1;
    stats.bundle = config.bundle.as_str().to_string();
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
        config.site_url.as_str(),
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
    let source_root = config.source_dir.clone();
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
    mermaid::compile_site(&mut rendered_site).map_err(|message| {
        evin_diagnostic::Diagnostic::new(
            message.clone(),
            evin_diagnostic::IssueCode::new("issue-docs-mermaid-failed").arg("error", message),
        )
    })?;
    let start_page = start_page::StartPageModel {
        search_prompts: vec![dictionary.search_placeholder.clone()],
    };
    let topbar_setting = config.topbar.as_ref();
    if let Some(setting) = topbar_setting {
        setting.validate().map_err(|message| {
            evin_diagnostic::Diagnostic::new(
                format!("Invalid topbar configuration: {message}"),
                evin_diagnostic::IssueCode::new("invalid-config"),
            )
        })?;
    }
    let topbar = resolve_topbar(config.topbar.as_ref()).map_err(|message| {
        evin_diagnostic::Diagnostic::new(
            format!("Invalid topbar configuration: {message}"),
            evin_diagnostic::IssueCode::new("invalid-config"),
        )
    })?;
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
    let out = config.out.clone();
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
    let mut site_used_components = HtmlRenderer::collect_site_used_components(&rendered_site);
    if topbar.is_some() {
        site_used_components.insert("presentation".to_string());
    }
    let active_components_scss = registry.collect_css(&site_used_components);
    let active_components_js = registry.collect_js(&site_used_components);
    let raw_css = topbar::configure_matter_scss(&raw_css, topbar.is_some(), false);

    let is_production = config.minify.unwrap_or(config.mode == DocsMode::Build);
    let single_file = config.bundle == BundleMode::SingleFile;
    let mut processed_css = CssOptimizer::process_with_components(
        &raw_css,
        &active_components_scss,
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
    let custom_css = if let Some(path) = &config.appearance.custom_css {
        let path = if path.is_absolute() {
            path.clone()
        } else {
            workspace_root.join(path)
        };
        Some(std::fs::read_to_string(&path).map_err(|error| {
            evin_diagnostic::Diagnostic::new(
                format!(
                    "Failed to read appearance.custom_css '{}': {error}",
                    path.display()
                ),
                evin_diagnostic::IssueCode::new("io-error"),
            )
        })?)
    } else {
        None
    };
    processed_css =
        append_appearance_css(&processed_css, &config.appearance, custom_css.as_deref());
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
    let requested_icons = default_icons();
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
    let grammar_paths = config
        .grammars
        .iter()
        .map(|path| {
            if path.is_absolute() {
                path.clone()
            } else {
                workspace_root.join(path)
            }
        })
        .collect::<Vec<_>>();
    let highlighter =
        crate::v1::syntax::SyntaxHighlighter::load_grammars(&grammar_paths).map_err(|message| {
            evin_diagnostic::Diagnostic::new(
                message.clone(),
                evin_diagnostic::IssueCode::new("invalid-grammar").arg("error", message),
            )
        })?;
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
    )
    .with_highlighter(highlighter);
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
    localize_html_chrome(&out_dir, dictionary)?;
    if !single_file {
        let assets = out_dir.join("assets");
        if std::fs::create_dir_all(&assets).is_err() {
            stats.write_failures += 1;
        }
        if std::fs::write(assets.join("theme.css"), processed_css).is_err() {
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

fn append_url_segments(base: &url::Url, segments: &[&str]) -> url::Url {
    let mut result = base.clone();
    if let Ok(mut path) = result.path_segments_mut() {
        path.pop_if_empty();
        for segment in segments {
            path.push(segment);
        }
        path.push("");
    }
    result
}

fn append_appearance_css(
    css: &str,
    appearance: &AppearanceConfig,
    custom_css: Option<&str>,
) -> String {
    let mut root = Vec::new();
    let mut dark = Vec::new();
    if let Some(color) = &appearance.accent {
        root.push(format!("--ui-color-primary: {}", color.as_str()));
        root.push(format!("--ui-color-link: {}", color.as_str()));
        dark.push(format!("--ui-color-primary: {}", color.as_str()));
        dark.push(format!("--ui-color-link: {}", color.as_str()));
    }
    if let Some(font) = &appearance.font_sans {
        root.push(format!("--ui-typescale-font-sans: {font}"));
    }
    if let Some(font) = &appearance.font_mono {
        root.push(format!("--ui-typescale-font-monospace: {font}"));
    }
    if root.is_empty() && dark.is_empty() && custom_css.is_none() {
        return css.to_string();
    }
    let mut result = css.to_string();
    result.push_str("\n@layer overrides {\n");
    if !root.is_empty() {
        result.push_str(":root {\n  ");
        result.push_str(&root.join(";\n  "));
        result.push_str(";\n}\n");
    }
    if !dark.is_empty() {
        result.push_str("[data-theme=\"dark\"] {\n  ");
        result.push_str(&dark.join(";\n  "));
        result.push_str(";\n}\n");
    }
    if let Some(custom_css) = custom_css {
        result.push_str(custom_css);
        result.push('\n');
    }
    result.push_str("}\n");
    result
}

fn write_json_artifact(
    path: &Path,
    value: &serde_json::Value,
) -> Result<(), evin_diagnostic::Diagnostic> {
    let text = serde_json::to_string_pretty(value).map_err(|error| {
        evin_diagnostic::Diagnostic::new(
            format!("Failed to serialize '{}': {error}", path.display()),
            evin_diagnostic::IssueCode::new("serialization-error"),
        )
    })?;
    write_text_artifact(path, &format!("{text}\n"))
}

fn write_text_artifact(path: &Path, contents: &str) -> Result<(), evin_diagnostic::Diagnostic> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| io_diagnostic(parent, error))?;
    }
    std::fs::write(path, contents).map_err(|error| io_diagnostic(path, error))
}

fn io_diagnostic(path: &Path, error: std::io::Error) -> evin_diagnostic::Diagnostic {
    evin_diagnostic::Diagnostic::new(
        format!("Failed to write '{}': {error}", path.display()),
        evin_diagnostic::IssueCode::new("io-error"),
    )
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn inject_version_banner(
    directory: &Path,
    current: &str,
    latest: &str,
    latest_url: &str,
    dictionary: &i18n::UiDictionary,
) -> Result<(), evin_diagnostic::Diagnostic> {
    let mut files = Vec::new();
    let mut pending = vec![directory.to_path_buf()];
    while let Some(current_dir) = pending.pop() {
        for entry in
            std::fs::read_dir(&current_dir).map_err(|error| io_diagnostic(&current_dir, error))?
        {
            let entry = entry.map_err(|error| io_diagnostic(&current_dir, error))?;
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.file_name().is_some_and(|name| name == "index.html") {
                files.push(path);
            }
        }
    }
    files.sort();
    for path in files {
        let mut html =
            std::fs::read_to_string(&path).map_err(|error| io_diagnostic(&path, error))?;
        let Some(article) = html.find("<article") else {
            continue;
        };
        let Some(open_end) = html[article..].find('>') else {
            continue;
        };
        let insert_at = article + open_end + 1;
        let banner = format!(
            "<p class=\"version-banner\"><a href=\"{}\">{}</a></p>",
            escape_html(latest_url),
            escape_html(&dictionary.version_banner(current, latest))
        );
        html.insert_str(insert_at, &banner);
        write_text_artifact(&path, &html)?;
    }
    Ok(())
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
    let site_ast = parse_configured_site(workspace_root, &config, &ctx.log, &mut stats)?;
    if ctx.mode == evin_action_sdk::PipelineMode::DryRun {
        let out_dir = if config.out.is_absolute() {
            config.out.clone()
        } else {
            workspace_root.join(&config.out)
        };
        let virtual_root = tempfile::tempdir().map_err(|error| {
            evin_diagnostic::Diagnostic::new(
                format!("Failed to prepare temporary docs output: {error}"),
                evin_diagnostic::IssueCode::new("io-error"),
            )
        })?;
        let mut preview_config = config.clone();
        preview_config.out = virtual_root.path().join("docs-output");
        render_docs(
            workspace_root,
            &preview_config,
            &site_ast,
            &ctx.log,
            &mut stats,
        )?;
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
        return Ok(());
    }
    let out_dir = render_docs(workspace_root, &config, &site_ast, &ctx.log, &mut stats)?;
    ctx.state
        .set_output("output_dir", &out_dir.to_string_lossy());
    ctx.state
        .set_output("page_count", &site_ast.pages.len().to_string());
    if config.mode == DocsMode::Serve {
        let host = resolve_serve_host(&config, invocation_flags);
        let binding = server::DevServer::bind(host, config.serve.port).await?;
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
        )
        .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
