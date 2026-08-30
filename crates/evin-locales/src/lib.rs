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

pub mod bundle;

use evin_diagnostic::{Diagnostic, IssueCode};
pub use fluent_bundle::FluentArgs;

/// Renders a Fluent message from a stable key and string-valued arguments.
pub fn render_message(key: &str, args: &[(&str, String)]) -> String {
    let mut fluent_args = FluentArgs::new();
    for (name, value) in args {
        fluent_args.set(*name, value.as_str());
    }
    bundle::getf(key, &fluent_args)
}

/// Macro for looking up localized messages by token string.
///
/// # Examples
/// ```ignore
/// t!("app-description")
/// t!("suggestion-hint", suggestion = "sync")
/// t!("hook-preview", hook = "pre-commit", count = 1, change = "create")
/// ```
#[macro_export]
macro_rules! t {
    ($key:expr) => {
        $crate::bundle::get($key)
    };
    ($key:expr, $($arg:ident = $val:expr),* $(,)?) => {{
        let mut args = $crate::FluentArgs::new();
        $(
            args.set(stringify!($arg), $val);
        )*
        $crate::bundle::getf($key, &args)
    }};
}

/// Macro for checking if an IssueCode matches a specific key string.
#[macro_export]
macro_rules! matches_key {
    ($code:expr, $key:expr) => {
        $code.key == $key
    };
}

/// Generic, locale-native renderer for IssueCode objects.
pub fn render_issue(code: &IssueCode) -> String {
    if code.is_custom() {
        return code
            .args
            .iter()
            .find(|(k, _)| *k == "message")
            .map(|(_, v)| v.clone())
            .unwrap_or_default();
    }
    if code.args.is_empty() {
        bundle::get(code.key)
    } else {
        let mut args = FluentArgs::new();
        for (k, v) in &code.args {
            args.set(*k, v.as_str());
        }
        bundle::getf(code.key, &args)
    }
}

/// Constructs a Diagnostic directly from an IssueCode.
/// Looks up `{key}-title` in FTL for the title, falling back to the rendered message or "Error".
pub fn diagnostic(code: IssueCode) -> Diagnostic {
    let title = bundle::get_optional(&format!("{}-title", code.key)).unwrap_or_else(|| {
        if code.is_custom() {
            "Error".to_string()
        } else {
            render_issue(&code)
        }
    });
    Diagnostic::new(title, code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_t_macro() {
        assert_eq!(t!("app-description"), "Evin — The Workflow Engine");
        assert_eq!(
            t!("suggestion-hint", suggestion = "sync"),
            "Did you mean 'sync'?"
        );
        assert_eq!(
            t!(
                "hook-preview",
                hook = "pre-commit",
                count = 1,
                change = "create"
            ),
            "pre-commit — create (1 task)"
        );
    }

    #[test]
    fn test_render_issue_generic() {
        let code = IssueCode::new("tag-already-exists").arg("tag", "v1.0.0");
        assert_eq!(
            render_issue(&code),
            "Tag 'v1.0.0' already exists. This usually indicates that no new releasable commits were found since the last release, or the tag was manually created."
        );
    }

    #[test]
    fn test_diagnostic_factory() {
        let code = IssueCode::new("issue-invalid-hook").arg("hook", "pre-commit");
        let diag = diagnostic(code);
        assert_eq!(diag.title, "Unknown Hook");
    }

    #[test]
    fn test_template_localization() {
        let gitrules = bundle::get("init-template-gitrules");
        assert!(
            !gitrules.contains("missing locale key"),
            "init-template-gitrules missing: {}",
            gitrules
        );
        assert!(gitrules.contains("version: 1"));
        assert!(gitrules.contains("commits:"));

        let workspace = bundle::get("init-template-workspace");
        assert!(
            !workspace.contains("missing locale key"),
            "init-template-workspace missing: {}",
            workspace
        );
        assert!(workspace.contains("workspace:"));

        let module = bundle::get("init-template-module");
        assert!(
            !module.contains("missing locale key"),
            "init-template-module missing: {}",
            module
        );
        assert!(module.contains("module:"));
    }
}
