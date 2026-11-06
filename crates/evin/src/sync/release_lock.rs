// Copyright (C) 2026 Eduardo Marques Santos

use evin_action_sdk::release::{self, ReleaseLock};
use evin_diagnostic::{Diagnostic, IssueCode};
use std::path::Path;
use tokio::process::Command;

pub enum ReconcileOutcome {
    Unneeded,
    Planned(String),
    Staged,
}

pub async fn reconcile(
    dry_run: bool,
    config: &evin_config::parser::EverInitConfig,
) -> Result<ReconcileOutcome, Diagnostic> {
    let root = std::env::current_dir().map_err(diagnostic)?;
    let unresolved = git_text(&root, &["diff", "--name-only", "--diff-filter=U"]).await?;
    let paths = unresolved
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>();
    if !paths.contains(&release::RELEASE_LOCK_PATH) {
        return Ok(ReconcileOutcome::Unneeded);
    }
    if paths != [release::RELEASE_LOCK_PATH] {
        return Err(diagnostic(format!(
            "{} must be the only unresolved path; found: {}",
            release::RELEASE_LOCK_PATH,
            paths.join(", ")
        )));
    }

    let rules = config
        .gitrules
        .as_ref()
        .ok_or_else(|| diagnostic("release-lock reconciliation requires .evin/gitrules.flow"))?;
    let policies = rules
        .releases
        .as_ref()
        .map(|r| &r.packages)
        .ok_or_else(|| diagnostic("GitRules has no release package policy"))?;
    release::validate_policies(policies).map_err(diagnostic)?;

    let base = read_stage(&root, 1)
        .await?
        .map(|source| release::parse_lock(&source))
        .transpose()
        .map_err(diagnostic)?;
    let ours = required_stage(&root, 2).await?;
    let theirs = required_stage(&root, 3).await?;
    let fingerprints = release::fingerprint_index(&root, policies)
        .await
        .map_err(diagnostic)?;
    let lock = release::reconcile_locks(base.as_ref(), &ours, &theirs, policies, &fingerprints)
        .map_err(diagnostic)?;
    lock.validate(policies).map_err(diagnostic)?;
    if dry_run {
        return Ok(ReconcileOutcome::Planned(
            lock.to_json().map_err(diagnostic)?,
        ));
    }
    release::write_lock(&root, &lock).map_err(diagnostic)?;
    let output = Command::new("git")
        .args(["add", "--", release::RELEASE_LOCK_PATH])
        .current_dir(&root)
        .output()
        .await
        .map_err(diagnostic)?;
    if !output.status.success() {
        return Err(diagnostic(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }
    Ok(ReconcileOutcome::Staged)
}

async fn required_stage(root: &Path, stage: u8) -> Result<ReleaseLock, Diagnostic> {
    let source = read_stage(root, stage)
        .await?
        .ok_or_else(|| diagnostic(format!("release lock has no stage {stage}")))?;
    release::parse_lock(&source).map_err(diagnostic)
}

async fn read_stage(root: &Path, stage: u8) -> Result<Option<String>, Diagnostic> {
    let spec = format!(":{stage}:{}", release::RELEASE_LOCK_PATH);
    let output = Command::new("git")
        .args(["show", &spec])
        .current_dir(root)
        .output()
        .await
        .map_err(diagnostic)?;
    if output.status.success() {
        Ok(Some(String::from_utf8_lossy(&output.stdout).into_owned()))
    } else {
        Ok(None)
    }
}

async fn git_text(root: &Path, args: &[&str]) -> Result<String, Diagnostic> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .await
        .map_err(diagnostic)?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(diagnostic(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ))
    }
}

fn diagnostic(error: impl ToString) -> Diagnostic {
    evin_locales::diagnostic(IssueCode::custom(error.to_string()))
}
