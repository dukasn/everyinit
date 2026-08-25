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

use std::collections::HashSet;
use std::path::Path;

/// Embedded default icons directory.
/// Default vector icons are provided by Tabler Icons (https://tabler.io/icons, MIT License).
pub static ICONS_DIR: include_dir::Dir = include_dir::include_dir!("$CARGO_MANIFEST_DIR/icons");

pub struct IconResolver;

impl IconResolver {
    pub fn get_icon_svg(
        name: &str,
        custom_icons_dir: Option<&Path>,
        theme_icons_dir: Option<&Path>,
    ) -> Option<String> {
        let filename = format!("{}.svg", name);

        // 1. Workspace custom icons directory: .evin/icons/<name>.svg
        if let Some(dir) = custom_icons_dir {
            let path = dir.join(&filename);
            if path.is_file()
                && let Ok(content) = std::fs::read_to_string(&path)
            {
                return Some(content);
            }
        }

        // 2. Layout theme icons directory
        if let Some(dir) = theme_icons_dir {
            let path = dir.join(&filename);
            if path.is_file()
                && let Ok(content) = std::fs::read_to_string(&path)
            {
                return Some(content);
            }
        }

        // 3. Embedded default icons directory
        if let Some(file) = ICONS_DIR.get_file(&filename)
            && let Some(content) = file.contents_utf8()
        {
            return Some(content.to_string());
        }

        None
    }
}

pub struct IconSpriteBuilder;

impl IconSpriteBuilder {
    /// Extracts `<path>` or inner SVG content from raw SVG string and turns it into a `<symbol>`
    pub fn convert_to_symbol(name: &str, raw_svg: &str) -> String {
        let view_box = if raw_svg.contains("viewBox=\"") {
            raw_svg
                .split("viewBox=\"")
                .nth(1)
                .and_then(|s| s.split('"').next())
                .unwrap_or("0 0 24 24")
        } else {
            "0 0 24 24"
        };

        let fill_attr = if raw_svg.contains("fill=\"") {
            let val = raw_svg
                .split("fill=\"")
                .nth(1)
                .and_then(|s| s.split('"').next())
                .unwrap_or("currentColor");
            format!(" fill=\"{}\"", val)
        } else {
            String::new()
        };

        let stroke_attr = if raw_svg.contains("stroke=\"") {
            let val = raw_svg
                .split("stroke=\"")
                .nth(1)
                .and_then(|s| s.split('"').next())
                .unwrap_or("currentColor");
            format!(" stroke=\"{}\"", val)
        } else {
            String::new()
        };

        let stroke_width_attr = if raw_svg.contains("stroke-width=\"") {
            let val = raw_svg
                .split("stroke-width=\"")
                .nth(1)
                .and_then(|s| s.split('"').next())
                .unwrap_or("2");
            format!(" stroke-width=\"{}\"", val)
        } else {
            String::new()
        };

        let stroke_linecap_attr = if raw_svg.contains("stroke-linecap=\"") {
            let val = raw_svg
                .split("stroke-linecap=\"")
                .nth(1)
                .and_then(|s| s.split('"').next())
                .unwrap_or("round");
            format!(" stroke-linecap=\"{}\"", val)
        } else {
            String::new()
        };

        let stroke_linejoin_attr = if raw_svg.contains("stroke-linejoin=\"") {
            let val = raw_svg
                .split("stroke-linejoin=\"")
                .nth(1)
                .and_then(|s| s.split('"').next())
                .unwrap_or("round");
            format!(" stroke-linejoin=\"{}\"", val)
        } else {
            String::new()
        };

        // Extract the contents of the actual SVG element. Bundled Tabler icons
        // begin with a metadata comment, so searching for the first `>` would
        // otherwise stop at `-->` and nest a complete `<svg>` in the symbol.
        let inner = raw_svg
            .find("<svg")
            .and_then(|svg_start| {
                let opening_end = raw_svg[svg_start..].find('>')? + svg_start;
                let closing_start = raw_svg[opening_end + 1..].rfind("</svg>")? + opening_end + 1;
                Some(&raw_svg[opening_end + 1..closing_start])
            })
            .unwrap_or(raw_svg);

        format!(
            "  <symbol id=\"icon-{}\" viewBox=\"{}\"{}{}{}{}{}>{}\n  </symbol>",
            name,
            view_box,
            fill_attr,
            stroke_attr,
            stroke_width_attr,
            stroke_linecap_attr,
            stroke_linejoin_attr,
            inner.trim()
        )
    }

    /// Assembles an SVG sprite sheet file string containing all requested unique icons
    pub fn build_sprite_sheet(
        icon_names: &HashSet<String>,
        custom_icons_dir: Option<&Path>,
        theme_icons_dir: Option<&Path>,
    ) -> String {
        let mut symbols = String::new();
        symbols.push_str("<svg xmlns=\"http://www.w3.org/2000/svg\" style=\"display:none;\">\n");

        let mut sorted_names: Vec<&String> = icon_names.iter().collect();
        sorted_names.sort();

        for name in sorted_names {
            let svg = IconResolver::get_icon_svg(name, custom_icons_dir, theme_icons_dir)
                .unwrap_or_else(|| {
                    // Fallback to description.svg
                    IconResolver::get_icon_svg("description", custom_icons_dir, theme_icons_dir)
                        .unwrap_or_default()
                });

            if !svg.is_empty() {
                let symbol = Self::convert_to_symbol(name, &svg);
                symbols.push_str(&symbol);
                symbols.push('\n');
            }
        }

        symbols.push_str("</svg>");
        symbols
    }

    /// Generates `<svg class="docs-icon" aria-hidden="true"><use href="...#icon-name"></use></svg>`
    pub fn render_use_tag(name: &str, sprite_href_prefix: &str) -> String {
        let href = if sprite_href_prefix.is_empty() {
            format!("#icon-{}", name)
        } else {
            format!("{}#icon-{}", sprite_href_prefix, name)
        };

        format!(
            "<svg class=\"docs-icon\" aria-hidden=\"true\"><use href=\"{}\"></use></svg>",
            href
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedded_icon_resolver() {
        let svg = IconResolver::get_icon_svg("terminal", None, None);
        assert!(svg.is_some());
        assert!(svg.unwrap().contains("viewBox"));
    }

    #[test]
    fn test_convert_to_symbol() {
        let raw = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\"><path d=\"M0 0h24v24H0z\"/></svg>";
        let symbol = IconSpriteBuilder::convert_to_symbol("test", raw);
        assert!(symbol.contains("id=\"icon-test\""));
        assert!(symbol.contains("viewBox=\"0 0 24 24\""));
        assert!(symbol.contains("<path d=\"M0 0h24v24H0z\"/>"));
    }

    #[test]
    fn test_convert_to_symbol_ignores_leading_metadata_comments() {
        let raw = "<!-- icon metadata -->\n<svg viewBox=\"0 0 24 24\"><path d=\"M1 1\"/></svg>";
        let symbol = IconSpriteBuilder::convert_to_symbol("commented", raw);

        assert!(symbol.contains("<path d=\"M1 1\"/>"));
        assert!(!symbol.contains("<svg viewBox"));
        assert!(!symbol.contains("icon metadata"));
    }
}
