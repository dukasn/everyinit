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

use evin_catalog::{CatalogEntry, CatalogExport};
use evin_config::semantic::CatalogTarget;
use std::sync::LazyLock;

pub(super) static CONFIG_CATALOG: LazyLock<CatalogExport> = LazyLock::new(|| {
    CatalogExport::from_json(include_str!("../../assets/config-catalog.json"))
        .expect("embedded Evin configuration catalog must be valid")
});

pub(super) fn catalog_entry(target: Option<&CatalogTarget>) -> Option<&'static CatalogEntry> {
    target.and_then(|target| CONFIG_CATALOG.symbols.get(target.as_str()))
}

pub(super) fn catalog_augmented_markdown(local: &str, target: Option<&CatalogTarget>) -> String {
    let Some(target) = target else {
        return local.into();
    };
    let mut candidate = target.as_str();
    loop {
        if let Some(entry) = CONFIG_CATALOG.symbols.get(candidate) {
            return format!("{local}\n\n{}", entry.hover_markdown);
        }
        let Some((parent, _)) = candidate.rsplit_once('.') else {
            return local.into();
        };
        candidate = parent;
    }
}
