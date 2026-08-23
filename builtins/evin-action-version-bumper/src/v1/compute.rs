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

use semver::{Prerelease, Version};

use evin_gitrules::models::Impact;

pub fn compute_version(
    current: &str,
    impacts: &[Impact],
    stable: bool,
    prerelease: Option<&str>,
    channel_template: Option<&str>,
    start_at_one: bool,
) -> Option<String> {
    let max_impact = impacts.iter().max().copied().unwrap_or(Impact::None);

    if max_impact == Impact::None {
        return None;
    }

    let mut v = Version::parse(current).unwrap();

    if stable {
        v.pre = Prerelease::EMPTY;
        return Some(v.to_string());
    }

    let start_num = if start_at_one { 1 } else { 0 };

    if let Some(template) = channel_template {
        // Find the `{n}` token
        let parts: Vec<&str> = template.split("{n}").collect();
        let prefix = parts.first().unwrap_or(&template);
        let prefix_clean = prefix.trim_end_matches('.');

        let pre_str = v.pre.as_str();
        if !pre_str.is_empty() {
            if pre_str.starts_with(prefix_clean) {
                // Same channel transition
                if let Some(pos) = pre_str.rfind('.') {
                    let (_, num_str) = pre_str.split_at(pos);
                    if let Ok(num) = num_str[1..].parse::<u64>() {
                        let new_pre = format!("{}.{}", prefix_clean, num + 1);
                        v.pre = Prerelease::new(&new_pre).unwrap_or(Prerelease::EMPTY);
                        return Some(v.to_string());
                    }
                }
                // Fallback if no numeric ending is found but prefix matches
                let new_pre = format!("{}.{}", prefix_clean, start_num);
                v.pre = Prerelease::new(&new_pre).unwrap_or(Prerelease::EMPTY);
                return Some(v.to_string());
            } else {
                // Cross channel transition from existing prerelease: do NOT bump the base version again
                let new_pre = format!("{}.{}", prefix_clean, start_num);
                v.pre = Prerelease::new(&new_pre).unwrap_or(Prerelease::EMPTY);
                return Some(v.to_string());
            }
        }

        // Branching from stable: bump base and reset `{n}`
        match max_impact {
            Impact::Major => {
                v.major += 1;
                v.minor = 0;
                v.patch = 0;
            }
            Impact::Minor => {
                v.minor += 1;
                v.patch = 0;
            }
            Impact::Patch => {
                v.patch += 1;
            }
            Impact::None => return None,
        }
        let new_pre = format!("{}.{}", prefix_clean, start_num);
        v.pre = Prerelease::new(&new_pre).unwrap_or(Prerelease::EMPTY);
        return Some(v.to_string());
    }

    if !v.pre.is_empty() {
        let pre_str = v.pre.as_str();
        if let Some(pos) = pre_str.rfind('.') {
            let (prefix, num_str) = pre_str.split_at(pos);
            if let Ok(num) = num_str[1..].parse::<u64>() {
                let new_pre = format!("{}.{}", prefix, num + 1);
                v.pre = Prerelease::new(&new_pre).unwrap_or(Prerelease::EMPTY);
                return Some(v.to_string());
            }
        }
        let new_pre = format!("{}.1", pre_str);
        v.pre = Prerelease::new(&new_pre).unwrap_or(Prerelease::EMPTY);
        return Some(v.to_string());
    }

    match max_impact {
        Impact::Major => {
            v.major += 1;
            v.minor = 0;
            v.patch = 0;
        }
        Impact::Minor => {
            v.minor += 1;
            v.patch = 0;
        }
        Impact::Patch => {
            v.patch += 1;
        }
        Impact::None => return None,
    }

    if let Some(prefix) = prerelease {
        let new_pre = format!("{}.1", prefix);
        v.pre = Prerelease::new(&new_pre).unwrap_or(Prerelease::EMPTY);
    }

    Some(v.to_string())
}
