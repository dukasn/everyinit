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

/// Presents a native path at the ESH language boundary.
///
/// ESH scripts always observe `/` separators. On Unix, backslashes remain
/// untouched because they are valid filename characters rather than path
/// separators.
pub(crate) fn present(path: &Path) -> String {
    present_lossy(&path.to_string_lossy())
}

#[cfg(not(windows))]
fn present_lossy(path: &str) -> String {
    path.to_string()
}

#[cfg(windows)]
fn present_lossy(path: &str) -> String {
    if let Some(path) = path.strip_prefix(r"\\?\UNC\") {
        return format!("//{}", path.replace('\\', "/"));
    }

    path.strip_prefix(r"\\?\")
        .unwrap_or(path)
        .replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::present;
    use std::path::Path;

    #[cfg(not(windows))]
    #[test]
    fn preserves_unix_backslashes() {
        assert_eq!(present(Path::new(r"dir/name\part")), r"dir/name\part");
    }

    #[cfg(windows)]
    #[test]
    fn presents_drive_and_verbatim_paths() {
        assert_eq!(present(Path::new(r"C:\work\project")), "C:/work/project");
        assert_eq!(
            present(Path::new(r"\\?\C:\work\project")),
            "C:/work/project"
        );
        assert_eq!(
            present(Path::new(r"\\?\UNC\server\share\project")),
            "//server/share/project"
        );
    }
}
