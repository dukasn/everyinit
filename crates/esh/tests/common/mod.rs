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

use std::path::Path;

#[allow(dead_code)]
pub fn should_compare_against_bash() -> bool {
    if cfg!(windows) {
        return false;
    }
    if !cfg!(target_os = "macos") {
        return true;
    }

    let Ok(version) = std::process::Command::new("bash").arg("--version").output() else {
        return false;
    };
    !String::from_utf8_lossy(&version.stdout).contains("version 3.2.")
}

#[allow(dead_code)]
pub fn presented_path(path: &Path) -> String {
    let value = path.to_string_lossy();
    #[cfg(windows)]
    {
        value
            .strip_prefix(r"\\?\")
            .unwrap_or(&value)
            .replace('\\', "/")
    }
    #[cfg(not(windows))]
    {
        value.into_owned()
    }
}
