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

pub struct CssOptimizer;

impl CssOptimizer {
    pub fn process(raw_css: &str, fs: &dyn grass::Fs, minify: bool) -> Result<String, String> {
        Self::process_with_components(raw_css, "", fs, minify)
    }

    pub fn process_with_components(
        raw_css: &str,
        components_scss: &str,
        fs: &dyn grass::Fs,
        minify: bool,
    ) -> Result<String, String> {
        let style = if minify {
            grass::OutputStyle::Compressed
        } else {
            grass::OutputStyle::Expanded
        };
        let options = grass::Options::default().style(style).fs(fs);
        let mut composite = raw_css.to_string();
        if !components_scss.trim().is_empty() {
            composite.push_str("\n@layer components {\n");
            composite.push_str(components_scss);
            composite.push_str("\n}\n");
        }
        grass::from_string(composite, &options)
            .map(|css| css.trim_start_matches('\u{feff}').to_string())
            .map_err(|error| format!("Failed to compile documentation styles: {error}"))
    }
}
