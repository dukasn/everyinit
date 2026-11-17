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

use crate::PolicyError;
use evin_diagnostic::{Diagnostic, IssueCode};
use evin_repository_policy::validator::ValidationError;

/// Converts policy validation errors into diagnostics.
pub fn violations_to_diagnostics(errors: &[PolicyError]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for e in errors {
        match e {
            PolicyError::Violation { error, issue_code } => {
                diagnostics.push(validation_error_to_diagnostic(error, issue_code));
            }
            PolicyError::GitState(msg) => {
                diagnostics.push(evin_locales::diagnostic(
                    IssueCode::new("policy-git-error").arg("message", msg.clone()),
                ));
            }
            PolicyError::MissingArgument { message, hint } => {
                let mut d = evin_locales::diagnostic(
                    IssueCode::new("policy-missing-argument").arg("message", message.clone()),
                );
                if let Some(h) = hint {
                    d = d.with_hint(h.clone());
                }
                diagnostics.push(d);
            }
            PolicyError::EmptyCommitsNotAllowed => {
                let d = evin_locales::diagnostic(IssueCode::new("issue-commit-rejected"))
                    .with_hint(evin_locales::t!("issue-empty-commits-hint"));
                diagnostics.push(d);
            }
        }
    }
    diagnostics
}

fn validation_error_to_diagnostic(e: &ValidationError, title: &IssueCode) -> Diagnostic {
    use ValidationError::*;
    match e {
        FileSizeExceeded { file, .. }
        | ContentBlockViolated { file, .. }
        | MissingRequiredPattern { file, .. }
        | ForbiddenPatternFound { file, .. }
        | LfsTrackingRequired { file }
        | InvalidFileName { file, .. }
        | PathBlocked { file } => {
            let category = evin_locales::render_issue(title);
            let message = format!("{}\n{}", category, e);
            evin_locales::diagnostic(IssueCode::custom(message))
                .with_file_location(file, None, None)
        }
        _ => {
            let message = evin_locales::render_issue(title);
            evin_locales::diagnostic(IssueCode::custom(e.to_string())).with_hint(message)
        }
    }
}
