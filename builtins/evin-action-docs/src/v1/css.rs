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

use std::collections::HashMap;

pub struct CssOptimizer;

impl CssOptimizer {
    /// Full SCSS pipeline: compile via grass with custom Fs → inject overrides → minify.
    ///
    /// The custom `grass::Fs` enables `@use` / `@import` resolution from embedded
    /// assets (`include_dir`) with 3-tier fallback: custom disk → theme → default.
    ///
    /// - `raw_css`: Theme SCSS entry point content (typically a `@use` manifest)
    /// - `overrides`: CSS custom property overrides to append as the winning `:root` block
    /// - `fs`: Custom filesystem for `@use`/`@import` resolution
    /// - `minify`: `true` for production (compressed output), `false` for dev (expanded)
    pub fn process(
        raw_css: &str,
        overrides: &HashMap<String, String>,
        fs: &dyn grass::Fs,
        minify: bool,
    ) -> Result<String, String> {
        Self::process_with_components(raw_css, "", overrides, fs, minify)
    }

    pub fn process_with_components(
        raw_css: &str,
        components_scss: &str,
        overrides: &HashMap<String, String>,
        fs: &dyn grass::Fs,
        minify: bool,
    ) -> Result<String, String> {
        let style = if minify {
            grass::OutputStyle::Compressed
        } else {
            grass::OutputStyle::Expanded
        };

        let options = grass::Options::default().style(style).fs(fs);

        let mut composite_scss = raw_css.to_string();
        if !components_scss.trim().is_empty() {
            composite_scss.push_str("\n@layer components {\n");
            composite_scss.push_str(components_scss);
            composite_scss.push_str("\n}\n");
        }

        // Stage 1: Compile SCSS through grass (resolves @use via our EmbeddedFs)
        let mut processed = grass::from_string(composite_scss, &options)
            .map_err(|e| format!("Failed to compile theme styles: {}", e))?;

        // Stage 2: Append theme overrides so they win in @layer overrides
        if !overrides.is_empty() {
            processed.push('\n');
            let block = Self::override_block(overrides, minify);
            if minify {
                processed.push_str("@layer overrides{");
                processed.push_str(&block);
                processed.push('}');
            } else {
                processed.push_str("@layer overrides {\n");
                processed.push_str(&block);
                processed.push_str("}\n");
            }
        }

        Ok(processed)
    }

    fn override_block(overrides: &HashMap<String, String>, minify: bool) -> String {
        let mut declarations: Vec<(String, &str)> = overrides
            .iter()
            .map(|(key, value)| (Self::normalize_override_name(key), value.as_str()))
            .collect();
        declarations.sort_by(|left, right| left.0.cmp(&right.0));

        if minify {
            let body = declarations
                .iter()
                .map(|(name, value)| format!("{name}:{value}"))
                .collect::<Vec<_>>()
                .join(";");
            format!(":root{{{body}}}")
        } else {
            let body = declarations
                .iter()
                .map(|(name, value)| format!("  {name}: {value};"))
                .collect::<Vec<_>>()
                .join("\n");
            format!(":root {{\n{body}\n}}\n")
        }
    }

    fn normalize_override_name(key: &str) -> String {
        let normalized = key.trim_start_matches("--").replace('_', "-");
        if let Some(step) = normalized.strip_prefix("ui-space-") {
            return format!("--ui-ref-space-{step}");
        }
        let semantic = match normalized.as_str() {
            "primary" | "ui-primary" => "ui-color-primary",
            "on-primary" | "ui-on-primary" => "ui-color-on-primary",
            "surface" | "ui-surface" => "ui-color-surface",
            "ui-bg" => "ui-color-background",
            "ui-on-bg" => "ui-color-on-background",
            "ui-surface-variant" => "ui-color-surface-variant",
            "ui-surface-container-lowest" => "ui-color-surface-container-lowest",
            "ui-surface-container-low" => "ui-color-surface-container-low",
            "ui-surface-container" => "ui-color-surface-container",
            "ui-surface-container-high" => "ui-color-surface-container-high",
            "ui-surface-container-highest" => "ui-color-surface-container-highest",
            "ui-on-surface" => "ui-color-on-surface",
            "ui-on-surface-variant" => "ui-color-on-surface-variant",
            "ui-outline" => "ui-color-outline",
            "ui-outline-variant" => "ui-color-outline-variant",
            "ui-hover" => "ui-color-state-hover",
            "ui-code-bg" => "ui-color-code-surface",
            "ui-code-text" => "ui-color-code-on-surface",
            "ui-link" => "ui-color-link",
            "ui-link-hover" => "ui-color-link-hover",
            "ui-banner-bg" => "ui-color-banner-surface",
            "ui-banner-text" => "ui-color-banner-on-surface",
            "ui-banner-badge-bg" => "ui-color-banner-badge",
            "ui-banner-badge-text" => "ui-color-banner-on-badge",
            "ui-banner-border" => "ui-color-banner-outline",
            "ui-radius-xs" => "ui-shape-corner-extra-small",
            "ui-radius-sm" => "ui-shape-corner-small",
            "ui-radius-md" => "ui-shape-corner-medium",
            "ui-radius-lg" => "ui-shape-corner-large",
            "ui-radius-xl" => "ui-shape-corner-extra-large",
            "ui-radius-full" => "ui-shape-corner-full",
            "font-sans" => "ui-typescale-font-sans",
            "font-mono" => "ui-typescale-font-monospace",
            "ease" => "ui-motion-easing-standard",
            "duration" => "ui-motion-duration-medium",
            "sidebar-width" => "ui-layout-sidebar-width",
            "content-max-width" => "ui-layout-content-max-width",
            "header-h" => "ui-layout-header-height",
            "spacing-paragraph-bottom" => "ui-doc-spacing-paragraph",
            "spacing-heading-top" => "ui-doc-spacing-heading-top",
            "spacing-heading-bottom" => "ui-doc-spacing-heading-bottom",
            "spacing-section-gap" => "ui-doc-spacing-section",
            "spacing-card-padding" => "ui-doc-spacing-card",
            "spacing-inline-gap" => "ui-doc-spacing-inline",
            _ => return format!("--{normalized}"),
        };
        format!("--{semantic}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_overrides_are_sorted_and_emitted_as_the_final_root_block() {
        let overrides = HashMap::from([
            ("ui_color_primary".to_string(), "#111111".to_string()),
            ("ui_layout_sidebar_width".to_string(), "18rem".to_string()),
        ]);
        let block = CssOptimizer::override_block(&overrides, false);

        assert_eq!(
            block,
            ":root {\n  --ui-color-primary: #111111;\n  --ui-layout-sidebar-width: 18rem;\n}\n"
        );
    }

    #[test]
    fn shorthand_override_names_target_the_semantic_contract() {
        let overrides = HashMap::from([
            ("--ui-bg".to_string(), "#222222".to_string()),
            ("ui_primary".to_string(), "#eeeeee".to_string()),
            ("font_sans".to_string(), "Inter, sans-serif".to_string()),
            (
                "font-mono".to_string(),
                "JetBrains Mono, monospace".to_string(),
            ),
            ("ui_radius_lg".to_string(), "2rem".to_string()),
            ("ui_space_6".to_string(), "1.75rem".to_string()),
            ("sidebar_width".to_string(), "20rem".to_string()),
        ]);
        let block = CssOptimizer::override_block(&overrides, true);

        assert_eq!(
            block,
            ":root{--ui-color-background:#222222;--ui-color-primary:#eeeeee;--ui-layout-sidebar-width:20rem;--ui-ref-space-6:1.75rem;--ui-shape-corner-large:2rem;--ui-typescale-font-monospace:JetBrains Mono, monospace;--ui-typescale-font-sans:Inter, sans-serif}"
        );
    }

    #[test]
    fn published_and_font_aliases_target_the_semantic_contract() {
        assert_eq!(
            CssOptimizer::normalize_override_name("primary"),
            "--ui-color-primary"
        );
        assert_eq!(
            CssOptimizer::normalize_override_name("on-primary"),
            "--ui-color-on-primary"
        );
        assert_eq!(
            CssOptimizer::normalize_override_name("surface"),
            "--ui-color-surface"
        );
        assert_eq!(
            CssOptimizer::normalize_override_name("font_sans"),
            "--ui-typescale-font-sans"
        );
        assert_eq!(
            CssOptimizer::normalize_override_name("font-mono"),
            "--ui-typescale-font-monospace"
        );
    }
}
