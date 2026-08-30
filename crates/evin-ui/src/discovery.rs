// Copyright (C) 2026 Eduardo Marques Santos
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.

use crate::render::{
    Layout, Mode, SYMBOL_DISCOVERY_TARGET, SYMBOL_FRAME_BAR, SYMBOL_INTRO, SYMBOL_OUTRO,
    SYMBOL_STEP, SYMBOL_SUCCESS, Theme,
};
use console::{Style, style};

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
        let source = Style::new()
            .cyan()
            .underlined()
            .apply_to(format!("file://{}", group.source));
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
            metadata_bullet(
                lines,
                layout,
                "cwd",
                target.cwd.as_deref().unwrap_or("file://"),
            );
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
    metadata_bullet(
        lines,
        layout,
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
                    source: "/repo/.evin/workspace.yaml".into(),
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
        assert!(output.contains("✓  Tasks at (file:///repo/.evin/workspace.yaml)"));
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
}
