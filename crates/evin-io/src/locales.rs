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

use include_dir::{Dir, DirEntry, include_dir};

// The complete locale tree is embedded so SDK protocol audits inspect the
// same fallback resources that production presentation uses.
static LOCALES_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/../../locales");

/// Represents a loaded Fluent translation resource file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FtlResource {
    pub path: String,
    pub content: String,
}

impl FtlResource {
    pub fn new(path: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            content: content.into(),
        }
    }
}

/// Helper function to recursively collect `.ftl` files from an `include_dir::Dir`.
fn collect_ftl_from_dir(dir: &Dir, results: &mut Vec<FtlResource>) {
    for entry in dir.entries() {
        match entry {
            DirEntry::File(file) => {
                let path = file.path();
                if path.extension().and_then(|ext| ext.to_str()) == Some("ftl")
                    && let Some(content) = file.contents_utf8()
                {
                    results.push(FtlResource::new(path.display().to_string(), content));
                }
            }
            DirEntry::Dir(sub_dir) => {
                collect_ftl_from_dir(sub_dir, results);
            }
        }
    }
}

/// Automatically loads all `.ftl` files for a given locale tag (`lang`), as well as base (`en`) fallbacks.
///
/// Looks inside `locales/{lang}/**/*` recursively.
/// If `lang` is not `"en"`, `en` resources are loaded first, followed by target `lang` resources.
pub fn load_ftl_resources(lang: &str) -> Vec<FtlResource> {
    let mut resources = Vec::new();

    // 1. Base fallback language: 'en'
    if let Some(en_dir) = LOCALES_DIR.get_dir("en") {
        collect_ftl_from_dir(en_dir, &mut resources);
    }

    // 2. Target language if different from 'en'
    let norm_lang = lang.trim();
    if !norm_lang.is_empty() && norm_lang != "en" {
        if let Some(target_dir) = LOCALES_DIR.get_dir(norm_lang) {
            collect_ftl_from_dir(target_dir, &mut resources);
        } else if let Some(base_lang) = norm_lang.split('-').next()
            && base_lang != "en"
            && base_lang != norm_lang
            && let Some(target_dir) = LOCALES_DIR.get_dir(base_lang)
        {
            collect_ftl_from_dir(target_dir, &mut resources);
        }
    }

    resources
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_en_resources() {
        let res = load_ftl_resources("en");
        assert!(!res.is_empty(), "Should load English locale files");
        assert!(
            res.iter().any(|r| r.path.contains("cli.ftl")),
            "Should include cli.ftl"
        );
        assert!(
            res.iter().any(|r| r.path.contains("plugins")),
            "Should include nested plugin ftl files"
        );
    }

    #[test]
    fn test_fallback_language() {
        let res = load_ftl_resources("non-existent-lang");
        // Should safely fall back to base 'en' resources
        assert!(!res.is_empty());
        assert!(res.iter().any(|r| r.path.contains("cli.ftl")));
    }
}
