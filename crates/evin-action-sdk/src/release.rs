// Copyright (C) 2026 Eduardo Marques Santos
//
// Deterministic, Git-backed package release state shared by native actions and
// the CLI. The lock file is data; policy remains in gitrules.flow.

use evin_gitrules::{PackageReleasePolicy, PackageRevisionScope};
use glob::Pattern;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use tokio::process::Command;

pub const RELEASE_LOCK_PATH: &str = ".evin/release-lock.yaml";
pub const PACKAGE_RELEASE_SUBJECT: &str = "chore(release): refresh package revisions";
pub const PACKAGE_RELEASE_TRAILER: &str = "Evin-Release-Kind: package";

#[derive(Debug, thiserror::Error)]
pub enum ReleaseError {
    #[error("invalid package release policy: {0}")]
    Policy(String),
    #[error("invalid release lock: {0}")]
    Lock(String),
    #[error("Git operation failed: {0}")]
    Git(String),
    #[error("release lock I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("release lock YAML failed: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("release JSON failed: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReleaseIdentity {
    pub version: String,
    pub tag: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RevisionState {
    pub version: String,
    pub revision: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PackageLock {
    pub fingerprint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<u32>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub targets: BTreeMap<String, RevisionState>,
}

impl PackageLock {
    pub fn shared(&self) -> Option<RevisionState> {
        Some(RevisionState {
            version: self.version.clone()?,
            revision: self.revision?,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReleaseLock {
    pub version: u32,
    pub release: ReleaseIdentity,
    #[serde(default)]
    pub packages: BTreeMap<String, PackageLock>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReleaseArtifact {
    pub schema: u32,
    pub release: ReleaseIdentity,
    pub packages: BTreeMap<String, ArtifactPackage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ArtifactPackage {
    pub revisions: BTreeMap<String, RevisionState>,
}

impl ReleaseLock {
    pub fn validate(
        &self,
        policies: &BTreeMap<String, PackageReleasePolicy>,
    ) -> Result<(), ReleaseError> {
        if self.version != 1
            || self.release.version.trim().is_empty()
            || self.release.tag.trim().is_empty()
        {
            return Err(ReleaseError::Lock(
                "unsupported schema or empty release identity".into(),
            ));
        }
        for id in self.packages.keys() {
            if !policies.contains_key(id) {
                return Err(ReleaseError::Lock(format!("orphaned package '{id}'")));
            }
        }
        for (id, policy) in policies {
            let Some(package) = self.packages.get(id) else {
                continue;
            };
            if !is_fingerprint(&package.fingerprint) {
                return Err(ReleaseError::Lock(format!(
                    "package '{id}' has an invalid fingerprint"
                )));
            }
            match policy.revision.scope {
                PackageRevisionScope::Package => {
                    if package.shared().is_none()
                        || !package.targets.is_empty()
                        || package.revision == Some(0)
                    {
                        return Err(ReleaseError::Lock(format!(
                            "package '{id}' does not contain shared revision state"
                        )));
                    }
                }
                PackageRevisionScope::Target => {
                    if package.version.is_some() || package.revision.is_some() {
                        return Err(ReleaseError::Lock(format!(
                            "package '{id}' mixes shared and target state"
                        )));
                    }
                    let wanted = normalized_targets(policy)?;
                    if package.targets.keys().cloned().collect::<BTreeSet<_>>() != wanted
                        || package
                            .targets
                            .values()
                            .any(|state| state.version.trim().is_empty() || state.revision == 0)
                    {
                        return Err(ReleaseError::Lock(format!(
                            "package '{id}' target state does not match GitRules"
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    pub fn artifact(
        &self,
        policies: &BTreeMap<String, PackageReleasePolicy>,
    ) -> Result<ReleaseArtifact, ReleaseError> {
        let mut packages = BTreeMap::new();
        for (id, policy) in policies {
            let Some(package) = self.packages.get(id) else {
                continue;
            };
            let revisions = match policy.revision.scope {
                PackageRevisionScope::Package => {
                    let state = package.shared().ok_or_else(|| {
                        ReleaseError::Lock(format!("package '{id}' has no shared state"))
                    })?;
                    normalized_targets(policy)?
                        .into_iter()
                        .map(|target| (target, state.clone()))
                        .collect()
                }
                PackageRevisionScope::Target => package.targets.clone(),
            };
            packages.insert(id.clone(), ArtifactPackage { revisions });
        }
        Ok(ReleaseArtifact {
            schema: 1,
            release: self.release.clone(),
            packages,
        })
    }

    pub fn to_yaml(&self) -> Result<String, ReleaseError> {
        let mut text = serde_yaml::to_string(self)?;
        if !text.ends_with('\n') {
            text.push('\n');
        }
        Ok(text)
    }
}

pub fn parse_lock(source: &str) -> Result<ReleaseLock, ReleaseError> {
    let lock: ReleaseLock = serde_yaml::from_str(source)?;
    if lock.version != 1 {
        return Err(ReleaseError::Lock(format!(
            "unsupported schema version {}",
            lock.version
        )));
    }
    Ok(lock)
}

pub fn read_lock(root: &Path) -> Result<Option<ReleaseLock>, ReleaseError> {
    let path = root.join(RELEASE_LOCK_PATH);
    match std::fs::read_to_string(path) {
        Ok(source) => parse_lock(&source).map(Some),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub fn write_lock(root: &Path, lock: &ReleaseLock) -> Result<(), ReleaseError> {
    let path = root.join(RELEASE_LOCK_PATH);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, lock.to_yaml()?)?;
    Ok(())
}

pub fn validate_policies(
    policies: &BTreeMap<String, PackageReleasePolicy>,
) -> Result<(), ReleaseError> {
    for (id, policy) in policies {
        if id.is_empty()
            || !id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return Err(ReleaseError::Policy(format!("invalid package ID '{id}'")));
        }
        if policy.watch.is_empty() {
            return Err(ReleaseError::Policy(format!(
                "package '{id}' has no watched paths"
            )));
        }
        normalized_targets(policy)?;
        for raw in &policy.watch {
            let path = Path::new(raw);
            if raw.trim().is_empty()
                || path.is_absolute()
                || path
                    .components()
                    .any(|c| matches!(c, std::path::Component::ParentDir))
            {
                return Err(ReleaseError::Policy(format!(
                    "package '{id}' has unsafe watch glob '{raw}'"
                )));
            }
            Pattern::new(raw).map_err(|e| {
                ReleaseError::Policy(format!(
                    "package '{id}' has invalid watch glob '{raw}': {e}"
                ))
            })?;
            if Pattern::new(raw).is_ok_and(|pattern| pattern.matches(RELEASE_LOCK_PATH)) {
                return Err(ReleaseError::Policy(format!(
                    "package '{id}' may not watch {RELEASE_LOCK_PATH}"
                )));
            }
        }
    }
    Ok(())
}

fn normalized_targets(policy: &PackageReleasePolicy) -> Result<BTreeSet<String>, ReleaseError> {
    let targets = policy.targets.iter().cloned().collect::<BTreeSet<_>>();
    if targets.is_empty()
        || targets.len() != policy.targets.len()
        || targets
            .iter()
            .any(|v| v != "deb" && v != "apk" && v != "arch" && v != "vsix")
    {
        return Err(ReleaseError::Policy(
            "targets must be a non-empty unique subset of deb, apk, arch, and vsix".into(),
        ));
    }
    Ok(targets)
}

fn is_fingerprint(value: &str) -> bool {
    value
        .strip_prefix("sha256:")
        .is_some_and(|hex| hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
}

pub fn resolve_lock(
    previous: Option<&ReleaseLock>,
    release: ReleaseIdentity,
    policies: &BTreeMap<String, PackageReleasePolicy>,
    fingerprints: &BTreeMap<String, String>,
) -> Result<ReleaseLock, ReleaseError> {
    validate_policies(policies)?;
    let mut packages = BTreeMap::new();
    for (id, policy) in policies {
        let fingerprint = fingerprints
            .get(id)
            .ok_or_else(|| ReleaseError::Lock(format!("missing fingerprint for package '{id}'")))?
            .clone();
        let old = previous.and_then(|lock| lock.packages.get(id));
        let changed = old.is_none_or(|old| old.fingerprint != fingerprint);
        let package = match policy.revision.scope {
            PackageRevisionScope::Package => {
                let old_states = old.map(all_states).unwrap_or_default();
                let identical = old_states.windows(2).all(|pair| pair[0] == pair[1]);
                let prior = old
                    .and_then(PackageLock::shared)
                    .or_else(|| identical.then(|| old_states.first().cloned()).flatten());
                let state = next_state(
                    prior.as_ref(),
                    &old_states,
                    &release.version,
                    changed || old.is_some_and(|p| p.shared().is_none() && !identical),
                );
                PackageLock {
                    fingerprint,
                    version: Some(state.version),
                    revision: Some(state.revision),
                    targets: BTreeMap::new(),
                }
            }
            PackageRevisionScope::Target => {
                let shared = old.and_then(PackageLock::shared);
                let mut targets = BTreeMap::new();
                for target in normalized_targets(policy)? {
                    let prior_states = old.map(all_states).unwrap_or_default();
                    let state = if let Some(prior) = old.and_then(|p| p.targets.get(&target)) {
                        next_state(Some(prior), &prior_states, &release.version, changed)
                    } else if let Some(shared) = shared.as_ref() {
                        shared.clone()
                    } else {
                        RevisionState {
                            version: release.version.clone(),
                            revision: 1,
                        }
                    };
                    targets.insert(target, state);
                }
                PackageLock {
                    fingerprint,
                    version: None,
                    revision: None,
                    targets,
                }
            }
        };
        packages.insert(id.clone(), package);
    }
    Ok(ReleaseLock {
        version: 1,
        release,
        packages,
    })
}

pub fn reconcile_locks(
    base: Option<&ReleaseLock>,
    ours: &ReleaseLock,
    theirs: &ReleaseLock,
    policies: &BTreeMap<String, PackageReleasePolicy>,
    fingerprints: &BTreeMap<String, String>,
) -> Result<ReleaseLock, ReleaseError> {
    if ours.release != theirs.release {
        return Err(ReleaseError::Lock(
            "both sides have different semantic release identities; rebase onto the canonical tagged history".into(),
        ));
    }
    let sides = [base, Some(ours), Some(theirs)];
    let mut prior_packages = BTreeMap::new();
    for (id, policy) in policies {
        let fingerprint = fingerprints
            .get(id)
            .ok_or_else(|| ReleaseError::Lock(format!("missing fingerprint for package '{id}'")))?;
        if let Some(package) = sides
            .iter()
            .flatten()
            .filter_map(|lock| lock.packages.get(id))
            .find(|package| &package.fingerprint == fingerprint)
        {
            prior_packages.insert(id.clone(), package.clone());
            continue;
        }
        let states = sides
            .iter()
            .flatten()
            .filter_map(|lock| lock.packages.get(id))
            .flat_map(all_states)
            .collect::<Vec<_>>();
        let max_for = |target: Option<&str>| -> Option<RevisionState> {
            sides
                .iter()
                .flatten()
                .filter_map(|lock| lock.packages.get(id))
                .flat_map(|package| {
                    package
                        .shared()
                        .into_iter()
                        .chain(target.and_then(|name| package.targets.get(name)).cloned())
                })
                .max_by_key(|state| state.revision)
        };
        let synthetic = match policy.revision.scope {
            PackageRevisionScope::Package => {
                let state = max_for(None)
                    .or_else(|| states.iter().max_by_key(|state| state.revision).cloned());
                PackageLock {
                    fingerprint: "sha256:merge-base".into(),
                    version: state.as_ref().map(|s| s.version.clone()),
                    revision: state.map(|s| s.revision),
                    targets: BTreeMap::new(),
                }
            }
            PackageRevisionScope::Target => {
                let targets = normalized_targets(policy)?
                    .into_iter()
                    .filter_map(|target| max_for(Some(&target)).map(|state| (target, state)))
                    .collect();
                PackageLock {
                    fingerprint: "sha256:merge-base".into(),
                    version: None,
                    revision: None,
                    targets,
                }
            }
        };
        prior_packages.insert(id.clone(), synthetic);
    }
    let prior = ReleaseLock {
        version: 1,
        release: ours.release.clone(),
        packages: prior_packages,
    };
    resolve_lock(Some(&prior), ours.release.clone(), policies, fingerprints)
}

fn all_states(package: &PackageLock) -> Vec<RevisionState> {
    package
        .shared()
        .into_iter()
        .chain(package.targets.values().cloned())
        .collect()
}

fn next_state(
    prior: Option<&RevisionState>,
    states: &[RevisionState],
    version: &str,
    changed: bool,
) -> RevisionState {
    if let Some(prior) = prior
        && !changed
    {
        return prior.clone();
    }
    let same_version = states
        .iter()
        .filter(|state| state.version == version)
        .map(|state| state.revision)
        .max();
    RevisionState {
        version: version.to_string(),
        revision: same_version.map_or(1, |revision| revision.saturating_add(1)),
    }
}

pub async fn fingerprint_index(
    root: &Path,
    policies: &BTreeMap<String, PackageReleasePolicy>,
) -> Result<BTreeMap<String, String>, ReleaseError> {
    validate_policies(policies)?;
    let output = git(root)
        .args(["ls-files", "-s", "-z"])
        .output()
        .await
        .map_err(|e| ReleaseError::Git(e.to_string()))?;
    if !output.status.success() {
        return Err(ReleaseError::Git(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }
    let mut entries = Vec::new();
    for raw in output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|raw| !raw.is_empty())
    {
        let text = String::from_utf8_lossy(raw);
        let Some((metadata, path)) = text.split_once('\t') else {
            continue;
        };
        let mut parts = metadata.split_whitespace();
        let mode = parts.next().unwrap_or_default();
        let oid = parts.next().unwrap_or_default();
        let stage = parts.next().unwrap_or_default();
        if stage == "0" && matches!(mode, "100644" | "100755") {
            entries.push((path.to_string(), mode.to_string(), oid.to_string()));
        }
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let mut result = BTreeMap::new();
    for (id, policy) in policies {
        let patterns = policy
            .watch
            .iter()
            .map(|value| Pattern::new(value).expect("validated pattern"))
            .collect::<Vec<_>>();
        let selected = entries
            .iter()
            .filter(|(path, _, _)| patterns.iter().any(|pattern| pattern.matches(path)))
            .collect::<Vec<_>>();
        if selected.is_empty() {
            return Err(ReleaseError::Policy(format!(
                "package '{id}' watch globs match no tracked regular files"
            )));
        }
        let mut hasher = Sha256::new();
        for (path, mode, oid) in selected {
            let blob = git(root)
                .args(["cat-file", "blob", oid])
                .output()
                .await
                .map_err(|e| ReleaseError::Git(e.to_string()))?;
            if !blob.status.success() {
                return Err(ReleaseError::Git(
                    String::from_utf8_lossy(&blob.stderr).into_owned(),
                ));
            }
            hash_field(&mut hasher, path.as_bytes());
            hash_field(&mut hasher, mode.as_bytes());
            hash_field(&mut hasher, &blob.stdout);
        }
        let digest = hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        result.insert(id.clone(), format!("sha256:{digest}"));
    }
    Ok(result)
}

fn hash_field(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}

pub async fn dirty_paths(root: &Path) -> Result<Vec<String>, ReleaseError> {
    let output = git(root)
        .args(["status", "--porcelain=v1", "-z", "--untracked-files=all"])
        .output()
        .await
        .map_err(|e| ReleaseError::Git(e.to_string()))?;
    if !output.status.success() {
        return Err(ReleaseError::Git(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }
    let mut paths = Vec::new();
    for entry in output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|entry| entry.len() > 3)
    {
        paths.push(String::from_utf8_lossy(&entry[3..]).replace('\\', "/"));
    }
    Ok(paths)
}

pub fn path_is_watched(path: &str, policies: &BTreeMap<String, PackageReleasePolicy>) -> bool {
    policies
        .values()
        .flat_map(|policy| &policy.watch)
        .any(|raw| Pattern::new(raw).is_ok_and(|pattern| pattern.matches(path)))
}

fn git(root: &Path) -> Command {
    let mut command = Command::new("git");
    command.current_dir(root);
    command
}

pub fn workspace_root(path: impl Into<PathBuf>) -> PathBuf {
    path.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policies(scope: PackageRevisionScope) -> BTreeMap<String, PackageReleasePolicy> {
        BTreeMap::from([(
            "app".into(),
            PackageReleasePolicy {
                watch: vec!["src/**".into()],
                targets: vec!["deb".into(), "apk".into(), "arch".into()],
                revision: evin_gitrules::PackageRevisionPolicy { scope },
            },
        )])
    }

    #[test]
    fn stable_artifact_expands_shared_state() {
        let lock = resolve_lock(
            None,
            ReleaseIdentity {
                version: "1.2.3".into(),
                tag: "v1.2.3".into(),
            },
            &policies(PackageRevisionScope::Package),
            &BTreeMap::from([("app".into(), format!("sha256:{}", "a".repeat(64)))]),
        )
        .unwrap();
        let json = serde_json::to_string(
            &lock
                .artifact(&policies(PackageRevisionScope::Package))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            json,
            r#"{"schema":1,"release":{"version":"1.2.3","tag":"v1.2.3"},"packages":{"app":{"revisions":{"apk":{"version":"1.2.3","revision":1},"arch":{"version":"1.2.3","revision":1},"deb":{"version":"1.2.3","revision":1}}}}}"#
                .to_string()
        );
    }

    #[test]
    fn target_to_shared_allocates_after_divergent_state() {
        let old = ReleaseLock {
            version: 1,
            release: ReleaseIdentity {
                version: "1.0.0".into(),
                tag: "v1.0.0".into(),
            },
            packages: BTreeMap::from([(
                "app".into(),
                PackageLock {
                    fingerprint: format!("sha256:{}", "a".repeat(64)),
                    version: None,
                    revision: None,
                    targets: BTreeMap::from([
                        (
                            "apk".into(),
                            RevisionState {
                                version: "1.0.0".into(),
                                revision: 2,
                            },
                        ),
                        (
                            "deb".into(),
                            RevisionState {
                                version: "1.0.0".into(),
                                revision: 4,
                            },
                        ),
                    ]),
                },
            )]),
        };
        let next = resolve_lock(
            Some(&old),
            old.release.clone(),
            &policies(PackageRevisionScope::Package),
            &BTreeMap::from([("app".into(), format!("sha256:{}", "a".repeat(64)))]),
        )
        .unwrap();
        assert_eq!(next.packages["app"].revision, Some(5));
    }

    #[test]
    fn target_scope_changes_follow_explicit_migration_rules() {
        let fingerprint = format!("sha256:{}", "a".repeat(64));
        let identity = ReleaseIdentity {
            version: "1.0.0".into(),
            tag: "v1.0.0".into(),
        };
        let shared = ReleaseLock {
            version: 1,
            release: identity.clone(),
            packages: BTreeMap::from([(
                "app".into(),
                PackageLock {
                    fingerprint: fingerprint.clone(),
                    version: Some("1.0.0".into()),
                    revision: Some(7),
                    targets: BTreeMap::new(),
                },
            )]),
        };
        let target = resolve_lock(
            Some(&shared),
            identity.clone(),
            &policies(PackageRevisionScope::Target),
            &BTreeMap::from([("app".into(), fingerprint.clone())]),
        )
        .unwrap();
        assert!(
            target.packages["app"]
                .targets
                .values()
                .all(|state| state.revision == 7)
        );
        let collapsed = resolve_lock(
            Some(&target),
            identity,
            &policies(PackageRevisionScope::Package),
            &BTreeMap::from([("app".into(), fingerprint)]),
        )
        .unwrap();
        assert_eq!(collapsed.packages["app"].revision, Some(7));
    }

    #[tokio::test]
    async fn fingerprint_uses_canonical_index_bytes_and_mode() {
        let temp = tempfile::TempDir::new().unwrap();
        let git = |args: &[&str]| {
            let status = std::process::Command::new("git")
                .args(args)
                .current_dir(temp.path())
                .status()
                .unwrap();
            assert!(status.success());
        };
        git(&["init", "-q"]);
        std::fs::create_dir(temp.path().join("src")).unwrap();
        std::fs::write(temp.path().join("src/app"), "a\nb\n").unwrap();
        git(&["add", "src/app"]);
        let policy = policies(PackageRevisionScope::Package);
        let first = fingerprint_index(temp.path(), &policy).await.unwrap();
        std::fs::write(temp.path().join("src/app"), "a\r\nb\r\n").unwrap();
        let worktree_changed = fingerprint_index(temp.path(), &policy).await.unwrap();
        assert_eq!(first, worktree_changed);
        git(&["update-index", "--chmod=+x", "src/app"]);
        let executable = fingerprint_index(temp.path(), &policy).await.unwrap();
        assert_ne!(first, executable);
    }

    #[test]
    fn reconciliation_reuses_a_matching_side_and_increments_new_merge() {
        let policy = policies(PackageRevisionScope::Package);
        let identity = ReleaseIdentity {
            version: "1.0.0".into(),
            tag: "v1.0.0".into(),
        };
        let lock = |fingerprint: &str, revision| ReleaseLock {
            version: 1,
            release: identity.clone(),
            packages: BTreeMap::from([(
                "app".into(),
                PackageLock {
                    fingerprint: format!("sha256:{}", fingerprint.repeat(64)),
                    version: Some("1.0.0".into()),
                    revision: Some(revision),
                    targets: BTreeMap::new(),
                },
            )]),
        };
        let base = lock("a", 1);
        let ours = lock("b", 2);
        let theirs = lock("c", 3);
        let matched = reconcile_locks(
            Some(&base),
            &ours,
            &theirs,
            &policy,
            &BTreeMap::from([("app".into(), theirs.packages["app"].fingerprint.clone())]),
        )
        .unwrap();
        assert_eq!(matched.packages["app"].revision, Some(3));
        let merged = reconcile_locks(
            Some(&base),
            &ours,
            &theirs,
            &policy,
            &BTreeMap::from([("app".into(), format!("sha256:{}", "d".repeat(64)))]),
        )
        .unwrap();
        assert_eq!(merged.packages["app"].revision, Some(4));
    }

    #[test]
    fn vsix_target_is_supported_in_package_policy() {
        let policy = BTreeMap::from([(
            "vscode".into(),
            PackageReleasePolicy {
                watch: vec!["editors/vscode/**".into()],
                targets: vec!["vsix".into()],
                revision: evin_gitrules::PackageRevisionPolicy {
                    scope: PackageRevisionScope::Package,
                },
            },
        )]);
        let lock = resolve_lock(
            None,
            ReleaseIdentity {
                version: "0.2.0".into(),
                tag: "v0.2.0".into(),
            },
            &policy,
            &BTreeMap::from([("vscode".into(), format!("sha256:{}", "e".repeat(64)))]),
        )
        .unwrap();
        let artifact = lock.artifact(&policy).unwrap();
        assert_eq!(
            artifact.packages["vscode"].revisions["vsix"].version,
            "0.2.0"
        );
    }
}
