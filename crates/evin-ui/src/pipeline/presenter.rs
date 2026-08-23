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

pub use super::ui::build_pipeline_summary_items;

pub fn format_intro_title(target: &str, payload: &[String], is_hook: bool) -> String {
    let mut full_target = target.to_string();
    if !payload.is_empty() {
        full_target.push_str(" -- ");
        full_target.push_str(&payload.join(" "));
    }

    let category = if is_hook {
        "Hook"
    } else if target.chars().next().unwrap_or('a').is_ascii_uppercase() {
        "Stage"
    } else {
        "Task"
    };

    evin_locales::t!("intro-title", category = category, target = &full_target)
}

pub fn resolve_phase_display_name(locale_key: &str) -> String {
    let ftl_key = locale_key.trim_start_matches("ui.phase.").replace('_', "-");
    evin_locales::bundle::get_optional(&format!("phase-{ftl_key}"))
        .unwrap_or_else(|| ftl_key.replace('-', " "))
}
