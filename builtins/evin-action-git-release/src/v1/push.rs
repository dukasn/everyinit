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

use super::config::PushConfig;
use evin_action_sdk::RunnerContext;
use std::fmt::Display;

fn push_error(remote: &str, tag: Option<&str>, error: impl Display) -> evin_diagnostic::Diagnostic {
    match tag {
        Some(tag) => evin_locales::diagnostic(
            evin_diagnostic::IssueCode::new("git-release-push-with-tag-failed")
                .arg("remote", remote)
                .arg("tag", tag)
                .arg("error", error.to_string()),
        ),
        None => evin_locales::diagnostic(
            evin_diagnostic::IssueCode::new("git-release-push-failed")
                .arg("remote", remote)
                .arg("error", error.to_string()),
        ),
    }
}

fn current_branch_error(error: impl Display) -> evin_diagnostic::Diagnostic {
    evin_locales::diagnostic(
        evin_diagnostic::IssueCode::new("git-release-current-branch-failed")
            .arg("error", error.to_string()),
    )
}

fn prune_format_error(error: impl Display) -> evin_diagnostic::Diagnostic {
    evin_locales::diagnostic(
        evin_diagnostic::IssueCode::new("git-release-prune-tag-format-invalid")
            .arg("error", error.to_string()),
    )
}

fn remote_tags_error(remote: &str, error: impl Display) -> evin_diagnostic::Diagnostic {
    evin_locales::diagnostic(
        evin_diagnostic::IssueCode::new("git-release-prune-tags-list-failed")
            .arg("remote", remote)
            .arg("error", error.to_string()),
    )
}

fn ancestor_error(tag: &str, error: impl Display) -> evin_diagnostic::Diagnostic {
    evin_locales::diagnostic(
        evin_diagnostic::IssueCode::new("git-release-prune-tag-check-failed")
            .arg("tag", tag)
            .arg("error", error.to_string()),
    )
}

fn delete_tag_error(remote: &str, tag: &str, error: impl Display) -> evin_diagnostic::Diagnostic {
    evin_locales::diagnostic(
        evin_diagnostic::IssueCode::new("git-release-prune-tag-delete-failed")
            .arg("remote", remote)
            .arg("tag", tag)
            .arg("error", error.to_string()),
    )
}

// This helper returns the SDK's diagnostic type and its caller uses that same type.
#[allow(clippy::result_large_err)]
pub(super) async fn publish(
    ctx: &RunnerContext,
    push: &PushConfig,
    tag: Option<&str>,
) -> Result<(), evin_diagnostic::Diagnostic> {
    let branch = match &push.branch {
        Some(branch) => branch.clone(),
        None => ctx
            .git()
            .current_branch()
            .await
            .map_err(current_branch_error)?,
    };
    let mut refs = vec![format!("refs/heads/{branch}:refs/heads/{branch}")];
    if let Some(tag) = tag {
        refs.push(format!("refs/tags/{tag}:refs/tags/{tag}"));
    }
    ctx.git()
        .push(&push.remote, &refs, push.force)
        .await
        .map_err(|error| push_error(&push.remote, tag, error))?;

    if !push.prune_stale_tags {
        return Ok(());
    }
    let (Some(tag), Some(base)) = (tag, push.base.as_deref()) else {
        return Ok(());
    };
    if ctx.mode == evin_action_sdk::PipelineMode::DryRun {
        ctx.log.plan(evin_action_sdk::activity::GitPruneTagsPlan {
            remote: push.remote.clone(),
        });
        return Ok(());
    }

    let format = evin_flow::template::regex::compile_template(ctx.tag_format())
        .map_err(prune_format_error)?;
    let base_ref = format!("refs/remotes/{}/{base}", push.remote);
    for (stale, commit) in ctx
        .git()
        .remote_tags(&push.remote)
        .await
        .map_err(|error| remote_tags_error(&push.remote, error))?
    {
        if stale == tag || !format.is_match(&stale) {
            continue;
        }
        // Commits missing from this clone cannot be classified, so their tags
        // are left alone.
        let released = ctx
            .git()
            .is_ancestor(&commit, &base_ref)
            .await
            .map_err(|error| ancestor_error(&stale, error))?;
        if released == Some(false) {
            ctx.git()
                .delete_remote_tag(&push.remote, &stale)
                .await
                .map_err(|error| delete_tag_error(&push.remote, &stale, error))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_fluent(error: evin_diagnostic::Diagnostic, key: &str, expected: String) {
        assert_eq!(error.code.key, key);
        assert_eq!(evin_locales::render_issue(&error.code), expected);
    }

    #[test]
    fn push_errors_use_fluent_messages_with_and_without_a_tag() {
        assert_fluent(
            current_branch_error("not a repository"),
            "git-release-current-branch-failed",
            evin_locales::t!(
                "git-release-current-branch-failed",
                error = "not a repository"
            ),
        );
        assert_fluent(
            push_error("origin", None, "network unavailable"),
            "git-release-push-failed",
            evin_locales::t!(
                "git-release-push-failed",
                remote = "origin",
                error = "network unavailable"
            ),
        );
        assert_fluent(
            push_error("origin", Some("v1.2.3"), "network unavailable"),
            "git-release-push-with-tag-failed",
            evin_locales::t!(
                "git-release-push-with-tag-failed",
                remote = "origin",
                tag = "v1.2.3",
                error = "network unavailable"
            ),
        );
    }

    #[test]
    fn stale_tag_pruning_errors_use_fluent_messages() {
        assert_fluent(
            prune_format_error("invalid template"),
            "git-release-prune-tag-format-invalid",
            evin_locales::t!(
                "git-release-prune-tag-format-invalid",
                error = "invalid template"
            ),
        );
        assert_fluent(
            remote_tags_error("origin", "permission denied"),
            "git-release-prune-tags-list-failed",
            evin_locales::t!(
                "git-release-prune-tags-list-failed",
                remote = "origin",
                error = "permission denied"
            ),
        );
        assert_fluent(
            ancestor_error("v1.2.3", "missing commit"),
            "git-release-prune-tag-check-failed",
            evin_locales::t!(
                "git-release-prune-tag-check-failed",
                tag = "v1.2.3",
                error = "missing commit"
            ),
        );
        assert_fluent(
            delete_tag_error("origin", "v1.2.3", "permission denied"),
            "git-release-prune-tag-delete-failed",
            evin_locales::t!(
                "git-release-prune-tag-delete-failed",
                remote = "origin",
                tag = "v1.2.3",
                error = "permission denied"
            ),
        );
    }
}
