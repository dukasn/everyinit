// Copyright (C) 2026 Eduardo Marques Santos
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.

use crate::render::{
    Layout, Mode, SYMBOL_DISCOVERY_TARGET, SYMBOL_ERROR, SYMBOL_FRAME_BAR, SYMBOL_INTRO,
    SYMBOL_OUTRO, SYMBOL_STEP, SYMBOL_SUCCESS, SYMBOL_WARN, Theme,
};
use console::{Style, style};
use evin_diagnostic::{Diagnostic, Severity};
use evin_io::SourceCache;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceIdentity {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetCard {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CountedTarget {
    pub name: String,
    pub task_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskGroup {
    /// Absolute `file:` URI for the manifest that declared these tasks.
    pub source: String,
    pub module: Option<String>,
    pub tasks: Vec<TargetCard>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogView {
    pub workspace: WorkspaceIdentity,
    pub groups: Vec<TaskGroup>,
    pub stages: Vec<CountedTarget>,
    pub hooks: Vec<CountedTarget>,
    pub modules: Vec<CountedTarget>,
    pub total_tasks: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetKind {
    Task,
    Stage,
    Hook,
    Workspace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetMetadata {
    pub name: String,
    pub kind: TargetKind,
    pub description: Option<String>,
    pub stage: Option<String>,
    /// Absolute `file:` URI for the target working directory.
    pub cwd: Option<String>,
    pub needs: Vec<String>,
    pub environment: Vec<String>,
    pub steps: Vec<String>,
    pub resolved_tasks: usize,
    pub modules: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionEntry {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplanationView {
    pub workspace: WorkspaceIdentity,
    pub target: TargetMetadata,
    pub execution: Vec<ExecutionEntry>,
    pub step_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailedExplanationView {
    pub workspace: WorkspaceIdentity,
    pub target: String,
    pub hint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestKind {
    Workspace,
    Module,
    Namespace,
    GitRules,
    Environment,
    Package,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedManifest {
    /// Absolute `file:` URI for the checked manifest.
    pub path: String,
    pub task_count: usize,
    pub kind: ManifestKind,
    pub issues: Vec<CheckIssueView>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckReportView {
    pub workspace: WorkspaceIdentity,
    pub checked_files: Vec<CheckedManifest>,
    pub total_tasks: usize,
    pub total_stages: usize,
    pub total_modules: usize,
    pub total_namespaces: usize,
    pub graph_nodes: usize,
    pub graph_edges: usize,
    pub graph_cycles: usize,
    pub issues: Vec<CheckIssueView>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckIssueView {
    pub diagnostic: Diagnostic,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckSelectionFailureView {
    pub workspace: WorkspaceIdentity,
    pub target: String,
    pub kind: String,
    pub detail: String,
}

impl CheckReportView {
    pub fn error_count(&self) -> usize {
        self.all_issues()
            .iter()
            .filter(|issue| issue.diagnostic.severity == Severity::Error)
            .count()
    }

    pub fn warning_count(&self) -> usize {
        self.all_issues()
            .iter()
            .filter(|issue| issue.diagnostic.severity == Severity::Warning)
            .count()
    }

    pub fn is_valid(&self) -> bool {
        self.error_count() == 0
    }

    fn all_issues(&self) -> Vec<&CheckIssueView> {
        self.checked_files
            .iter()
            .flat_map(|manifest| manifest.issues.iter())
            .chain(self.issues.iter())
            .collect()
    }
}

pub fn render_check(
    view: &CheckReportView,
    width: usize,
    source_cache: Option<&SourceCache>,
) -> String {
    let layout = Layout::with_width(width);
    let mut lines = header(&view.workspace);
    intro(&mut lines, "Workspace › Check");
    framed_text(
        &mut lines,
        &layout,
        &evin_locales::t!("check-description"),
        0,
        None,
    );
    bar(&mut lines);

    for manifest in &view.checked_files {
        let msg = match manifest.kind {
            ManifestKind::Workspace
            | ManifestKind::Module
            | ManifestKind::Namespace
            | ManifestKind::Package => {
                evin_locales::t!(
                    "check-checked-manifest",
                    path = manifest.path.as_str(),
                    tasks = manifest.task_count
                )
            }
            ManifestKind::GitRules => {
                evin_locales::t!("check-checked-rules", path = manifest.path.as_str())
            }
            ManifestKind::Environment => {
                evin_locales::t!("check-checked-env", path = manifest.path.as_str())
            }
        };
        let manifest_errors = manifest
            .issues
            .iter()
            .filter(|issue| issue.diagnostic.severity == Severity::Error)
            .count();
        let manifest_warnings = manifest
            .issues
            .iter()
            .filter(|issue| issue.diagnostic.severity == Severity::Warning)
            .count();
        if manifest_errors > 0 {
            severity_title(&mut lines, &layout, SYMBOL_ERROR, Severity::Error, &msg);
        } else if manifest_warnings > 0 {
            severity_title(&mut lines, &layout, SYMBOL_WARN, Severity::Warning, &msg);
        } else {
            check_manifest_section(&mut lines, &layout, &msg);
        }
        for issue in &manifest.issues {
            lines.extend(check_issue_fence(
                issue,
                layout,
                source_cache,
                manifest_errors > 0,
            ));
        }
    }

    let errors = view.error_count();
    let warnings = view.warning_count();
    if !view.issues.is_empty() {
        let (symbol, severity, title) = if errors > 0 {
            (
                SYMBOL_ERROR,
                Severity::Error,
                evin_locales::t!("check-errors-title"),
            )
        } else {
            (
                SYMBOL_WARN,
                Severity::Warning,
                evin_locales::t!("check-warnings-title"),
            )
        };
        severity_title(&mut lines, &layout, symbol, severity, &title);
        for issue in &view.issues {
            lines.extend(check_issue_fence(issue, layout, source_cache, errors > 0));
        }
    }

    let status_text = evin_locales::t!(
        "check-status-summary",
        status = if errors == 0 {
            evin_locales::t!("check-status-valid")
        } else {
            evin_locales::t!("check-status-invalid")
        },
        errors = errors,
        warnings = warnings
    );

    let targets_text = evin_locales::t!(
        "check-all-summary",
        tasks = view.total_tasks,
        stages = view.total_stages,
        modules = view.total_modules,
        namespaces = view.total_namespaces
    );

    let graph_text = evin_locales::t!(
        "check-graph-summary",
        nodes = view.graph_nodes,
        edges = view.graph_edges,
        cycles = view.graph_cycles
    );

    lines.extend(crate::render::summary_lines(
        layout,
        Mode::Normal,
        evin_locales::t!("check-summary-title"),
        &[
            format!(
                "{}: {status_text}",
                dim(&evin_locales::t!("check-status-label"))
            ),
            format!(
                "{}: {targets_text}",
                dim(&evin_locales::t!("check-targets-label"))
            ),
            format!(
                "{}: {graph_text}",
                dim(&evin_locales::t!("check-graph-label"))
            ),
        ],
    ));

    if errors == 0 {
        outro(&mut lines, &evin_locales::t!("discovery-finished"));
    } else {
        lines.push(format!(
            "{}  {}",
            Theme::frame(SYMBOL_OUTRO),
            Style::new()
                .red()
                .apply_to(evin_locales::t!("check-finished-failure"))
        ));
    }
    lines.push(String::new());
    lines.push(dim(&evin_locales::t!(
        "check-result-summary",
        tasks = view.total_tasks,
        errors = errors,
        warnings = warnings
    )));
    lines.push(String::new());
    lines.join("\n")
}

pub fn render_check_selection_failure(view: &CheckSelectionFailureView, width: usize) -> String {
    let layout = Layout::with_width(width);
    let mut lines = header(&view.workspace);
    intro(&mut lines, "Workspace › Check");
    let title = format!("{}  {}", view.target, dim(&format!("<{}>", view.kind)));
    error_title(&mut lines, &layout, &title);
    suggestion_lines(&mut lines, &layout, &view.detail);
    lines.push(format!(
        "{}  {}",
        Theme::frame(SYMBOL_OUTRO),
        Style::new()
            .red()
            .apply_to(evin_locales::t!("check-finished-failure"))
    ));
    lines.push(String::new());
    lines.push(dim(&evin_locales::t!("check-selection-summary")));
    lines.push(String::new());
    lines.join("\n")
}

pub fn render_catalog(view: &CatalogView, width: usize) -> String {
    let layout = Layout::with_width(width);
    let mut lines = header(&view.workspace);
    intro(&mut lines, "Workspace › Catalog");
    framed_text(
        &mut lines,
        &layout,
        &evin_locales::t!("discovery-catalog-description"),
        0,
        None,
    );
    bar(&mut lines);

    for group in &view.groups {
        let source = Style::new().cyan().underlined().apply_to(&group.source);
        success_section(
            &mut lines,
            &format!(
                "{} ({source})",
                evin_locales::t!("discovery-tasks-at-title")
            ),
        );
        task_table(&mut lines, &layout, &group.tasks);
        bar(&mut lines);
    }

    let summary_details = [
        summary_detail(&evin_locales::t!("discovery-stages-title"), &view.stages),
        summary_detail(&evin_locales::t!("discovery-hooks-title"), &view.hooks),
        summary_detail(&evin_locales::t!("discovery-modules-title"), &view.modules),
    ];
    lines.extend(crate::render::summary_lines(
        layout,
        Mode::Normal,
        evin_locales::t!("discovery-catalog-summary-title"),
        &summary_details,
    ));

    outro(&mut lines, &evin_locales::t!("discovery-finished"));
    lines.push(String::new());
    let summary = evin_locales::t!("discovery-catalog-all-summary", count = view.total_tasks);
    lines.push(dim(&summary));
    lines.push(String::new());
    lines.join("\n")
}

pub fn render_explanation(view: &ExplanationView, width: usize) -> String {
    let layout = Layout::with_width(width);
    let mut lines = header(&view.workspace);
    intro(&mut lines, "Workspace › Explain");

    let target_title = format!(
        "{}  {}",
        view.target.name,
        dim(&format!(
            "<{}>",
            capitalize(&target_kind_label(&view.target.kind))
        ))
    );
    status_title(&mut lines, &layout, SYMBOL_DISCOVERY_TARGET, &target_title);
    if let Some(description) = &view.target.description {
        framed_text(&mut lines, &layout, description, 1, None);
        bar(&mut lines);
    }
    metadata(&mut lines, &layout, &view.target);
    bar(&mut lines);

    if view.target.kind == TargetKind::Task {
        section(&mut lines, &evin_locales::t!("discovery-steps-title"));
        if view.target.steps.is_empty() {
            framed_text(&mut lines, &layout, "(none)", 1, None);
        } else {
            for step in &view.target.steps {
                let command = step
                    .strip_prefix("run: ")
                    .or_else(|| step.strip_prefix("uses: "))
                    .unwrap_or(step);
                framed_text(
                    &mut lines,
                    &layout,
                    &format!("(action) ↩  {command}"),
                    1,
                    None,
                );
            }
        }
        bar(&mut lines);
    }

    execution_section(
        &mut lines,
        &evin_locales::t!("discovery-execution-plan-title"),
    );
    if view.execution.is_empty() {
        framed_text(
            &mut lines,
            &layout,
            &evin_locales::t!("discovery-no-tasks"),
            0,
            None,
        );
    } else {
        for (index, entry) in view.execution.iter().enumerate() {
            let number = style(format!("{:02}", index + 1)).green().to_string();
            let text = format!("{number}  {}  {}", entry.name, dim(&entry.description));
            framed_text(&mut lines, &layout, &text, 0, None);
        }
    }
    bar(&mut lines);
    outro(&mut lines, &evin_locales::t!("discovery-finished"));
    lines.push(String::new());
    lines.push(dim(&evin_locales::t!(
        "discovery-explain-summary",
        tasks = view.execution.len(),
        steps = view.step_count
    )));
    lines.push(String::new());
    lines.join("\n")
}

pub fn render_failed_explanation(view: &FailedExplanationView, width: usize) -> String {
    let layout = Layout::with_width(width);
    let mut lines = header(&view.workspace);
    intro(&mut lines, "Workspace › Explain");
    let title = format!(
        "{}  {}",
        view.target,
        dim(&evin_locales::t!("discovery-kind-unknown"))
    );
    error_title(&mut lines, &layout, &title);
    if let Some(hint) = &view.hint {
        let hint = console::strip_ansi_codes(hint);
        let wrapped = layout.wrap_with_margin(&hint, 3);
        for (index, line) in wrapped.iter().enumerate() {
            if index == 0 {
                lines.push(format!("{}   {line}", Style::new().blue().apply_to("•")));
            } else {
                lines.push(format!("{}   {line}", Theme::frame(SYMBOL_FRAME_BAR)));
            }
        }
        bar(&mut lines);
    }
    lines.push(format!(
        "{}  {}",
        Theme::frame(SYMBOL_OUTRO),
        Style::new()
            .red()
            .apply_to(evin_locales::t!("discovery-failed"))
    ));
    lines.push(String::new());
    lines.join("\n")
}

fn header(workspace: &WorkspaceIdentity) -> Vec<String> {
    vec![format!(
        "{} {} · {} {}",
        dim("evin:"),
        workspace.version,
        dim("workspace:"),
        workspace.name
    )]
}

fn intro(lines: &mut Vec<String>, title: &str) {
    lines.push(String::new());
    lines.push(format!(
        "{}  {}",
        Theme::frame(SYMBOL_INTRO),
        Mode::Normal.title_style().apply_to(format!(" {title} "))
    ));
    bar(lines);
}

fn error_title(lines: &mut Vec<String>, layout: &Layout, title: &str) {
    let wrapped = layout.wrap_with_margin(title, 3);
    for (index, line) in wrapped.iter().enumerate() {
        if index == 0 {
            lines.push(format!(
                "{}   {}",
                Style::new().red().apply_to(crate::render::SYMBOL_ERROR),
                Style::new().bold().apply_to(line)
            ));
        } else {
            lines.push(format!("{}   {line}", Theme::frame(SYMBOL_FRAME_BAR)));
        }
    }
    bar(lines);
}

fn severity_title(
    lines: &mut Vec<String>,
    layout: &Layout,
    symbol: &str,
    severity: Severity,
    title: &str,
) {
    let wrapped = layout.wrap_with_margin(title, 3);
    for (index, line) in wrapped.iter().enumerate() {
        if index == 0 {
            lines.push(format!(
                "{}   {}",
                Theme::severity_style(severity, symbol),
                Style::new().bold().apply_to(line)
            ));
        } else {
            lines.push(format!("{}   {line}", Theme::frame(SYMBOL_FRAME_BAR)));
        }
    }
    bar(lines);
}

fn suggestion_lines(lines: &mut Vec<String>, layout: &Layout, hint: &str) {
    let hint = console::strip_ansi_codes(hint);
    let wrapped = layout.wrap_with_margin(&hint, 3);
    for (index, line) in wrapped.iter().enumerate() {
        if index == 0 {
            lines.push(format!("{}   {line}", Style::new().blue().apply_to("•")));
        } else {
            lines.push(format!("{}   {line}", Theme::frame(SYMBOL_FRAME_BAR)));
        }
    }
    bar(lines);
}

fn check_issue_fence(
    issue: &CheckIssueView,
    layout: Layout,
    source_cache: Option<&SourceCache>,
    aborting: bool,
) -> Vec<String> {
    let severity = issue.diagnostic.severity;
    let label = match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "info",
    };
    let title = format!(
        "({label}) ↩ {} [{}]",
        issue.diagnostic.title, issue.diagnostic.code
    );
    let mut body = layout.wrap_with_margin(&issue.message, 6);
    if let Some(hint) = &issue.diagnostic.hint {
        body.push(String::new());
        let hint = console::strip_ansi_codes(hint);
        let wrapped = layout.wrap_with_margin(&hint, 8);
        for (index, line) in wrapped.iter().enumerate() {
            if index == 0 {
                body.push(format!("{} {line}", Theme::remark_symbol()));
            } else {
                body.push(format!("  {line}"));
            }
        }
    }
    if issue.diagnostic.location.is_some()
        && let Some(snippet) = crate::diagnostics::snippet::SnippetRenderer::new()
            .with_cache(source_cache)
            .render_with_layout(&issue.diagnostic, &issue.message, layout)
    {
        body.push(String::new());
        body.extend(snippet);
    }
    let footer = if severity == Severity::Warning && !aborting {
        evin_locales::t!("check-fence-review")
    } else {
        evin_locales::t!("check-fence-aborted")
    };
    crate::render::fenced_lines(
        layout,
        &Theme::default(),
        &title,
        &body,
        Some(severity),
        Some(&footer),
        true,
    )
}

fn status_title(lines: &mut Vec<String>, layout: &Layout, symbol: &str, title: &str) {
    let wrapped = layout.wrap_with_margin(title, 3);
    for (index, line) in wrapped.iter().enumerate() {
        if index == 0 {
            lines.push(format!(
                "{}   {}",
                Style::new().green().apply_to(symbol),
                Style::new().bold().apply_to(line)
            ));
        } else {
            lines.push(format!("{}   {line}", Theme::frame(SYMBOL_FRAME_BAR)));
        }
    }
    bar(lines);
}

fn section(lines: &mut Vec<String>, title: &str) {
    lines.push(format!(
        "{}  {}",
        Style::new().green().apply_to(SYMBOL_STEP),
        Style::new().bold().apply_to(title)
    ));
    bar(lines);
}

fn success_section(lines: &mut Vec<String>, title: &str) {
    lines.push(format!(
        "{}  {}",
        Style::new().green().apply_to(SYMBOL_SUCCESS),
        Style::new().bold().apply_to(title)
    ));
    bar(lines);
}

fn check_manifest_section(lines: &mut Vec<String>, layout: &Layout, title: &str) {
    let wrapped = layout.wrap_with_margin(title, 3);
    for (index, line) in wrapped.iter().enumerate() {
        if index == 0 {
            lines.push(format!(
                "{}  {}",
                Style::new().green().apply_to(SYMBOL_SUCCESS),
                Style::new().bold().apply_to(line)
            ));
        } else {
            lines.push(format!("{}   {line}", Theme::frame(SYMBOL_FRAME_BAR)));
        }
    }
    bar(lines);
}

fn execution_section(lines: &mut Vec<String>, title: &str) {
    lines.push(format!(
        "{}  {}",
        Style::new()
            .color256(244)
            .apply_to(crate::render::SYMBOL_DEBUG),
        Style::new().bold().apply_to(title)
    ));
    bar(lines);
}

fn capitalize(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn task_table(lines: &mut Vec<String>, layout: &Layout, tasks: &[TargetCard]) {
    if tasks.is_empty() {
        return;
    }
    let name_width = tasks
        .iter()
        .map(|task| console::measure_text_width(&task.name))
        .max()
        .unwrap_or_default();
    for task in tasks {
        let description_width = layout.inner_width().saturating_sub(name_width + 5).max(20);
        let wrapped = textwrap::wrap(
            &task.description,
            textwrap::Options::new(description_width)
                .break_words(true)
                .word_splitter(textwrap::WordSplitter::NoHyphenation),
        );
        for (index, description) in wrapped.iter().enumerate() {
            let name = if index == 0 {
                format!("{:<name_width$}", task.name)
            } else {
                " ".repeat(name_width)
            };
            lines.push(format!(
                "{}   {}  {description}",
                Theme::frame(SYMBOL_FRAME_BAR),
                Style::new().bold().apply_to(name)
            ));
        }
    }
}

fn summary_detail(label: &str, items: &[CountedTarget]) -> String {
    let separator = dim(" - ");
    let values = items
        .iter()
        .map(|item| {
            format!(
                "{}{}",
                Style::new().cyan().apply_to(&item.name),
                dim(&format!("({})", item.task_count))
            )
        })
        .collect::<Vec<_>>()
        .join(&separator);
    format!("{}: {values}", dim(label))
}

fn metadata(lines: &mut Vec<String>, layout: &Layout, target: &TargetMetadata) {
    if target.kind != TargetKind::Task {
        if target.kind == TargetKind::Stage {
            metadata_bullet(lines, layout, "tasks", &target.resolved_tasks.to_string());
            metadata_uri_bullet(lines, "cwd", target.cwd.as_deref().unwrap_or("(none)"));
            metadata_bullet(lines, layout, "modules", &modules_or_none(&target.modules));
            return;
        }
        framed_text(
            lines,
            layout,
            &format!(
                "{}  {}",
                dim(&evin_locales::t!("discovery-resolved-label")),
                evin_locales::t!("discovery-task-count", count = target.resolved_tasks)
            ),
            1,
            None,
        );
        return;
    }
    metadata_bullet(
        lines,
        layout,
        &evin_locales::t!("discovery-stage-label"),
        target.stage.as_deref().unwrap_or("(none)"),
    );
    metadata_uri_bullet(
        lines,
        &evin_locales::t!("discovery-cwd-label"),
        target.cwd.as_deref().unwrap_or("."),
    );
    metadata_bullet(
        lines,
        layout,
        &evin_locales::t!("discovery-needs-label"),
        &joined_or_none(&target.needs),
    );
    metadata_bullet(
        lines,
        layout,
        &evin_locales::t!("discovery-environment-label"),
        &joined_or_none(&target.environment),
    );
}

fn metadata_uri_bullet(lines: &mut Vec<String>, label: &str, uri: &str) {
    // A file URI is one copyable token. Wrapping it with frame gutters inserts
    // characters into the URI, so let terminals visually wrap the single line.
    lines.push(format!(
        "{}  • {}: {uri}",
        Theme::frame(SYMBOL_FRAME_BAR),
        dim(label),
    ));
}

fn metadata_bullet(lines: &mut Vec<String>, layout: &Layout, label: &str, value: &str) {
    framed_text(
        lines,
        layout,
        &format!("• {}: {value}", dim(label)),
        1,
        None,
    );
}

fn target_kind_label(kind: &TargetKind) -> String {
    match kind {
        TargetKind::Task => evin_locales::t!("discovery-kind-task"),
        TargetKind::Stage => evin_locales::t!("discovery-kind-stage"),
        TargetKind::Hook => evin_locales::t!("discovery-kind-hook"),
        TargetKind::Workspace => evin_locales::t!("discovery-kind-workspace"),
    }
}

fn joined_or_none(values: &[String]) -> String {
    if values.is_empty() {
        "(none)".to_string()
    } else {
        values.join(" · ")
    }
}

fn modules_or_none(values: &[String]) -> String {
    if values.is_empty() {
        "(none)".to_string()
    } else {
        values.join(" ")
    }
}

fn framed_text(
    lines: &mut Vec<String>,
    layout: &Layout,
    text: &str,
    indent: usize,
    text_style: Option<&Style>,
) {
    let padding = "   ".repeat(indent);
    for line in layout.wrap_with_margin(text, padding.len()) {
        let line = match text_style {
            Some(style) => style.apply_to(line).to_string(),
            None => line,
        };
        lines.push(format!(
            "{}  {padding}{line}",
            Theme::frame(SYMBOL_FRAME_BAR)
        ));
    }
}

fn bar(lines: &mut Vec<String>) {
    lines.push(Theme::frame(SYMBOL_FRAME_BAR));
}

fn outro(lines: &mut Vec<String>, message: &str) {
    lines.push(format!(
        "{}  {}",
        Theme::frame(SYMBOL_OUTRO),
        Style::new().green().apply_to(message)
    ));
}

fn dim(value: &str) -> String {
    Style::new().color256(244).apply_to(value).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use evin_diagnostic::IssueCode;

    fn workspace() -> WorkspaceIdentity {
        WorkspaceIdentity {
            name: "demo".into(),
            version: "1.2.3".into(),
        }
    }

    #[test]
    fn catalog_plain_snapshot_has_frame_and_sections() {
        let output = render_catalog(
            &CatalogView {
                workspace: workspace(),
                groups: vec![TaskGroup {
                    source: "file:///repo/.evin/workspace.flow".into(),
                    module: None,
                    tasks: vec![TargetCard {
                        name: "verify".into(),
                        description: "Validate everything".into(),
                    }],
                }],
                stages: vec![CountedTarget {
                    name: "Test".into(),
                    task_count: 1,
                }],
                hooks: vec![CountedTarget {
                    name: "pre-commit".into(),
                    task_count: 1,
                }],
                modules: vec![],
                total_tasks: 3,
            },
            80,
        );
        let output = console::strip_ansi_codes(&output);
        assert!(output.contains("╭   Workspace › Catalog "));
        assert!(output.contains("✓  Tasks at (file:///repo/.evin/workspace.flow)"));
        assert!(output.contains("│   verify  Validate everything"));
        assert!(output.contains("• Stages: Test(1)"));
        assert!(output.contains("╰  Finished!"));
    }

    #[test]
    fn narrow_explanation_keeps_every_line_in_the_frame() {
        let output = render_explanation(
            &ExplanationView {
                workspace: workspace(),
                target: TargetMetadata {
                    name: "verify".into(),
                    kind: TargetKind::Task,
                    description: Some("A deliberately long target description for wrapping".into()),
                    stage: Some("Test".into()),
                    cwd: Some(".".into()),
                    needs: vec!["check".into()],
                    environment: vec!["TOKEN".into()],
                    steps: vec!["cargo test --workspace --locked".into()],
                    resolved_tasks: 2,
                    modules: Vec::new(),
                },
                execution: vec![ExecutionEntry {
                    name: "verify".into(),
                    description: "A deliberately long execution description".into(),
                }],
                step_count: 1,
            },
            36,
        );
        let output = console::strip_ansi_codes(&output);
        assert!(output.contains("│"));
        assert!(output.contains("»   verify  <Task>"));
        assert!(output.contains("• environment: TOKEN"));
        assert!(output.contains("◇  Steps"));
        assert!(output.contains("(action) ↩"));
        assert!(output.contains("~  Execution Plan"));
        assert!(output.contains("nothing was executed"));
    }

    #[test]
    fn narrow_explanation_keeps_cwd_uri_intact() {
        let uri =
            "file:///private/var/folders/temporary/workspace%20with%20spaces/nested%20directory";
        let output = render_explanation(
            &ExplanationView {
                workspace: workspace(),
                target: TargetMetadata {
                    name: "inspect".into(),
                    kind: TargetKind::Task,
                    description: None,
                    stage: Some("Test".into()),
                    cwd: Some(uri.into()),
                    needs: Vec::new(),
                    environment: Vec::new(),
                    steps: Vec::new(),
                    resolved_tasks: 1,
                    modules: Vec::new(),
                },
                execution: Vec::new(),
                step_count: 0,
            },
            36,
        );
        let output = console::strip_ansi_codes(&output);
        assert!(output.contains(uri), "{output}");
    }

    #[test]
    fn failed_explanation_uses_the_discovery_frame() {
        let output = render_failed_explanation(
            &FailedExplanationView {
                workspace: workspace(),
                target: "Testy".into(),
                hint: Some("Did you mean 'Test'?".into()),
            },
            80,
        );
        let output = console::strip_ansi_codes(&output);
        assert!(output.contains("╭   Workspace › Explain "));
        assert!(output.contains("×   Testy  unknown"));
        assert!(output.contains("•   Did you mean 'Test'?"));
        assert!(output.contains("╰  Failed!"));
    }

    #[test]
    fn check_view_renders_validation_card() {
        let output = render_check(
            &CheckReportView {
                workspace: workspace(),
                checked_files: vec![
                    CheckedManifest {
                        path: ".evin/workspace.flow".into(),
                        task_count: 5,
                        kind: ManifestKind::Workspace,
                        issues: Vec::new(),
                    },
                    CheckedManifest {
                        path: ".evin/gitrules.flow".into(),
                        task_count: 0,
                        kind: ManifestKind::GitRules,
                        issues: Vec::new(),
                    },
                ],
                total_tasks: 5,
                total_stages: 2,
                total_modules: 0,
                total_namespaces: 0,
                graph_nodes: 5,
                graph_edges: 3,
                graph_cycles: 0,
                issues: Vec::new(),
            },
            80,
            None,
        );
        let output = console::strip_ansi_codes(&output);
        assert!(output.contains("╭   Workspace › Check "));
        assert!(output.contains("Checked .evin/workspace.flow (5 tasks)"));
        assert!(output.contains("Checked .evin/gitrules.flow"));
        assert!(output.contains("Validation Summary"));
        assert!(output.contains("Status: Valid (0 errors, 0 warnings)"));
        assert!(output.contains("5 tasks, 2 stages, 0 modules, 0 namespaces"));
        assert!(output.contains("5 nodes, 3 edges, 0 cycles detected"));
        assert!(output.contains("╰  Finished!"));
    }

    #[test]
    fn check_error_renders_a_fenced_source_diagnostic() {
        let source_cache = SourceCache::default();
        let source = "tasks:\n  publish:\n    steps:\n      - uses: example/action@v1\n        with: '{{ old.syntax }}'\n";
        source_cache.insert(".evin/workspace.flow", source);
        let offset = source.find("{{ old.syntax }}").unwrap();
        let diagnostic = Diagnostic::new(
            "Invalid action configuration",
            IssueCode::new("issue-action-config-invalid"),
        )
        .with_file_span(
            ".evin/workspace.flow",
            offset,
            16,
            Some("invalid template expression".into()),
        );
        let issue = CheckIssueView {
            diagnostic,
            message: "Task 'publish' contains an invalid action template".into(),
        };
        let output = render_check(
            &CheckReportView {
                workspace: workspace(),
                checked_files: vec![CheckedManifest {
                    path: ".evin/workspace.flow".into(),
                    task_count: 1,
                    kind: ManifestKind::Workspace,
                    issues: vec![issue],
                }],
                total_tasks: 1,
                total_stages: 1,
                total_modules: 0,
                total_namespaces: 0,
                graph_nodes: 1,
                graph_edges: 0,
                graph_cycles: 0,
                issues: Vec::new(),
            },
            80,
            Some(&source_cache),
        );
        let output = console::strip_ansi_codes(&output);

        assert!(output.contains("╭─(error) ↩ Invalid action configuration"));
        assert!(output.contains("×   Checked .evin/workspace.flow (1 task)"));
        assert!(output.contains(".evin/workspace.flow"));
        assert!(output.contains("with: '{{ old.syntax }}'"));
        assert!(output.contains("invalid template express"));
        assert!(output.contains("╰─(action) ↩ aborted"));
        assert!(output.contains("╰  Failed!"));
        assert!(output.contains("1 task · 1 error · 0 warnings · nothing was executed"));
    }

    #[test]
    fn warning_only_check_finishes_successfully() {
        let diagnostic = Diagnostic::new(
            "Workspace warning",
            IssueCode::new("issue-check-workspace-warning"),
        )
        .with_severity(Severity::Warning);
        let output = render_check(
            &CheckReportView {
                workspace: workspace(),
                checked_files: Vec::new(),
                total_tasks: 2,
                total_stages: 1,
                total_modules: 0,
                total_namespaces: 0,
                graph_nodes: 2,
                graph_edges: 1,
                graph_cycles: 0,
                issues: vec![CheckIssueView {
                    diagnostic,
                    message: "A deprecated field is still accepted".into(),
                }],
            },
            48,
            None,
        );
        let output = console::strip_ansi_codes(&output);

        assert!(output.contains("╭─(warning) ↩ Workspace warning"));
        assert!(output.contains("╰─(action) ↩ review"));
        assert!(output.contains("Status: Valid (0 errors, 1 warning)"));
        assert!(output.contains("╰  Finished!"));
    }

    #[test]
    fn check_selection_failure_uses_the_compact_discovery_frame() {
        let output = render_check_selection_failure(
            &CheckSelectionFailureView {
                workspace: workspace(),
                target: "releaze".into(),
                kind: "unknown module".into(),
                detail: "Did you mean 'release'?".into(),
            },
            44,
        );
        let output = console::strip_ansi_codes(&output);

        assert!(output.contains("╭   Workspace › Check "));
        assert!(output.contains("×   releaze  <unknown module>"));
        assert!(output.contains("•   Did you mean 'release'?"));
        assert!(output.contains("╰  Failed!"));
        assert!(output.contains("No manifests were checked"));
    }
}
