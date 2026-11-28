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

use std::path::{Component, Path, PathBuf};

pub fn clean_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => out.push(Component::Prefix(prefix)),
            Component::RootDir => {
                // A Windows absolute path is represented by a drive prefix
                // followed by a root component (for example `C:` then `\\`).
                // Clear prior path components while retaining that prefix.
                // Clearing the whole buffer here turns `C:\\work` into the
                // drive-relative `\\work`, which breaks workspace containment.
                #[cfg(windows)]
                let prefix = out
                    .components()
                    .next()
                    .filter(|component| matches!(component, Component::Prefix(_)))
                    .map(|component| component.as_os_str().to_owned());
                out.clear();
                #[cfg(windows)]
                if let Some(prefix) = prefix {
                    out.push(prefix);
                }
                out.push(Component::RootDir);
            }
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(c) => out.push(c),
        }
    }
    if out.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::clean_path;
    use std::path::Path;

    #[cfg(windows)]
    #[test]
    fn cleaning_absolute_windows_paths_preserves_the_drive_prefix() {
        assert_eq!(
            clean_path(Path::new(r"C:\workspace\crate\..\file.txt")),
            Path::new(r"C:\workspace\file.txt")
        );
    }

    #[cfg(unix)]
    #[test]
    fn cleaning_absolute_unix_paths_preserves_the_root() {
        assert_eq!(
            clean_path(Path::new("/workspace/crate/../file.txt")),
            Path::new("/workspace/file.txt")
        );
    }
}
