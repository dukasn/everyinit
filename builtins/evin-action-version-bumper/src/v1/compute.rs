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

use evin_gitrules::models::{Impact, VersioningStrategy};

fn bump_base(v: &mut Version, impact: Impact, strategy: VersioningStrategy) {
    match (
        strategy == VersioningStrategy::Cargo && v.major == 0,
        impact,
    ) {
        (true, Impact::Major) | (false, Impact::Minor) => {
            v.minor += 1;
            v.patch = 0;
        }
        (false, Impact::Major) => {
            v.major += 1;
            v.minor = 0;
            v.patch = 0;
        }
        (_, Impact::Patch) | (true, Impact::Minor) => v.patch += 1,
        (_, Impact::None) => {}
    }
}

pub fn compute_version(
    current: &str,
    impacts: &[Impact],
    stable: bool,
    prerelease: Option<&str>,
    channel_template: Option<&str>,
    start_at_one: bool,
    strategy: VersioningStrategy,
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
        bump_base(&mut v, max_impact, strategy);
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

    bump_base(&mut v, max_impact, strategy);

    if let Some(prefix) = prerelease {
        let new_pre = format!("{}.1", prefix);
        v.pre = Prerelease::new(&new_pre).unwrap_or(Prerelease::EMPTY);
    }

    Some(v.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn impacts_follow_pre_one_and_stable_version_rules() {
        for (current, impact, expected) in [
            ("0.1.0", Impact::Patch, "0.1.1"),
            ("0.1.0", Impact::Minor, "0.1.1"),
            ("0.1.2", Impact::Major, "0.2.0"),
            ("0.0.1", Impact::Major, "0.1.0"),
            ("1.2.3", Impact::Patch, "1.2.4"),
            ("1.2.3", Impact::Minor, "1.3.0"),
            ("1.2.3", Impact::Major, "2.0.0"),
        ] {
            assert_eq!(
                compute_version(
                    current,
                    &[impact],
                    false,
                    None,
                    None,
                    false,
                    VersioningStrategy::Cargo
                )
                .as_deref(),
                Some(expected)
            );
        }
        assert_eq!(
            compute_version(
                "0.1.0",
                &[Impact::Patch, Impact::Major],
                false,
                None,
                None,
                false,
                VersioningStrategy::Cargo
            )
            .as_deref(),
            Some("0.2.0")
        );
        assert_eq!(
            compute_version(
                "0.1.0",
                &[],
                false,
                None,
                None,
                false,
                VersioningStrategy::Cargo
            ),
            None
        );
    }

    #[test]
    fn prerelease_modes_use_pre_one_base_and_preserve_existing_prereleases() {
        for (current, impact, stable, prerelease, channel, expected) in [
            (
                "0.1.0",
                Impact::Minor,
                false,
                Some("rc"),
                None,
                "0.1.1-rc.1",
            ),
            (
                "0.1.3",
                Impact::Major,
                false,
                None,
                Some("rc.{n}"),
                "0.2.0-rc.0",
            ),
            (
                "1.2.3",
                Impact::Major,
                false,
                None,
                Some("rc.{n}"),
                "2.0.0-rc.0",
            ),
            ("0.2.0-rc.1", Impact::Major, false, None, None, "0.2.0-rc.2"),
            (
                "0.2.0-rc.1",
                Impact::Major,
                false,
                None,
                Some("beta.{n}"),
                "0.2.0-beta.0",
            ),
            ("0.2.0-rc.1", Impact::Minor, true, None, None, "0.2.0"),
        ] {
            assert_eq!(
                compute_version(
                    current,
                    &[impact],
                    stable,
                    prerelease,
                    channel,
                    false,
                    VersioningStrategy::Cargo
                )
                .as_deref(),
                Some(expected)
            );
        }
    }

    #[test]
    fn generic_mode_keeps_existing_semver_behavior() {
        for (impact, expected) in [
            (Impact::Patch, "0.1.1"),
            (Impact::Minor, "0.2.0"),
            (Impact::Major, "1.0.0"),
        ] {
            assert_eq!(
                compute_version(
                    "0.1.0",
                    &[impact],
                    false,
                    None,
                    None,
                    false,
                    VersioningStrategy::Semver
                )
                .as_deref(),
                Some(expected)
            );
        }
        assert_eq!(
            compute_version(
                "0.1.0",
                &[Impact::Minor],
                false,
                Some("rc"),
                None,
                false,
                VersioningStrategy::Semver
            )
            .as_deref(),
            Some("0.2.0-rc.1")
        );
        assert_eq!(
            compute_version(
                "0.1.0",
                &[Impact::Major],
                false,
                None,
                Some("rc.{n}"),
                false,
                VersioningStrategy::Semver
            )
            .as_deref(),
            Some("1.0.0-rc.0")
        );
    }
}
