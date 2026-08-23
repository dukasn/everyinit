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

use crate::v1::resolver::ResolvedFavicon;
use std::collections::HashMap;
use std::path::Path;

pub struct FaviconPipeline;

impl FaviconPipeline {
    pub const DEFAULT_FAVICON_SVG: &'static str = r##"<svg width="32" height="32" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
<path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
<path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
</svg>"##;

    /// Generates favicon assets (favicon.<ext>, site.webmanifest) and returns (rel_path -> content, html_head_tags)
    pub fn process(
        resolved_favicon: Option<&ResolvedFavicon>,
        site_title: &str,
        theme_color: Option<&str>,
        background_color: Option<&str>,
        assets_rel_prefix: &str,
    ) -> (HashMap<String, Vec<u8>>, String) {
        let mut outputs = HashMap::new();

        let (favicon_bytes, ext, mime_type) = match resolved_favicon {
            Some(ResolvedFavicon::Custom {
                bytes,
                extension,
                mime_type,
            }) => (bytes.clone(), extension.as_str(), mime_type.as_str()),
            _ => (
                Self::DEFAULT_FAVICON_SVG.as_bytes().to_vec(),
                "svg",
                "image/svg+xml",
            ),
        };

        // 1. Output favicon asset with correct extension
        let favicon_filename = format!("favicon.{}", ext);
        let favicon_rel_path = format!("assets/favicons/{}", favicon_filename);
        outputs.insert(favicon_rel_path.clone(), favicon_bytes);

        // 2. Output site.webmanifest
        let t_color = theme_color.unwrap_or("#1f1f1f");
        let bg_color = background_color.unwrap_or("#ffffff");

        let manifest_json = format!(
            r#"{{
  "name": "{}",
  "short_name": "{}",
  "icons": [
    {{
      "src": "{}{}",
      "sizes": "any",
      "type": "{}"
    }}
  ],
  "theme_color": "{}",
  "background_color": "{}",
  "display": "standalone"
}}"#,
            site_title,
            site_title,
            assets_rel_prefix,
            favicon_rel_path,
            mime_type,
            t_color,
            bg_color
        );

        outputs.insert(
            "assets/favicons/site.webmanifest".to_string(),
            manifest_json.as_bytes().to_vec(),
        );

        // 3. HTML head tags
        let head_html = format!(
            r#"<link rel="icon" type="{}" href="{}{}">
    <link rel="manifest" href="{}assets/favicons/site.webmanifest">
    <meta name="theme-color" content="{}">"#,
            mime_type, assets_rel_prefix, favicon_rel_path, assets_rel_prefix, t_color
        );

        (outputs, head_html)
    }

    pub fn process_from_path(
        source_path: Option<&str>,
        site_title: &str,
        theme_color: Option<&str>,
        background_color: Option<&str>,
        assets_rel_prefix: &str,
    ) -> (HashMap<String, Vec<u8>>, String) {
        let resolved = source_path.and_then(|path_str| {
            let p = Path::new(path_str);
            if p.is_file() {
                let extension = p
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("svg")
                    .to_lowercase();
                let mime_type = crate::v1::resolver::mime_for_extension(&extension).to_string();
                let bytes = std::fs::read(p).ok()?;
                Some(ResolvedFavicon::Custom {
                    bytes,
                    extension,
                    mime_type,
                })
            } else {
                None
            }
        });
        Self::process(
            resolved.as_ref(),
            site_title,
            theme_color,
            background_color,
            assets_rel_prefix,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_favicon_pipeline_default() {
        let (assets, head_html) =
            FaviconPipeline::process(None, "Test Site", Some("#6750A4"), Some("#FFFFFF"), "./");

        assert!(assets.contains_key("assets/favicons/favicon.svg"));
        assert!(assets.contains_key("assets/favicons/site.webmanifest"));
        assert!(head_html.contains("favicon.svg"));
        assert!(head_html.contains("image/svg+xml"));
        assert!(head_html.contains("#6750A4"));
    }

    #[test]
    fn test_favicon_pipeline_custom_png() {
        // Create a temporary png file for testing
        let temp_dir = std::env::temp_dir();
        let png_path = temp_dir.join("test_favicon.png");
        let fake_png_bytes = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        let _ = std::fs::write(&png_path, &fake_png_bytes);

        let (assets, head_html) =
            FaviconPipeline::process_from_path(png_path.to_str(), "PNG Site", None, None, "./");

        assert!(assets.contains_key("assets/favicons/favicon.png"));
        assert_eq!(
            assets.get("assets/favicons/favicon.png"),
            Some(&fake_png_bytes)
        );
        assert!(head_html.contains("favicon.png"));
        assert!(head_html.contains("image/png"));

        let _ = std::fs::remove_file(png_path);
    }
}
