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

use evin_config::workspace::HookRunner;

pub fn build_hook_payload(hook_runner: Option<&HookRunner>, install_hint: Option<&str>) -> String {
    if let Some(hook_runner) = hook_runner {
        let command = std::iter::once(&hook_runner.command)
            .chain(hook_runner.args.iter())
            .map(|argument| shell_quote(argument))
            .collect::<Vec<_>>()
            .join(" ");

        return format!(
            r#"repo_root="$(git rev-parse --show-toplevel 2>/dev/null)" || exit 1
cd "$repo_root" || exit 1
exec {} run "$(basename "$0")" -- "$@""#,
            command
        );
    }

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

fn shell_quote(argument: &str) -> String {
    format!("'{}'", argument.replace('\'', "'\"'\"'"))
}

#[cfg(test)]
mod tests {
    use super::build_hook_payload;
    use evin_config::workspace::HookRunner;

    #[test]
    fn default_runner_uses_evin_from_path() {
        let payload = build_hook_payload(None, Some("cargo install evin"));

        assert!(payload.contains("command -v evin"));
        assert!(payload.contains("exec evin run"));
    }

    #[test]
    fn configured_runner_executes_from_repository_root() {
        let runner = HookRunner {
            command: "cargo".into(),
            args: vec!["evin-dev".into()],
        };
        let payload = build_hook_payload(Some(&runner), None);

        assert!(payload.contains("git rev-parse --show-toplevel"));
        assert!(payload.contains("cd \"$repo_root\""));
        assert!(payload.contains("exec 'cargo' 'evin-dev' run"));
        assert!(!payload.contains("command -v evin"));
    }

    #[test]
    fn configured_runner_quotes_every_argument() {
        let runner = HookRunner {
            command: "cargo".into(),
            args: vec!["run".into(), "--package".into(), "evin's-source".into()],
        };
        let payload = build_hook_payload(Some(&runner), None);

        assert!(payload.contains("exec 'cargo' 'run' '--package' 'evin'\"'\"'s-source' run"));
    }
}
