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

use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UiDictionary {
    pub search_placeholder: String,
    pub on_this_page: String,
    pub previous: String,
    pub next: String,
    pub untranslated_notice: String,
    pub version_banner: String,
}

impl Default for UiDictionary {
    fn default() -> Self {
        Self {
            search_placeholder: "Search documentation".to_string(),
            on_this_page: "On this page".to_string(),
            previous: "Previous".to_string(),
            next: "Next".to_string(),
            untranslated_notice: "Untranslated — showing {locale}".to_string(),
            version_banner: "Viewing {current} documentation — switch to {latest} (latest)"
                .to_string(),
        }
    }
}

impl UiDictionary {
    pub fn load(
        workspace_root: &Path,
        locale: &str,
        directory: Option<&Path>,
    ) -> Result<Self, String> {
        let path = directory
            .map_or_else(
                || workspace_root.join(".evin/docs/i18n"),
                |path| {
                    if path.is_absolute() {
                        path.to_path_buf()
                    } else {
                        workspace_root.join(path)
                    }
                },
            )
            .join(format!("{locale}.flow"));
        if !path.is_file() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(&path).map_err(|error| {
            format!("failed to read UI dictionary '{}': {error}", path.display())
        })?;
        serde_yaml::from_str(&text)
            .map_err(|error| format!("invalid UI dictionary '{}': {error}", path.display()))
    }

    pub fn version_banner(&self, current: &str, latest: &str) -> String {
        self.version_banner
            .replace("{current}", current)
            .replace("{latest}", latest)
    }

    pub fn untranslated_notice(&self, locale: &str) -> String {
        let label = if locale.eq_ignore_ascii_case("en") {
            "English"
        } else {
            locale
        };
        self.untranslated_notice.replace("{locale}", label)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dictionaries_fill_omitted_labels_and_reject_unknown_fields() {
        let directory = tempfile::tempdir().unwrap();
        let dictionaries = directory.path().join("i18n");
        std::fs::create_dir_all(&dictionaries).unwrap();
        std::fs::write(
            dictionaries.join("pt-BR.flow"),
            "search_placeholder: Buscar\nnext: Próxima\n",
        )
        .unwrap();
        let dictionary =
            UiDictionary::load(directory.path(), "pt-BR", Some(&dictionaries)).unwrap();
        assert_eq!(dictionary.search_placeholder, "Buscar");
        assert_eq!(dictionary.next, "Próxima");
        assert_eq!(dictionary.previous, "Previous");
        assert_eq!(
            dictionary.version_banner("canary", "v0.2"),
            "Viewing canary documentation — switch to v0.2 (latest)"
        );

        std::fs::write(
            dictionaries.join("en.flow"),
            "search_placeholder: Search\nunknown: value\n",
        )
        .unwrap();
        assert!(UiDictionary::load(directory.path(), "en", Some(&dictionaries)).is_err());
    }
}
