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

use super::dsl::TapeConfig;
use super::screen::{CellAttributes, TerminalColor, TerminalFrame};

const FINAL_FRAME_HOLD_MS: u32 = 1_000;
const CURSOR_WIDTH: u32 = 2;

/// The immutable cell geometry used by every tape. IBM Plex Mono is bundled
/// with the documentation action, so the SVG and its web font agree on these
/// measurements instead of depending on the page's code-font setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalMetrics {
    pub font_size: u32,
    pub cell_width: u32,
    pub row_height: u32,
    pub baseline: u32,
    pub inset: u32,
}

pub const IBM_PLEX_MONO_METRICS: TerminalMetrics = TerminalMetrics {
    font_size: 14,
    cell_width: 9,
    row_height: 20,
    baseline: 15,
    inset: 16,
};

pub fn render_svg(config: &TapeConfig, frames: &[TerminalFrame], description: &str) -> String {
    let fallback = TerminalFrame {
        timestamp_ms: 0,
        rows: config.rows,
        cols: config.cols,
        cells: vec![Default::default(); usize::from(config.rows) * usize::from(config.cols)],
        cursor_row: 0,
        cursor_col: 0,
    };
    let frames = if frames.is_empty() {
        std::slice::from_ref(&fallback)
    } else {
        frames
    };
    let metrics = IBM_PLEX_MONO_METRICS;
    let rendered_rows = if config.auto_rows {
        frames
            .iter()
            .map(occupied_rows)
            .max()
            .unwrap_or(1)
            .clamp(3, config.rows)
    } else {
        config.rows
    };
    let row_width = u32::from(config.cols) * metrics.cell_width;
    let width = row_width + metrics.inset * 2;
    let height = u32::from(rendered_rows) * metrics.row_height + metrics.inset * 2;
    // A few executor operations can produce distinct screen snapshots at the
    // same logical instant. Give those snapshots a strictly increasing visual
    // timestamp so their cursor groups can never be visible together.
    let timeline = frames
        .iter()
        .scan(None, |previous: &mut Option<u32>, frame| {
            let timestamp = previous.map_or(frame.timestamp_ms, |last| {
                frame.timestamp_ms.max(last.saturating_add(1))
            });
            *previous = Some(timestamp);
            Some(timestamp)
        })
        .collect::<Vec<_>>();
    let duration = timeline
        .last()
        .copied()
        .map(|timestamp| timestamp.saturating_add(FINAL_FRAME_HOLD_MS).max(1))
        .unwrap_or(FINAL_FRAME_HOLD_MS);
    let title = config.title.as_deref().unwrap_or("Terminal recording");
    let tape_id = stable_tape_id(config, frames, description);
    let cols = config.cols;
    let mut output = format!(
        "<svg class=\"terminal-tape-svg\" data-tape-duration-ms=\"{duration}\" data-tape-cols=\"{cols}\" data-tape-rows=\"{rendered_rows}\" font-size=\"{}\" xml:space=\"preserve\" role=\"img\" aria-labelledby=\"tape-title-{tape_id:016x} tape-desc-{tape_id:016x}\" style=\"--tape-width:{width}px;--tape-height:{height}px;--tape-cols:{cols};--tape-rows:{rendered_rows};--tape-aspect:{width}/{height};user-select:none;-webkit-user-select:none;pointer-events:none\" viewBox=\"0 0 {width} {height}\" xmlns=\"http://www.w3.org/2000/svg\"><title id=\"tape-title-{tape_id:016x}\">{}</title><desc id=\"tape-desc-{tape_id:016x}\">{}</desc><style>",
        metrics.font_size,
        escape(title),
        escape(description)
    );
    output.push_str(".terminal-tape-svg{--term-bg:var(--ui-color-code-surface);--term-fg:var(--ui-color-code-on-surface);background:var(--term-bg);overflow:hidden}.terminal-tape-svg,.tape-frame,.tape-frame text,.tape-frame tspan{font-family:'IBM Plex Mono','Cascadia Mono','Cascadia Code','Fira Code',ui-monospace,Menlo,Monaco,Consolas,monospace;font-kerning:none;font-variant-ligatures:none;font-synthesis:none;text-anchor:start;dominant-baseline:alphabetic}");
    output.push_str(".term-default{fill:var(--term-fg)}.term-command,.term-builtin{fill:var(--ui-syn-function)}.term-keyword{fill:var(--ui-syn-keyword)}.term-flag,.term-punctuation{fill:var(--ui-syn-punctuation)}.term-variable{fill:var(--ui-syn-variable)}.term-string{fill:var(--ui-syn-string)}.term-number{fill:var(--ui-syn-constant)}.term-operator{fill:var(--ui-syn-operator)}.term-comment{fill:var(--ui-syn-comment)}.term-ansi-0{fill:var(--ui-palette-black);stroke:var(--ui-palette-black)}.term-ansi-1{fill:var(--ui-palette-red);stroke:var(--ui-palette-red)}.term-ansi-2{fill:var(--ui-palette-green);stroke:var(--ui-palette-green)}.term-ansi-3{fill:var(--ui-palette-yellow);stroke:var(--ui-palette-yellow)}.term-ansi-4{fill:var(--ui-palette-blue);stroke:var(--ui-palette-blue)}.term-ansi-5{fill:var(--ui-palette-magenta);stroke:var(--ui-palette-magenta)}.term-ansi-6{fill:var(--ui-palette-cyan);stroke:var(--ui-palette-cyan)}.term-ansi-7,.term-ansi-8,.term-ansi-15{fill:var(--ui-palette-white);stroke:var(--ui-palette-white)}.term-ansi-9{fill:var(--ui-palette-red);stroke:var(--ui-palette-red)}.term-ansi-10{fill:var(--ui-palette-green);stroke:var(--ui-palette-green)}.term-ansi-11{fill:var(--ui-palette-yellow);stroke:var(--ui-palette-yellow)}.term-ansi-12{fill:var(--ui-palette-blue);stroke:var(--ui-palette-blue)}.term-ansi-13{fill:var(--ui-palette-magenta);stroke:var(--ui-palette-magenta)}.term-ansi-14{fill:var(--ui-palette-cyan);stroke:var(--ui-palette-cyan)}.term-bold{font-weight:700}.term-dim{opacity:0.7}.term-italic{font-style:italic}.term-underline{text-decoration:underline}.term-strikethrough{text-decoration:line-through}.term-overline{text-decoration:overline}.term-hidden{opacity:0}.term-invert-fg{fill:var(--term-bg)}.term-invert-bg{fill:var(--term-fg)}.term-underline-curly{fill:none;stroke-width:1.2;stroke-linecap:round}");
    output.push_str("</style>");
    output.push_str(&format!(
        "<rect width=\"{width}\" height=\"{height}\" fill=\"var(--term-bg)\"/>"
    ));
    for (index, frame) in frames.iter().enumerate() {
        let display = if index + 1 == frames.len() {
            "inline"
        } else {
            "none"
        };
        output.push_str(&format!(
            "<g class=\"tape-frame\" data-tape-frame-start-ms=\"{}\" display=\"{display}\">",
            timeline[index]
        ));
        for row in 0..frame.rows.min(rendered_rows) {
            let start = usize::from(row) * usize::from(frame.cols);
            let cells = &frame.cells[start..start + usize::from(frame.cols)];
            write_row_backgrounds(&mut output, cells, row, metrics);
            output.push_str(&format!(
                "<text x=\"{}\" y=\"{}\">",
                metrics.inset,
                metrics.inset + u32::from(row) * metrics.row_height + metrics.baseline
            ));
            write_row(&mut output, cells, metrics);
            output.push_str("</text>");
            write_row_underlines(&mut output, cells, row, metrics);
            write_row_decorations(&mut output, cells, row, metrics);
        }
        if frame.cursor_row < rendered_rows {
            output.push_str(&format!(
                "<rect class=\"term-cursor\" x=\"{}\" y=\"{}\" width=\"{CURSOR_WIDTH}\" height=\"{}\" fill=\"var(--term-fg)\"/>",
                metrics.inset + u32::from(frame.cursor_col) * metrics.cell_width,
                metrics.inset + u32::from(frame.cursor_row) * metrics.row_height,
                metrics.row_height,
            ));
        }
        output.push_str("</g>");
    }
    output.push_str("</svg>");
    output
}

/// A stable per-recording namespace prevents keyframe and accessibility-ID
/// collisions when several inline SVG tapes appear on the same page.
fn stable_tape_id(config: &TapeConfig, frames: &[TerminalFrame], description: &str) -> u64 {
    let fingerprint = format!("{config:?}{frames:?}{description}");
    fingerprint
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
}

/// Returns the number of visual rows needed to render a frame. Cursor rows
/// count too: an empty line where input is being typed is still part of the
/// terminal state authors expect to see.
fn occupied_rows(frame: &TerminalFrame) -> u16 {
    let last_content_row = (0..frame.rows).rev().find(|row| {
        let start = usize::from(*row) * usize::from(frame.cols);
        frame.cells[start..start + usize::from(frame.cols)]
            .iter()
            .any(|cell| cell.ch != ' ')
    });
    last_content_row
        .map(|row| row.saturating_add(1))
        .unwrap_or(0)
        .max(frame.cursor_row.saturating_add(1))
}

fn is_blank_default(cell: &super::screen::TerminalCell) -> bool {
    if cell.ch != ' ' {
        return false;
    }
    let (_, effective_bg) = cell.attributes.effective_colors();
    effective_bg == super::screen::TerminalColor::Default
        && !cell.attributes.invert
        && !cell.attributes.underline
        && !cell.attributes.strikethrough
        && !cell.attributes.overline
}

fn write_row(output: &mut String, cells: &[super::screen::TerminalCell], metrics: TerminalMetrics) {
    let end_col = cells
        .iter()
        .rposition(|c| !is_blank_default(c))
        .map(|idx| idx + 1)
        .unwrap_or(0);
    if end_col == 0 {
        return;
    }
    let cells = &cells[..end_col];

    let mut column = 0;
    while column < cells.len() {
        if cells[column].continuation {
            column += 1;
            continue;
        }

        let attributes = &cells[column].attributes;
        let mut positions = String::new();
        let mut content = String::new();
        while column < cells.len() {
            let cell = &cells[column];
            if cell.continuation {
                column += 1;
                continue;
            }
            if &cell.attributes != attributes {
                break;
            }

            if !positions.is_empty() {
                positions.push(' ');
            }
            positions.push_str(
                &(metrics.inset + u32::try_from(column).unwrap_or(u32::MAX) * metrics.cell_width)
                    .to_string(),
            );
            if cell.attributes.hidden {
                content.push(' ');
            } else {
                content.push_str(&escape_char(cell.ch));
            }
            column += 1;
        }
        output.push_str(&format!(
            "<tspan x=\"{positions}\" {}>{content}</tspan>",
            style(attributes)
        ));
    }
}

fn write_row_backgrounds(
    output: &mut String,
    cells: &[super::screen::TerminalCell],
    row: u16,
    metrics: TerminalMetrics,
) {
    let mut column = 0usize;
    while column < cells.len() {
        let cell = &cells[column];
        let (_, effective_bg) = cell.attributes.effective_colors();
        if cell.continuation || (effective_bg == TerminalColor::Default && !cell.attributes.invert)
        {
            column += 1;
            continue;
        }
        let start = column;
        let is_invert_default =
            cell.attributes.invert && cell.attributes.foreground == TerminalColor::Default;
        let mut visual_width = 0u32;
        while column < cells.len() {
            let current = &cells[column];
            let (_, cur_bg) = current.attributes.effective_colors();
            let cur_inv_def = current.attributes.invert
                && current.attributes.foreground == TerminalColor::Default;
            if current.continuation || cur_bg != effective_bg || cur_inv_def != is_invert_default {
                break;
            }
            let cell_width = if cells.get(column + 1).is_some_and(|next| next.continuation) {
                2
            } else {
                1
            };
            visual_width += cell_width;
            column += usize::try_from(cell_width).unwrap_or(1);
        }
        if is_invert_default {
            output.push_str(&format!(
                "<rect class=\"term-invert-bg\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"var(--term-fg)\"/>",
                metrics.inset + u32::try_from(start).unwrap_or(u32::MAX) * metrics.cell_width,
                metrics.inset + u32::from(row) * metrics.row_height,
                visual_width * metrics.cell_width,
                metrics.row_height,
            ));
        } else {
            output.push_str(&format!(
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" {}/>",
                metrics.inset + u32::try_from(start).unwrap_or(u32::MAX) * metrics.cell_width,
                metrics.inset + u32::from(row) * metrics.row_height,
                visual_width * metrics.cell_width,
                metrics.row_height,
                color_style(&effective_bg),
            ));
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ModifierSpec {
    pub is_bold: bool,
    pub is_dim: bool,
    pub is_italic: bool,
    pub is_underline: bool,
    pub is_invert: bool,
    pub is_strikethrough: bool,
    pub is_overline: bool,
    pub is_hidden: bool,
}

impl ModifierSpec {
    pub const fn from_mask(mask: u8) -> Self {
        Self {
            is_bold: (mask & (CellAttributes::MASK_BOLD)) != 0,
            is_dim: (mask & (CellAttributes::MASK_DIM)) != 0,
            is_italic: (mask & (CellAttributes::MASK_ITALIC)) != 0,
            is_underline: (mask & (CellAttributes::MASK_UNDERLINE)) != 0,
            is_invert: (mask & (CellAttributes::MASK_INVERT)) != 0,
            is_strikethrough: (mask & (CellAttributes::MASK_STRIKETHROUGH)) != 0,
            is_overline: (mask & (CellAttributes::MASK_OVERLINE)) != 0,
            is_hidden: (mask & (CellAttributes::MASK_HIDDEN)) != 0,
        }
    }
}

fn write_row_underlines(
    output: &mut String,
    cells: &[super::screen::TerminalCell],
    row: u16,
    metrics: TerminalMetrics,
) {
    let mut column = 0usize;
    while column < cells.len() {
        let cell = &cells[column];
        if cell.continuation || !cell.attributes.underline {
            column += 1;
            continue;
        }
        let start = column;
        let style = cell.attributes.underline_style;
        let color = cell
            .attributes
            .underline_color
            .as_ref()
            .unwrap_or(&cell.attributes.foreground);
        let mut visual_width = 0u32;
        while column < cells.len() {
            let current = &cells[column];
            let current_color = current
                .attributes
                .underline_color
                .as_ref()
                .unwrap_or(&current.attributes.foreground);
            if current.continuation
                || !current.attributes.underline
                || current.attributes.underline_style != style
                || current_color != color
            {
                break;
            }
            let cell_width = if cells.get(column + 1).is_some_and(|next| next.continuation) {
                2
            } else {
                1
            };
            visual_width += cell_width;
            column += usize::try_from(cell_width).unwrap_or(1);
        }

        let color_name = color_class(color);
        let mut extra_fill = String::new();
        match color {
            TerminalColor::Indexed(c) => {
                extra_fill = format!(
                    " fill=\"{}\" stroke=\"{}\"",
                    indexed_color(*c),
                    indexed_color(*c)
                );
            }
            TerminalColor::Rgb(red, green, blue) => {
                extra_fill = format!(
                    " fill=\"rgb({red} {green} {blue})\" stroke=\"rgb({red} {green} {blue})\""
                );
            }
            _ => {}
        }
        let x = metrics.inset + u32::try_from(start).unwrap_or(u32::MAX) * metrics.cell_width;
        let total_w = visual_width * metrics.cell_width;
        let baseline_y = metrics.inset + u32::from(row) * metrics.row_height + metrics.baseline;

        match style {
            super::screen::UnderlineStyle::None => {}
            super::screen::UnderlineStyle::Single => {
                output.push_str(&format!(
                    "<rect class=\"term-underline {color_name}\" x=\"{x}\" y=\"{}\" width=\"{total_w}\" height=\"1\"{extra_fill}/>",
                    baseline_y + 2
                ));
            }
            super::screen::UnderlineStyle::Double => {
                output.push_str(&format!(
                    "<rect class=\"term-underline {color_name}\" x=\"{x}\" y=\"{}\" width=\"{total_w}\" height=\"1\"{extra_fill}/>",
                    baseline_y + 1
                ));
                output.push_str(&format!(
                    "<rect class=\"term-underline {color_name}\" x=\"{x}\" y=\"{}\" width=\"{total_w}\" height=\"1\"{extra_fill}/>",
                    baseline_y + 3
                ));
            }
            super::screen::UnderlineStyle::Curly => {
                let mut path_d = String::from("M");
                let wave_y = baseline_y as f32 + 2.0;
                path_d.push_str(&format!(" {x:.1} {wave_y:.1}"));
                let wave_half = (metrics.cell_width as f32) / 4.0;
                for _ in 0..visual_width {
                    path_d.push_str(&format!(
                        " q {wave_half:.2} -1.5, {:.2} 0 t {:.2} 0",
                        wave_half * 2.0,
                        wave_half * 2.0
                    ));
                }
                output.push_str(&format!(
                    "<path class=\"term-underline-curly {color_name}\" d=\"{path_d}\"{extra_fill}/>"
                ));
            }
            super::screen::UnderlineStyle::Dotted => {
                output.push_str(&format!(
                    "<line class=\"term-underline {color_name}\" x1=\"{x}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke-width=\"1.5\" stroke-dasharray=\"1.5,1.5\"{extra_fill}/>",
                    baseline_y + 2,
                    x + total_w,
                    baseline_y + 2
                ));
            }
            super::screen::UnderlineStyle::Dashed => {
                output.push_str(&format!(
                    "<line class=\"term-underline {color_name}\" x1=\"{x}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke-width=\"1\" stroke-dasharray=\"3,2\"{extra_fill}/>",
                    baseline_y + 2,
                    x + total_w,
                    baseline_y + 2
                ));
            }
        }
    }
}

fn write_row_decorations(
    output: &mut String,
    cells: &[super::screen::TerminalCell],
    row: u16,
    metrics: TerminalMetrics,
) {
    // Strikethrough
    let mut column = 0usize;
    while column < cells.len() {
        let cell = &cells[column];
        if cell.continuation || !cell.attributes.strikethrough {
            column += 1;
            continue;
        }
        let start = column;
        let (foreground, _) = cell.attributes.effective_colors();
        let mut visual_width = 0u32;
        while column < cells.len() {
            let current = &cells[column];
            let (cur_fg, _) = current.attributes.effective_colors();
            if current.continuation || !current.attributes.strikethrough || cur_fg != foreground {
                break;
            }
            let cell_width = if cells.get(column + 1).is_some_and(|next| next.continuation) {
                2
            } else {
                1
            };
            visual_width += cell_width;
            column += usize::try_from(cell_width).unwrap_or(1);
        }
        let color_name = color_class(&foreground);
        let mut extra_fill = String::new();
        match &foreground {
            TerminalColor::Indexed(c) => extra_fill = format!(" fill=\"{}\"", indexed_color(*c)),
            TerminalColor::Rgb(r, g, b) => extra_fill = format!(" fill=\"rgb({r} {g} {b})\""),
            _ => {}
        }
        let x = metrics.inset + u32::try_from(start).unwrap_or(u32::MAX) * metrics.cell_width;
        let y = metrics.inset + u32::from(row) * metrics.row_height + metrics.baseline - 5;
        output.push_str(&format!(
            "<rect class=\"term-strikethrough {color_name}\" x=\"{x}\" y=\"{y}\" width=\"{}\" height=\"1\"{extra_fill}/>",
            visual_width * metrics.cell_width
        ));
    }

    // Overline
    let mut column = 0usize;
    while column < cells.len() {
        let cell = &cells[column];
        if cell.continuation || !cell.attributes.overline {
            column += 1;
            continue;
        }
        let start = column;
        let (foreground, _) = cell.attributes.effective_colors();
        let mut visual_width = 0u32;
        while column < cells.len() {
            let current = &cells[column];
            let (cur_fg, _) = current.attributes.effective_colors();
            if current.continuation || !current.attributes.overline || cur_fg != foreground {
                break;
            }
            let cell_width = if cells.get(column + 1).is_some_and(|next| next.continuation) {
                2
            } else {
                1
            };
            visual_width += cell_width;
            column += usize::try_from(cell_width).unwrap_or(1);
        }
        let color_name = color_class(&foreground);
        let mut extra_fill = String::new();
        match &foreground {
            TerminalColor::Indexed(c) => extra_fill = format!(" fill=\"{}\"", indexed_color(*c)),
            TerminalColor::Rgb(r, g, b) => extra_fill = format!(" fill=\"rgb({r} {g} {b})\""),
            _ => {}
        }
        let x = metrics.inset + u32::try_from(start).unwrap_or(u32::MAX) * metrics.cell_width;
        let y = metrics.inset + u32::from(row) * metrics.row_height + 2;
        output.push_str(&format!(
            "<rect class=\"term-overline {color_name}\" x=\"{x}\" y=\"{y}\" width=\"{}\" height=\"1\"{extra_fill}/>",
            visual_width * metrics.cell_width
        ));
    }
}

fn style(attributes: &CellAttributes) -> String {
    let mask = attributes.modifier_mask();
    let spec = ModifierSpec::from_mask(mask);
    let (effective_fg, _) = attributes.effective_colors();
    let mut classes = String::from("class=\"");
    if attributes.invert && attributes.background == TerminalColor::Default {
        classes.push_str("term-invert-fg");
    } else {
        classes.push_str(&color_class(&effective_fg));
    }
    if spec.is_bold {
        classes.push_str(" term-bold");
    }
    if spec.is_dim {
        classes.push_str(" term-dim");
    }
    if spec.is_italic {
        classes.push_str(" term-italic");
    }
    if spec.is_underline {
        classes.push_str(" term-underline");
    }
    if spec.is_strikethrough {
        classes.push_str(" term-strikethrough");
    }
    if spec.is_overline {
        classes.push_str(" term-overline");
    }
    if spec.is_hidden {
        classes.push_str(" term-hidden");
    }
    classes.push('"');

    let mut style = classes;
    match &effective_fg {
        TerminalColor::Indexed(color) => {
            style.push_str(&format!(" fill=\"{}\"", indexed_color(*color)))
        }
        TerminalColor::Rgb(red, green, blue) => {
            style.push_str(&format!(" fill=\"rgb({red} {green} {blue})\""))
        }
        _ => {}
    }
    if spec.is_bold {
        style.push_str(" font-weight=\"700\"");
    }
    if spec.is_dim {
        style.push_str(" opacity=\"0.7\"");
    }
    if spec.is_italic {
        style.push_str(" font-style=\"italic\"");
    }
    if spec.is_underline {
        style.push_str(" text-decoration=\"underline\"");
    }
    if spec.is_strikethrough {
        style.push_str(" text-decoration=\"line-through\"");
    }
    if spec.is_overline {
        style.push_str(" text-decoration=\"overline\"");
    }
    if spec.is_hidden {
        style.push_str(" opacity=\"0\"");
    }
    style
}

fn color_class(color: &TerminalColor) -> String {
    match color {
        TerminalColor::Default => "term-default".to_string(),
        TerminalColor::Command => "term-command".to_string(),
        TerminalColor::Builtin => "term-builtin".to_string(),
        TerminalColor::Keyword => "term-keyword".to_string(),
        TerminalColor::Flag => "term-flag".to_string(),
        TerminalColor::Variable => "term-variable".to_string(),
        TerminalColor::String => "term-string".to_string(),
        TerminalColor::Number => "term-number".to_string(),
        TerminalColor::Operator => "term-operator".to_string(),
        TerminalColor::Punctuation => "term-punctuation".to_string(),
        TerminalColor::Comment => "term-comment".to_string(),
        TerminalColor::Ansi(color) => format!("term-ansi-{color}"),
        TerminalColor::Indexed(color) => format!("term-indexed-{color}"),
        TerminalColor::Rgb(red, green, blue) => format!("term-rgb-{red}-{green}-{blue}"),
    }
}

fn color_style(color: &TerminalColor) -> String {
    let class = color_class(color);
    let mut style = format!("class=\"{class}\"");
    match color {
        TerminalColor::Indexed(color) => {
            style.push_str(&format!(" fill=\"{}\"", indexed_color(*color)))
        }
        TerminalColor::Rgb(red, green, blue) => {
            style.push_str(&format!(" fill=\"rgb({red} {green} {blue})\""))
        }
        _ => {}
    }
    style
}

fn indexed_color(index: u8) -> String {
    if index < 16 {
        return format!(
            "var(--ui-palette-{})",
            match index {
                0 => "black",
                1 => "red",
                2 => "green",
                3 => "yellow",
                4 => "blue",
                5 => "magenta",
                6 => "cyan",
                _ => "white",
            }
        );
    }
    if index >= 232 {
        let value = 8 + (index - 232) * 10;
        return format!("rgb({value} {value} {value})");
    }
    let color = index - 16;
    let scale = [0, 95, 135, 175, 215, 255];
    let red = scale[usize::from(color / 36)];
    let green = scale[usize::from((color / 6) % 6)];
    let blue = scale[usize::from(color % 6)];
    format!("rgb({red} {green} {blue})")
}

fn escape(value: &str) -> String {
    value.chars().map(escape_char).collect()
}
fn escape_char(value: char) -> String {
    match value {
        '&' => "&amp;".to_string(),
        '<' => "&lt;".to_string(),
        '>' => "&gt;".to_string(),
        '"' => "&quot;".to_string(),
        _ => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tape::VirtualScreen;

    #[test]
    fn emits_accessible_theme_aware_svg() {
        let mut screen = VirtualScreen::new(2, 8);
        screen.write_str("\x1b[31mred");
        let svg = render_svg(
            &TapeConfig::default(),
            &[screen.capture(40)],
            "A command recording",
        );
        assert!(svg.contains("data-tape-duration-ms"));
        assert!(svg.contains("data-tape-cols=\"80\""));
        assert!(svg.contains("--tape-width:"));
        assert!(svg.contains("--tape-cols:80"));
        assert!(svg.contains("--tape-aspect:"));
        assert!(svg.contains("var(--ui-color-code-surface)"));
        assert!(svg.contains("term-ansi-1"));
        assert!(svg.contains(".term-command,.term-builtin{fill:var(--ui-syn-function)}"));
        assert!(!svg.contains("M12 12h"));
    }

    #[test]
    fn preserves_truecolor_and_xterm_palette_output() {
        let mut screen = VirtualScreen::new(1, 8);
        screen.write_str("\x1b[38;2;1;2;3mr\x1b[38;5;196mx");
        let svg = render_svg(
            &TapeConfig::default(),
            &[screen.capture(40)],
            "color output",
        );
        assert!(svg.contains("rgb(1 2 3)"));
        assert!(svg.contains("rgb(255 0 0)"));
    }

    #[test]
    fn renders_ansi_background_cells_behind_terminal_text() {
        let mut screen = VirtualScreen::new(1, 8);
        screen.write_str("\x1b[30;48;5;120mTask\x1b[0m");
        let svg = render_svg(&TapeConfig::default(), &[screen.capture(40)], "title bar");

        assert!(svg.contains("<rect x=\"16\" y=\"16\" width=\"36\" height=\"20\""));
        assert!(svg.contains("fill=\"rgb(135 255 135)\""));
        assert!(svg.contains("term-ansi-0"));
    }

    #[test]
    fn auto_rows_trims_the_svg_to_the_occupied_screen_height() {
        let mut screen = VirtualScreen::new(8, 8);
        screen.write_str("first\nsecond");
        let config = TapeConfig {
            rows: 8,
            cols: 8,
            auto_rows: true,
            ..TapeConfig::default()
        };

        let svg = render_svg(&config, &[screen.capture(40)], "compact recording");

        // Two occupied rows retain the three-row minimum, rather than the
        // eight-row bounded capture screen.
        assert!(svg.contains("viewBox=\"0 0 104 92\""));
    }

    #[test]
    fn metrics_define_discrete_text_grid_and_cursor_geometry() {
        let mut screen = VirtualScreen::new(1, 8);
        screen.write_str("界");
        let config = TapeConfig {
            rows: 1,
            cols: 8,
            ..TapeConfig::default()
        };
        let svg = render_svg(&config, &[screen.capture(40)], "metrics");

        assert!(svg.contains("'IBM Plex Mono','Cascadia Mono','Cascadia Code','Fira Code'"));
        assert!(svg.contains("data-tape-duration-ms=\"1040\""));
        assert!(svg.contains("<text x=\"16\" y=\"31\">"));
        assert!(svg.contains("<tspan x=\"16\""));
        assert!(!svg.contains("textLength="));
        assert!(!svg.contains("lengthAdjust="));
        assert!(svg.contains("y=\"31\""));
        assert!(svg.contains("class=\"term-cursor\" x=\"34\" y=\"16\" width=\"2\" height=\"20\""));
    }

    #[test]
    fn trailing_blank_cells_are_trimmed_while_interior_spaces_preserve_grid() {
        let mut screen = VirtualScreen::new(1, 8);
        screen.write_str("a b");
        let config = TapeConfig {
            rows: 1,
            cols: 8,
            ..TapeConfig::default()
        };
        let svg = render_svg(&config, &[screen.capture(40)], "grid");
        assert!(svg.contains("<tspan x=\"16 25 34\""));
        assert!(!svg.contains("43 52 61 70 79"));
    }

    #[test]
    fn gives_the_final_output_frame_a_visible_hold() {
        let mut screen = VirtualScreen::new(1, 8);
        let initial = screen.capture(0);
        screen.write_str("done");
        let final_frame = screen.capture(200);
        let svg = render_svg(
            &TapeConfig::default(),
            &[initial, final_frame],
            "visible final output",
        );

        assert!(svg.contains("data-tape-duration-ms=\"1200\""));
        assert!(svg.contains("data-tape-frame-start-ms=\"200\" display=\"inline\""));
        assert!(!svg.contains("@keyframes"));
        assert!(!svg.contains("animation:"));
    }

    #[test]
    fn gives_distinct_same_time_frames_separate_cursor_intervals() {
        let mut screen = VirtualScreen::new(1, 8);
        let initial = screen.capture(0);
        screen.write_str("x");
        let typed = screen.capture(0);
        let svg = render_svg(&TapeConfig::default(), &[initial, typed], "no overlap");

        assert!(svg.contains("data-tape-frame-start-ms=\"0\" display=\"none\""));
        assert!(svg.contains("data-tape-frame-start-ms=\"1\" display=\"inline\""));
    }

    #[test]
    fn namespaces_multiple_tape_records() {
        let screen = VirtualScreen::new(1, 8);
        let first = render_svg(&TapeConfig::default(), &[screen.capture(0)], "first tape");
        let second = render_svg(&TapeConfig::default(), &[screen.capture(0)], "second tape");

        assert_ne!(first, second);
        assert!(!first.contains("aria-labelledby=\"tape-title tape-desc\""));
        assert!(!first.contains("visibility:hidden"));
        assert!(!first.contains("@keyframes"));
        assert!(first.contains("data-tape-frame-start-ms=\"0\" display=\"inline\""));
    }

    #[test]
    fn emits_complete_frames_at_shared_coordinates_for_script_selection() {
        let mut screen = VirtualScreen::new(1, 8);
        let initial = screen.capture(0);
        screen.write_str("done");
        let final_frame = screen.capture(200);
        let config = TapeConfig {
            rows: 1,
            cols: 8,
            ..TapeConfig::default()
        };

        let svg = render_svg(&config, &[initial, final_frame], "scripted frame selection");

        assert_eq!(svg.matches("class=\"tape-frame\"").count(), 2);
        assert!(svg.contains("data-tape-frame-start-ms=\"0\" display=\"none\""));
        assert!(svg.contains("data-tape-frame-start-ms=\"200\" display=\"inline\""));
    }

    #[test]
    fn renders_ansi_bold_and_underline_styles_and_underlines() {
        let mut screen = VirtualScreen::new(1, 16);
        screen.write_str("\x1b[1;4;32mbold-under\x1b[0m");
        let svg = render_svg(&TapeConfig::default(), &[screen.capture(40)], "bold-under");

        assert!(svg.contains(".term-bold{font-weight:700}"));
        assert!(svg.contains(".term-underline{text-decoration:underline}"));
        assert!(svg.contains("class=\"term-ansi-2 term-bold term-underline\""));
        assert!(svg.contains("font-weight=\"700\""));
        assert!(svg.contains("text-decoration=\"underline\""));
        // Geometric underline rect has a single valid class attribute
        assert!(svg.contains("<rect class=\"term-underline term-ansi-2\" x=\"16\" y=\"33\" width=\"90\" height=\"1\"/>"));
    }

    #[test]
    fn renders_clap_style_consecutive_bold_and_underline_codes() {
        let mut screen = VirtualScreen::new(1, 16);
        screen.write_str("\x1b[1m\x1b[4mUsage:\x1b[0m");
        let svg = render_svg(&TapeConfig::default(), &[screen.capture(40)], "clap-help");

        assert!(svg.contains("class=\"term-default term-bold term-underline\""));
        assert!(svg.contains("<rect class=\"term-underline term-default\" x=\"16\" y=\"33\" width=\"54\" height=\"1\"/>"));
    }

    #[test]
    fn modifier_spec_covers_all_eight_dimensions() {
        for mask in 0..=255u8 {
            let spec = ModifierSpec::from_mask(mask);
            assert_eq!(spec.is_bold, (mask & CellAttributes::MASK_BOLD) != 0);
            assert_eq!(spec.is_dim, (mask & CellAttributes::MASK_DIM) != 0);
            assert_eq!(spec.is_italic, (mask & CellAttributes::MASK_ITALIC) != 0);
            assert_eq!(
                spec.is_underline,
                (mask & CellAttributes::MASK_UNDERLINE) != 0
            );
            assert_eq!(spec.is_invert, (mask & CellAttributes::MASK_INVERT) != 0);
            assert_eq!(
                spec.is_strikethrough,
                (mask & CellAttributes::MASK_STRIKETHROUGH) != 0
            );
            assert_eq!(
                spec.is_overline,
                (mask & CellAttributes::MASK_OVERLINE) != 0
            );
            assert_eq!(spec.is_hidden, (mask & CellAttributes::MASK_HIDDEN) != 0);
        }
    }

    #[test]
    fn renders_strikethrough_overline_and_invert_decorations() {
        let mut screen = VirtualScreen::new(1, 24);
        screen.write_str("\x1b[9;53;7mINVERT-STRIKE-OVER\x1b[0m");
        let svg = render_svg(&TapeConfig::default(), &[screen.capture(40)], "decorations");

        assert!(svg.contains(".term-strikethrough{text-decoration:line-through}"));
        assert!(svg.contains(".term-overline{text-decoration:overline}"));
        assert!(svg.contains("class=\"term-invert-fg term-strikethrough term-overline\""));
        assert!(svg.contains("<rect class=\"term-invert-bg\" x=\"16\" y=\"16\" width=\"162\" height=\"20\" fill=\"var(--term-fg)\"/>"));
        assert!(svg.contains("<rect class=\"term-strikethrough term-default\""));
        assert!(svg.contains("<rect class=\"term-overline term-default\""));
    }

    #[test]
    fn renders_double_and_curly_underlines_with_custom_color() {
        let mut screen = VirtualScreen::new(1, 16);
        // Curly underline with custom RGB color
        screen.write_str("\x1b[4:3;58:2::255:50:50mWAVY\x1b[0m");
        let svg = render_svg(
            &TapeConfig::default(),
            &[screen.capture(40)],
            "curly-underline",
        );

        assert!(
            svg.contains(".term-underline-curly{fill:none;stroke-width:1.2;stroke-linecap:round}")
        );
        assert!(svg.contains("<path class=\"term-underline-curly term-rgb-255-50-50\""));
        assert!(svg.contains("stroke=\"rgb(255 50 50)\""));

        // Double underline
        let mut screen_double = VirtualScreen::new(1, 16);
        screen_double.write_str("\x1b[21mDOUBLE\x1b[0m");
        let svg_double = render_svg(
            &TapeConfig::default(),
            &[screen_double.capture(40)],
            "double-underline",
        );
        assert!(svg_double.contains("y=\"32\" width=\"54\" height=\"1\""));
        assert!(svg_double.contains("y=\"34\" width=\"54\" height=\"1\""));
    }
}
