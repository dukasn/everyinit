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

use crate::models::{
    ActionOpcode, ReasonCode, ReleaseFlags, ReleaseSyncPlan, ReleaseSyncPlanAction,
};
use crate::traits::{
    ArtifactInspector, GitStateProvider, NotesExtractor, PlanSink, PolicyEvaluator, ProtocolError,
    RemoteStateSource,
};
use std::collections::{HashMap, HashSet};

/// Offline, deterministic reconciliation state machine implementing ECP-1.0
pub struct Reconciler<'a> {
    pub state_source: &'a dyn RemoteStateSource,
    pub notes_extractor: &'a dyn NotesExtractor,
    pub git_provider: &'a dyn GitStateProvider,
    pub artifact_inspector: &'a dyn ArtifactInspector,
    pub policy_evaluator: &'a dyn PolicyEvaluator,
    pub plan_sink: &'a mut dyn PlanSink,
}

impl<'a> Reconciler<'a> {
    pub fn new(
        state_source: &'a dyn RemoteStateSource,
        notes_extractor: &'a dyn NotesExtractor,
        git_provider: &'a dyn GitStateProvider,
        artifact_inspector: &'a dyn ArtifactInspector,
        policy_evaluator: &'a dyn PolicyEvaluator,
        plan_sink: &'a mut dyn PlanSink,
    ) -> Self {
        Self {
            state_source,
            notes_extractor,
            git_provider,
            artifact_inspector,
            policy_evaluator,
            plan_sink,
        }
    }

    pub fn reconcile(&mut self) -> Result<ReleaseSyncPlan, ProtocolError> {
        let remote_state = self.state_source.load_remote_state()?;
        if remote_state.v != 1 {
            return Err(ProtocolError::UnsupportedSchemaVersion(remote_state.v));
        }

        let local_tags = self.git_provider.local_tags()?;
        let mut remote_releases_by_tag: HashMap<&str, &crate::models::RemoteRelease> =
            HashMap::new();
        for release in &remote_state.releases {
            remote_releases_by_tag.insert(&release.tag, release);
        }

        let mut actions = Vec::new();
        let mut action_index = 1usize;

        let next_id = |index: &mut usize| {
            let id = format!("act-{:03}", *index);
            *index += 1;
            id
        };

        let mut processed_remote_tags = HashSet::new();

        // 1. Reconcile each local tag against remote state
        for local in &local_tags {
            let note_slice = self.notes_extractor.extract_notes(&local.tag)?;
            let local_artifacts = self.artifact_inspector.local_artifacts(&local.tag)?;
            let is_prerelease = self.policy_evaluator.is_prerelease(&local.tag);
            let is_draft = self.policy_evaluator.is_draft(&local.tag);

            if let Some(remote) = remote_releases_by_tag.get(local.tag.as_str()) {
                processed_remote_tags.insert(remote.tag.as_str());
                let actions_before = actions.len();

                // Check commit target drift
                if let Some(remote_commit) = &remote.commit
                    && !remote_commit.is_empty()
                    && remote_commit != &local.commit
                {
                    actions.push(ReleaseSyncPlanAction {
                        id: next_id(&mut action_index),
                        op: ActionOpcode::RelRetarget.opcode_number(),
                        code: ActionOpcode::RelRetarget,
                        reason: ReasonCode::DiffTargetDrift,
                        r#ref: local.tag.clone(),
                        commit: Some(local.commit.clone()),
                        title: None,
                        notes_src: None,
                        flags: None,
                        src: None,
                        dest: None,
                    });
                }

                // Check release notes sync
                if let Some(slice) = &note_slice {
                    let needs_sync = match &remote.body_sha256 {
                        None => Some(ReasonCode::DiffBodyEmpty),
                        Some(hash) if hash.is_empty() => Some(ReasonCode::DiffBodyEmpty),
                        Some(hash) if hash != &slice.sha256 => Some(ReasonCode::DiffBodyStale),
                        _ => None,
                    };

                    if let Some(reason) = needs_sync {
                        let notes_path = self
                            .plan_sink
                            .write_notes_file(&slice.sanitized_filename, &slice.body)?;
                        actions.push(ReleaseSyncPlanAction {
                            id: next_id(&mut action_index),
                            op: ActionOpcode::RelNotesSync.opcode_number(),
                            code: ActionOpcode::RelNotesSync,
                            reason,
                            r#ref: local.tag.clone(),
                            commit: None,
                            title: None,
                            notes_src: Some(notes_path),
                            flags: None,
                            src: None,
                            dest: None,
                        });
                    }
                }

                // Check release metadata / flags
                let flags_changed =
                    remote.is_draft != is_draft || remote.is_prerelease != is_prerelease;
                let title_changed = match (&remote.title, &local.title) {
                    (Some(r_title), Some(l_title)) => r_title != l_title,
                    (None, Some(_)) => true,
                    _ => false,
                };

                if flags_changed || title_changed {
                    actions.push(ReleaseSyncPlanAction {
                        id: next_id(&mut action_index),
                        op: ActionOpcode::RelMut.opcode_number(),
                        code: ActionOpcode::RelMut,
                        reason: ReasonCode::DiffChannelDesync,
                        r#ref: local.tag.clone(),
                        commit: None,
                        title: local.title.clone().or_else(|| Some(local.tag.clone())),
                        notes_src: None,
                        flags: Some(ReleaseFlags {
                            draft: Some(is_draft),
                            prerelease: Some(is_prerelease),
                        }),
                        src: None,
                        dest: None,
                    });
                }

                // Check distribution artifacts
                let mut remote_assets_by_name: HashMap<&str, &crate::models::RemoteAsset> =
                    HashMap::new();
                for asset in &remote.assets {
                    remote_assets_by_name.insert(&asset.name, asset);
                }

                for artifact in &local_artifacts {
                    if let Some(remote_asset) =
                        remote_assets_by_name.get(artifact.destination_name.as_str())
                    {
                        let size_mismatch = remote_asset.size_bytes != artifact.size_bytes;
                        let hash_mismatch = match (&remote_asset.sha256, &artifact.sha256) {
                            (Some(r_sha), Some(l_sha)) => r_sha != l_sha,
                            _ => false,
                        };

                        if size_mismatch || hash_mismatch {
                            actions.push(ReleaseSyncPlanAction {
                                id: next_id(&mut action_index),
                                op: ActionOpcode::ArtPut.opcode_number(),
                                code: ActionOpcode::ArtPut,
                                reason: ReasonCode::DiffChecksum,
                                r#ref: local.tag.clone(),
                                commit: None,
                                title: None,
                                notes_src: None,
                                flags: None,
                                src: Some(artifact.local_path.clone()),
                                dest: Some(artifact.destination_name.clone()),
                            });
                        }
                    } else {
                        // Asset absent on remote
                        actions.push(ReleaseSyncPlanAction {
                            id: next_id(&mut action_index),
                            op: ActionOpcode::ArtPut.opcode_number(),
                            code: ActionOpcode::ArtPut,
                            reason: ReasonCode::DiffAbsent,
                            r#ref: local.tag.clone(),
                            commit: None,
                            title: None,
                            notes_src: None,
                            flags: None,
                            src: Some(artifact.local_path.clone()),
                            dest: Some(artifact.destination_name.clone()),
                        });
                    }
                }

                // If nothing changed for this existing release, record SYNC_OK
                if actions.len() == actions_before {
                    actions.push(ReleaseSyncPlanAction {
                        id: next_id(&mut action_index),
                        op: ActionOpcode::SyncOk.opcode_number(),
                        code: ActionOpcode::SyncOk,
                        reason: ReasonCode::DiffAbsent,
                        r#ref: local.tag.clone(),
                        commit: None,
                        title: None,
                        notes_src: None,
                        flags: None,
                        src: None,
                        dest: None,
                    });
                }
            } else {
                // Target absent on remote -> REL_INIT
                let notes_path = if let Some(slice) = &note_slice {
                    Some(
                        self.plan_sink
                            .write_notes_file(&slice.sanitized_filename, &slice.body)?,
                    )
                } else {
                    None
                };

                actions.push(ReleaseSyncPlanAction {
                    id: next_id(&mut action_index),
                    op: ActionOpcode::RelInit.opcode_number(),
                    code: ActionOpcode::RelInit,
                    reason: ReasonCode::DiffAbsent,
                    r#ref: local.tag.clone(),
                    commit: Some(local.commit.clone()),
                    title: local.title.clone().or_else(|| Some(local.tag.clone())),
                    notes_src: notes_path,
                    flags: Some(ReleaseFlags {
                        draft: Some(is_draft),
                        prerelease: Some(is_prerelease),
                    }),
                    src: None,
                    dest: None,
                });

                // Attach all local artifacts
                for artifact in &local_artifacts {
                    actions.push(ReleaseSyncPlanAction {
                        id: next_id(&mut action_index),
                        op: ActionOpcode::ArtPut.opcode_number(),
                        code: ActionOpcode::ArtPut,
                        reason: ReasonCode::DiffAbsent,
                        r#ref: local.tag.clone(),
                        commit: None,
                        title: None,
                        notes_src: None,
                        flags: None,
                        src: Some(artifact.local_path.clone()),
                        dest: Some(artifact.destination_name.clone()),
                    });
                }
            }
        }

        // 2. Check for remote entities that should be pruned
        for release in &remote_state.releases {
            if !processed_remote_tags.contains(release.tag.as_str())
                && self.policy_evaluator.should_prune_remote(release)
            {
                actions.push(ReleaseSyncPlanAction {
                    id: next_id(&mut action_index),
                    op: ActionOpcode::RelPrune.opcode_number(),
                    code: ActionOpcode::RelPrune,
                    reason: ReasonCode::DiffPrunePolicy,
                    r#ref: release.tag.clone(),
                    commit: None,
                    title: None,
                    notes_src: None,
                    flags: None,
                    src: None,
                    dest: None,
                });
            }
        }

        let delta_count = actions.iter().filter(|a| a.code.is_mutating()).count();

        let plan = ReleaseSyncPlan {
            schema: Some(crate::models::SCHEMA_RELEASE_PLAN_V1.to_string()),
            v: 1,
            provider: remote_state.provider.clone(),
            status: 200,
            reconciled: true,
            delta_count,
            plan: actions,
        };

        self.plan_sink.write_plan(&plan)?;
        Ok(plan)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{RemoteAsset, RemoteRelease, RemoteReleaseState};
    use crate::traits::{LocalArtifactInfo, LocalTagInfo, NoteSlice};
    use std::cell::RefCell;

    struct MockRemoteState(RemoteReleaseState);
    impl RemoteStateSource for MockRemoteState {
        fn load_remote_state(&self) -> Result<RemoteReleaseState, ProtocolError> {
            Ok(self.0.clone())
        }
    }

    struct MockNotes(HashMap<String, NoteSlice>);
    impl NotesExtractor for MockNotes {
        fn extract_notes(&self, tag: &str) -> Result<Option<NoteSlice>, ProtocolError> {
            Ok(self.0.get(tag).cloned())
        }
    }

    struct MockGit(Vec<LocalTagInfo>);
    impl GitStateProvider for MockGit {
        fn local_tags(&self) -> Result<Vec<LocalTagInfo>, ProtocolError> {
            Ok(self.0.clone())
        }
    }

    struct MockArtifacts(HashMap<String, Vec<LocalArtifactInfo>>);
    impl ArtifactInspector for MockArtifacts {
        fn local_artifacts(&self, tag: &str) -> Result<Vec<LocalArtifactInfo>, ProtocolError> {
            Ok(self.0.get(tag).cloned().unwrap_or_default())
        }
    }

    struct MockPolicy {
        prereleases: HashSet<String>,
        drafts: HashSet<String>,
        prune_orphans: bool,
    }
    impl PolicyEvaluator for MockPolicy {
        fn is_prerelease(&self, tag: &str) -> bool {
            self.prereleases.contains(tag)
        }
        fn is_draft(&self, tag: &str) -> bool {
            self.drafts.contains(tag)
        }
        fn should_prune_remote(&self, release: &RemoteRelease) -> bool {
            self.prune_orphans && release.is_draft
        }
    }

    struct MockSink {
        notes: RefCell<HashMap<String, String>>,
        plan: RefCell<Option<ReleaseSyncPlan>>,
    }
    impl PlanSink for MockSink {
        fn write_notes_file(
            &mut self,
            filename: &str,
            content: &str,
        ) -> Result<String, ProtocolError> {
            let path = format!(".evin/actions/release-sync/notes/{filename}");
            self.notes
                .borrow_mut()
                .insert(path.clone(), content.to_string());
            Ok(path)
        }
        fn write_plan(&mut self, plan: &ReleaseSyncPlan) -> Result<(), ProtocolError> {
            *self.plan.borrow_mut() = Some(plan.clone());
            Ok(())
        }
    }

    #[test]
    fn reconciles_absent_release_and_artifacts() {
        let state = MockRemoteState(RemoteReleaseState {
            schema: None,
            v: 1,
            provider: "github".into(),
            repository: "owner/repo".into(),
            releases: vec![],
        });

        let mut notes_map = HashMap::new();
        notes_map.insert(
            "v1.0.0-canary.1".into(),
            NoteSlice {
                body: "Initial canary release".into(),
                sha256: "abc123hash".into(),
                sanitized_filename: "v1.0.0-canary.1.md".into(),
            },
        );
        let notes = MockNotes(notes_map);

        let git = MockGit(vec![LocalTagInfo {
            tag: "v1.0.0-canary.1".into(),
            commit: "112233445566".into(),
            title: Some("v1.0.0-canary.1".into()),
            is_annotated: true,
        }]);

        let mut art_map = HashMap::new();
        art_map.insert(
            "v1.0.0-canary.1".into(),
            vec![LocalArtifactInfo {
                local_path: "dist/bin/evin-linux.tar.gz".into(),
                destination_name: "evin-v1.0.0-canary.1-linux.tar.gz".into(),
                size_bytes: 1024,
                sha256: Some("sha-tar".into()),
            }],
        );
        let artifacts = MockArtifacts(art_map);

        let mut prereleases = HashSet::new();
        prereleases.insert("v1.0.0-canary.1".into());
        let policy = MockPolicy {
            prereleases,
            drafts: HashSet::new(),
            prune_orphans: false,
        };

        let mut sink = MockSink {
            notes: RefCell::new(HashMap::new()),
            plan: RefCell::new(None),
        };

        let mut reconciler = Reconciler::new(&state, &notes, &git, &artifacts, &policy, &mut sink);
        let plan = reconciler.reconcile().unwrap();

        assert_eq!(plan.status, 200);
        assert!(plan.reconciled);
        assert_eq!(plan.delta_count, 2);
        assert_eq!(plan.plan.len(), 2);

        assert_eq!(plan.plan[0].code, ActionOpcode::RelInit);
        assert_eq!(plan.plan[0].op, 201);
        assert_eq!(plan.plan[0].reason, ReasonCode::DiffAbsent);
        assert_eq!(
            plan.plan[0].notes_src.as_deref(),
            Some(".evin/actions/release-sync/notes/v1.0.0-canary.1.md")
        );
        assert_eq!(
            plan.plan[0].flags,
            Some(ReleaseFlags {
                draft: Some(false),
                prerelease: Some(true)
            })
        );

        assert_eq!(plan.plan[1].code, ActionOpcode::ArtPut);
        assert_eq!(plan.plan[1].op, 301);
        assert_eq!(plan.plan[1].reason, ReasonCode::DiffAbsent);
    }

    #[test]
    fn reconciles_stale_notes_and_target_drift() {
        let state = MockRemoteState(RemoteReleaseState {
            schema: None,
            v: 1,
            provider: "github".into(),
            repository: "owner/repo".into(),
            releases: vec![RemoteRelease {
                tag: "v1.0.0".into(),
                commit: Some("old_commit_sha".into()),
                title: Some("v1.0.0".into()),
                is_draft: false,
                is_prerelease: false,
                body_sha256: Some("old_sha256".into()),
                published_at: Some("2026-08-25T12:00:00Z".into()),
                assets: vec![],
            }],
        });

        let mut notes_map = HashMap::new();
        notes_map.insert(
            "v1.0.0".into(),
            NoteSlice {
                body: "Updated release notes".into(),
                sha256: "new_sha256".into(),
                sanitized_filename: "v1.0.0.md".into(),
            },
        );
        let notes = MockNotes(notes_map);

        let git = MockGit(vec![LocalTagInfo {
            tag: "v1.0.0".into(),
            commit: "new_commit_sha".into(),
            title: Some("v1.0.0".into()),
            is_annotated: true,
        }]);

        let artifacts = MockArtifacts(HashMap::new());
        let policy = MockPolicy {
            prereleases: HashSet::new(),
            drafts: HashSet::new(),
            prune_orphans: false,
        };

        let mut sink = MockSink {
            notes: RefCell::new(HashMap::new()),
            plan: RefCell::new(None),
        };

        let mut reconciler = Reconciler::new(&state, &notes, &git, &artifacts, &policy, &mut sink);
        let plan = reconciler.reconcile().unwrap();

        assert_eq!(plan.delta_count, 2);
        assert_eq!(plan.plan[0].code, ActionOpcode::RelRetarget);
        assert_eq!(plan.plan[0].reason, ReasonCode::DiffTargetDrift);
        assert_eq!(plan.plan[0].commit.as_deref(), Some("new_commit_sha"));

        assert_eq!(plan.plan[1].code, ActionOpcode::RelNotesSync);
        assert_eq!(plan.plan[1].reason, ReasonCode::DiffBodyStale);
    }

    #[test]
    fn reconciles_synced_release_as_sync_ok() {
        let state = MockRemoteState(RemoteReleaseState {
            schema: None,
            v: 1,
            provider: "github".into(),
            repository: "owner/repo".into(),
            releases: vec![RemoteRelease {
                tag: "v1.0.0".into(),
                commit: Some("same_commit".into()),
                title: Some("v1.0.0".into()),
                is_draft: false,
                is_prerelease: false,
                body_sha256: Some("exact_sha".into()),
                published_at: Some("2026-08-25T12:00:00Z".into()),
                assets: vec![RemoteAsset {
                    name: "asset.tar.gz".into(),
                    size_bytes: 500,
                    sha256: Some("asset_sha".into()),
                }],
            }],
        });

        let mut notes_map = HashMap::new();
        notes_map.insert(
            "v1.0.0".into(),
            NoteSlice {
                body: "Exact body".into(),
                sha256: "exact_sha".into(),
                sanitized_filename: "v1.0.0.md".into(),
            },
        );
        let notes = MockNotes(notes_map);

        let git = MockGit(vec![LocalTagInfo {
            tag: "v1.0.0".into(),
            commit: "same_commit".into(),
            title: Some("v1.0.0".into()),
            is_annotated: true,
        }]);

        let mut art_map = HashMap::new();
        art_map.insert(
            "v1.0.0".into(),
            vec![LocalArtifactInfo {
                local_path: "dist/asset.tar.gz".into(),
                destination_name: "asset.tar.gz".into(),
                size_bytes: 500,
                sha256: Some("asset_sha".into()),
            }],
        );
        let artifacts = MockArtifacts(art_map);

        let policy = MockPolicy {
            prereleases: HashSet::new(),
            drafts: HashSet::new(),
            prune_orphans: false,
        };

        let mut sink = MockSink {
            notes: RefCell::new(HashMap::new()),
            plan: RefCell::new(None),
        };

        let mut reconciler = Reconciler::new(&state, &notes, &git, &artifacts, &policy, &mut sink);
        let plan = reconciler.reconcile().unwrap();

        assert_eq!(plan.delta_count, 0);
        assert_eq!(plan.plan.len(), 1);
        assert_eq!(plan.plan[0].code, ActionOpcode::SyncOk);
    }

    #[test]
    fn reconciles_metadata_flags_and_channel_desync() {
        let state = MockRemoteState(RemoteReleaseState {
            schema: None,
            v: 1,
            provider: "github".into(),
            repository: "owner/repo".into(),
            releases: vec![RemoteRelease {
                tag: "v1.0.0-canary.1".into(),
                commit: Some("commit_1".into()),
                title: Some("v1.0.0-canary.1".into()),
                is_draft: true, // desync: remote says draft, local says not draft
                is_prerelease: false, // desync: remote says stable, local says prerelease
                body_sha256: Some("sha_body".into()),
                published_at: None,
                assets: vec![],
            }],
        });

        let mut notes_map = HashMap::new();
        notes_map.insert(
            "v1.0.0-canary.1".into(),
            NoteSlice {
                body: "Body".into(),
                sha256: "sha_body".into(),
                sanitized_filename: "v1.0.0-canary.1.md".into(),
            },
        );
        let notes = MockNotes(notes_map);

        let git = MockGit(vec![LocalTagInfo {
            tag: "v1.0.0-canary.1".into(),
            commit: "commit_1".into(),
            title: Some("v1.0.0-canary.1".into()),
            is_annotated: true,
        }]);

        let mut prereleases = HashSet::new();
        prereleases.insert("v1.0.0-canary.1".into());
        let policy = MockPolicy {
            prereleases,
            drafts: HashSet::new(),
            prune_orphans: false,
        };

        let mut sink = MockSink {
            notes: RefCell::new(HashMap::new()),
            plan: RefCell::new(None),
        };

        let artifacts = MockArtifacts(HashMap::new());
        let mut reconciler = Reconciler::new(&state, &notes, &git, &artifacts, &policy, &mut sink);
        let plan = reconciler.reconcile().unwrap();

        assert_eq!(plan.delta_count, 1);
        assert_eq!(plan.plan[0].code, ActionOpcode::RelMut);
        assert_eq!(plan.plan[0].reason, ReasonCode::DiffChannelDesync);
        assert_eq!(
            plan.plan[0].flags,
            Some(ReleaseFlags {
                draft: Some(false),
                prerelease: Some(true)
            })
        );
    }

    #[test]
    fn reconciles_orphan_remote_release_prune() {
        let state = MockRemoteState(RemoteReleaseState {
            schema: None,
            v: 1,
            provider: "github".into(),
            repository: "owner/repo".into(),
            releases: vec![RemoteRelease {
                tag: "v0.0.0-orphaned-draft".into(),
                commit: Some("deadbeef".into()),
                title: Some("Orphan".into()),
                is_draft: true,
                is_prerelease: true,
                body_sha256: None,
                published_at: None,
                assets: vec![],
            }],
        });

        let notes = MockNotes(HashMap::new());
        let git = MockGit(vec![]); // No local tags
        let policy = MockPolicy {
            prereleases: HashSet::new(),
            drafts: HashSet::new(),
            prune_orphans: true,
        };

        let mut sink = MockSink {
            notes: RefCell::new(HashMap::new()),
            plan: RefCell::new(None),
        };

        let artifacts = MockArtifacts(HashMap::new());
        let mut reconciler = Reconciler::new(&state, &notes, &git, &artifacts, &policy, &mut sink);
        let plan = reconciler.reconcile().unwrap();

        assert_eq!(plan.delta_count, 1);
        assert_eq!(plan.plan[0].code, ActionOpcode::RelPrune);
        assert_eq!(plan.plan[0].reason, ReasonCode::DiffPrunePolicy);
        assert_eq!(plan.plan[0].r#ref, "v0.0.0-orphaned-draft");
    }

    #[test]
    fn test_schema_json_roundtrip_spec_conformance() {
        let raw_spec_remote = r#"{
          "$schema": "https://everyinit.dev/schemas/v1/release-sync-remote-state.json",
          "v": 1,
          "provider": "github",
          "repository": "dukasn/everyinit",
          "releases": [
            {
              "tag": "v1.0.0-canary.5",
              "commit": "8f3b129a34bcde12",
              "title": "v1.0.0-canary.5",
              "is_draft": false,
              "is_prerelease": true,
              "body_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
              "published_at": "2026-08-25T14:30:00Z",
              "assets": [
                {
                  "name": "evin-linux-x86_64.tar.gz",
                  "size_bytes": 14280900
                }
              ]
            }
          ]
        }"#;

        let parsed: RemoteReleaseState = serde_json::from_str(raw_spec_remote).unwrap();
        assert_eq!(parsed.v, 1);
        assert_eq!(parsed.provider, "github");
        assert_eq!(parsed.releases.len(), 1);
        assert_eq!(parsed.releases[0].tag, "v1.0.0-canary.5");
        assert_eq!(parsed.releases[0].assets.len(), 1);
        assert_eq!(
            parsed.releases[0].assets[0].name,
            "evin-linux-x86_64.tar.gz"
        );

        let raw_spec_plan = r#"{
          "$schema": "https://everyinit.dev/schemas/v1/release-sync-plan.json",
          "v": 1,
          "provider": "github",
          "status": 200,
          "reconciled": true,
          "delta_count": 3,
          "plan": [
            {
              "id": "act-001",
              "op": 201,
              "code": "REL_INIT",
              "reason": "DIFF_ABSENT",
              "ref": "v1.0.0-canary.6",
              "commit": "a1b2c3d4e5f67890",
              "title": "v1.0.0-canary.6",
              "notes_src": ".evin/actions/release-sync/notes/v1.0.0-canary.6.md",
              "flags": {
                "draft": false,
                "prerelease": true
              }
            }
          ]
        }"#;

        let parsed_plan: ReleaseSyncPlan = serde_json::from_str(raw_spec_plan).unwrap();
        assert_eq!(parsed_plan.v, 1);
        assert_eq!(parsed_plan.delta_count, 3);
        assert_eq!(parsed_plan.plan[0].code, ActionOpcode::RelInit);
        assert_eq!(parsed_plan.plan[0].reason, ReasonCode::DiffAbsent);
    }
}
