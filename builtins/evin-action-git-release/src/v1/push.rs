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

pub(super) async fn publish(
    ctx: &RunnerContext,
    push: &PushConfig,
    tag: Option<&str>,
) -> Result<(), String> {
    let branch = match &push.branch {
        Some(branch) => branch.clone(),
        None => ctx
            .git()
            .current_branch()
            .await
            .map_err(|e| e.to_string())?,
    };
    let mut refs = vec![format!("refs/heads/{branch}:refs/heads/{branch}")];
    if let Some(tag) = tag {
        refs.push(format!("refs/tags/{tag}:refs/tags/{tag}"));
    }
    ctx.git()
        .push(&push.remote, &refs, push.force)
        .await
        .map_err(|e| {
            format!(
                "push to '{}' failed; the local commit{} is intact and the push can be retried: {e}",
                push.remote,
                tag.map(|t| format!(" and tag {t}")).unwrap_or_default(),
            )
        })?;

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
        .map_err(|e| format!("tag format is not usable for pruning: {e}"))?;
    let base_ref = format!("refs/remotes/{}/{base}", push.remote);
    for (stale, commit) in ctx
        .git()
        .remote_tags(&push.remote)
        .await
        .map_err(|e| e.to_string())?
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
            .map_err(|e| e.to_string())?;
        if released == Some(false) {
            ctx.git()
                .delete_remote_tag(&push.remote, &stale)
                .await
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
