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

#![allow(clippy::result_large_err)]
use evin_action_sdk::packaging::{PackageContext, PreparedEntryKind};
use std::collections::BTreeMap;

pub(super) struct ArchiveInputReport {
    pub(super) path: String,
}

#[derive(Default)]
pub(super) struct ArchivePathNode {
    pub(super) terminal: bool,
    pub(super) children: BTreeMap<String, ArchivePathNode>,
}

pub(super) struct ArchiveReport {
    pub(super) inputs: Vec<ArchiveInputReport>,
    pub(super) input_bytes: u64,
}

pub(super) fn archive_input_report(context: &PackageContext) -> ArchiveReport {
    let inputs = context
        .entries
        .iter()
        .filter(|entry| entry.kind == PreparedEntryKind::File)
        .map(|entry| ArchiveInputReport {
            path: entry.path.clone(),
        })
        .collect::<Vec<_>>();
    let input_bytes = context
        .entries
        .iter()
        .filter(|entry| entry.kind == PreparedEntryKind::File)
        .map(|entry| entry.data.len() as u64)
        .sum();
    ArchiveReport {
        inputs,
        input_bytes,
    }
}

pub(super) fn compact_archive_paths(inputs: &[ArchiveInputReport]) -> String {
    let mut root = ArchivePathNode::default();
    for input in inputs {
        let mut node = &mut root;
        for component in input.path.split('/') {
            node = node.children.entry(component.to_string()).or_default();
        }
        node.terminal = true;
    }
    render_archive_paths(&root)
}

fn render_archive_paths(node: &ArchivePathNode) -> String {
    if node.children.is_empty() {
        return String::new();
    }

    let extension = node
        .children
        .iter()
        .all(|(_, child)| child.terminal && child.children.is_empty())
        .then(|| {
            node.children
                .keys()
                .next()
                .and_then(|name| name.rsplit_once('.'))
                .and_then(|(stem, extension)| {
                    (!stem.is_empty() && !extension.is_empty()).then_some(extension)
                })
        })
        .flatten()
        .filter(|extension| {
            node.children.keys().all(|name| {
                name.rsplit_once('.')
                    .is_some_and(|(stem, candidate)| !stem.is_empty() && candidate == *extension)
            })
        });

    if node.children.len() > 1
        && let Some(extension) = extension
    {
        return format!("*.{extension}({})", node.children.len());
    }

    let mut branches = node
        .children
        .iter()
        .map(|(component, child)| {
            let component = escape_archive_path_component(component);
            let suffix = render_archive_paths(child);
            if suffix.is_empty() {
                component
            } else {
                format!("{component}/{suffix}")
            }
        })
        .collect::<Vec<_>>();
    if node.terminal {
        branches.insert(0, String::new());
    }
    if branches.len() == 1 {
        branches.pop().unwrap_or_default()
    } else {
        format!("{{{}}}", branches.join(","))
    }
}

fn escape_archive_path_component(component: &str) -> String {
    component
        .chars()
        .map(|character| {
            if "\\,{}*?[]".contains(character) {
                format!("\\{character}")
            } else {
                character.to_string()
            }
        })
        .collect()
}

pub(super) fn format_byte_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}
