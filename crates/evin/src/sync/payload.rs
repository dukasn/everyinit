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

pub fn build_hook_payload(install_hint: Option<&str>) -> String {
    let hint_msg = match install_hint {
        Some(hint) => format!(
            r#"echo -e "   Please install it using: \033[1;36m{}\033[0m""#,
            hint
        ),
        None => r#"echo -e "   Please ensure EVIN is installed and available in your PATH.""#
            .to_string(),
    };

    format!(
        r#"if ! command -v evin >/dev/null 2>&1; then
  echo -e "\033[1;31m×  EVIN Error\033[0m"
  echo -e "   The 'evin' CLI is not installed or not in your PATH."
  {}
  exit 1
fi
exec evin run "$(basename "$0")" -- "$@""#,
        hint_msg
    )
}
