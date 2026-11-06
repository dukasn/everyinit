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

//! Editor-facing shape of GitRules. Keep this beside the serde models and
//! cover its field names against serialization in tests.

use evin_flow::schema::{Field, Schema, ValueConstraint};

const STRING: Schema = Schema::String;
const STRING_LIST: Schema = Schema::Sequence(&STRING);
const FORMAT: Schema = Schema::Union(&[Schema::String, Schema::Sequence(&STRING)]);
const BUMP: Schema = Schema::Enum(&["major", "minor", "patch", "none"]);

const CONSTRAINT: Schema = Schema::Object(&[
    Field::new(
        "must_be",
        Schema::String,
        "flow-schema-named-format-required-for-the-captured-value",
    ),
    Field::new(
        "allowed_values",
        STRING_LIST,
        "flow-schema-allowed-captured-values",
    ),
    Field::new(
        "pattern",
        Schema::String,
        "flow-schema-regular-expression-for-the-captured-value",
    )
    .constraints(&[ValueConstraint::Regex]),
    Field::new(
        "max_length",
        Schema::Integer,
        "flow-schema-maximum-captured-value-length",
    ),
]);
const CONSTRAINTS: Schema = Schema::Map(&CONSTRAINT);
const COMMIT_POLICIES: Schema = Schema::Object(&[
    Field::new(
        "require_body",
        Schema::Boolean,
        "flow-schema-require-a-commit-body",
    ),
    Field::new(
        "max_body_line_length",
        Schema::Integer,
        "flow-schema-maximum-commit-body-line-length",
    ),
    Field::new(
        "allow_empty_commits",
        Schema::Boolean,
        "flow-schema-allow-commits-without-changes",
    ),
    Field::new(
        "require_ticket_reference",
        Schema::Boolean,
        "flow-schema-require-a-ticket-reference",
    ),
]);
const COMMITS: Schema = Schema::Object(&[
    Field::new(
        "message_format",
        FORMAT,
        "flow-schema-commit-message-template-or-alternative-templates-placeholders-capture-fields-optional-groups-and-patterns-follow-the-gitrules-template-syntax",
    )
    .constraints(&[ValueConstraint::GitTemplate]),
    Field::new(
        "ignore_patterns",
        STRING_LIST,
        "flow-schema-regular-expressions-for-commits-excluded-from-checking",
    )
    .non_nullable()
    .constraints(&[ValueConstraint::Regex]),
    Field::new(
        "fields",
        CONSTRAINTS,
        "flow-schema-constraints-for-named-template-capture-groups",
    ).non_nullable(),
    Field::new("policies", COMMIT_POLICIES, "flow-schema-commit-validation-policies"),
]);
const BRANCH_POLICIES: Schema = Schema::Object(&[Field::new(
    "block_push_to_main",
    Schema::Boolean,
    "flow-schema-block-direct-pushes-to-the-default-branch",
)]);
const BRANCHES: Schema = Schema::Object(&[
    Field::new(
        "name_format",
        FORMAT,
        "flow-schema-branch-name-template-or-alternative-templates",
    )
    .constraints(&[ValueConstraint::GitTemplate]),
    Field::new(
        "fields",
        CONSTRAINTS,
        "flow-schema-constraints-for-named-branch-capture-groups",
    )
    .non_nullable(),
    Field::new(
        "policies",
        BRANCH_POLICIES,
        "flow-schema-branch-protection-policies",
    ),
]);
const CHANGE_TYPE: Schema = Schema::Object(&[
    Field::new(
        "bump",
        BUMP,
        "flow-schema-version-bump-for-this-commit-type",
    )
    .required(),
    Field::new(
        "changelog_section",
        Schema::String,
        "flow-schema-changelog-heading-for-this-commit-type",
    ),
]);
const CHANGE_TYPES: Schema = Schema::Map(&CHANGE_TYPE);
const VERSIONING: Schema = Schema::Object(&[Field::new(
    "strategy",
    Schema::Enum(&["semver", "cargo"]),
    "flow-schema-versioning-strategy",
)
.non_nullable()]);
const TICKET_LINKING: Schema = Schema::Object(&[
    Field::new(
        "prefix",
        Schema::String,
        "flow-schema-issue-identifier-prefix",
    ),
    Field::new(
        "url_template",
        Schema::String,
        "flow-schema-url-template-for-linked-issues",
    ),
]);
const CHANGELOG: Schema = Schema::Object(&[
    Field::new(
        "ticket_linking",
        TICKET_LINKING,
        "flow-schema-issue-link-configuration",
    ),
    Field::new(
        "host_integration",
        Schema::String,
        "flow-schema-hosting-service-integration",
    ),
]);
const RELEASE_POLICIES: Schema = Schema::Object(&[
    Field::new(
        "breaking_change_bump",
        BUMP,
        "flow-schema-version-bump-for-breaking-changes",
    ),
    Field::new(
        "pre_release_prefix",
        Schema::String,
        "flow-schema-prerelease-label-prefix",
    ),
]);
const CHANNELS: Schema = Schema::Map(&STRING);
const PRERELEASE: Schema = Schema::Object(&[
    Field::new(
        "start_at_one",
        Schema::Boolean,
        "flow-schema-start-prerelease-counters-at-one",
    ),
    Field::new(
        "channels",
        CHANNELS,
        "flow-schema-named-prerelease-format-templates",
    )
    .non_nullable(),
]);
const REVISION: Schema = Schema::Object(&[Field::new(
    "scope",
    Schema::Enum(&["package", "target"]),
    "flow-schema-scope-of-the-package-revision-counter",
)
.non_nullable()]);
const PACKAGE: Schema = Schema::Object(&[
    Field::new(
        "watch",
        STRING_LIST,
        "flow-schema-paths-that-trigger-a-package-release",
    )
    .required(),
    Field::new(
        "targets",
        STRING_LIST,
        "flow-schema-distribution-targets-for-the-package",
    )
    .required(),
    Field::new("revision", REVISION, "flow-schema-package-revision-policy").non_nullable(),
]);
const PACKAGES: Schema = Schema::Map(&PACKAGE);
const RELEASES: Schema = Schema::Object(&[
    Field::new("versioning", VERSIONING, "flow-schema-release-versioning").non_nullable(),
    Field::new(
        "change_types",
        CHANGE_TYPES,
        "flow-schema-version-and-changelog-rules-by-commit-type",
    )
    .non_nullable(),
    Field::new(
        "changelog",
        CHANGELOG,
        "flow-schema-changelog-formatting-and-links",
    ),
    Field::new("policies", RELEASE_POLICIES, "flow-schema-release-policies"),
    Field::new(
        "prerelease",
        PRERELEASE,
        "flow-schema-prerelease-channels-and-counters",
    ),
    Field::new(
        "commit_message",
        Schema::String,
        "flow-schema-release-commit-message-template",
    ),
    Field::new(
        "packages",
        PACKAGES,
        "flow-schema-release-packages-keyed-by-package-id",
    )
    .non_nullable(),
]);
const AUTHORSHIP_POLICIES: Schema = Schema::Object(&[
    Field::new(
        "require_signatures",
        Schema::Boolean,
        "flow-schema-require-signed-commits",
    ),
    Field::new(
        "block_identity_forging",
        Schema::Boolean,
        "flow-schema-reject-forged-author-identities",
    ),
]);
const AUTHORSHIP: Schema = Schema::Object(&[
    Field::new(
        "allowed_emails",
        STRING_LIST,
        "flow-schema-allowed-author-email-patterns",
    )
    .non_nullable(),
    Field::new(
        "policies",
        AUTHORSHIP_POLICIES,
        "flow-schema-author-identity-policies",
    ),
]);
const CONTENT_BLOCK: Schema = Schema::Object(&[
    Field::new(
        "pattern",
        Schema::String,
        "flow-schema-content-pattern-to-block",
    )
    .required()
    .constraints(&[ValueConstraint::Regex]),
    Field::new(
        "error_message",
        Schema::String,
        "flow-schema-message-for-a-matching-block",
    )
    .required(),
]);
const HISTORY_POLICIES: Schema = Schema::Object(&[
    Field::new(
        "require_linear_history",
        Schema::Boolean,
        "flow-schema-require-linear-git-history",
    ),
    Field::new(
        "max_staged_file_size",
        Schema::String,
        "flow-schema-maximum-size-of-a-staged-file",
    ),
]);
const HISTORY: Schema = Schema::Object(&[
    Field::new(
        "content_blocks",
        Schema::Sequence(&CONTENT_BLOCK),
        "flow-schema-patterns-blocked-in-history",
    )
    .non_nullable(),
    Field::new(
        "policies",
        HISTORY_POLICIES,
        "flow-schema-history-integrity-policies",
    ),
]);
const TAG_POLICIES: Schema = Schema::Object(&[
    Field::new(
        "require_annotated",
        Schema::Boolean,
        "flow-schema-require-annotated-tags",
    ),
    Field::new(
        "block_deletion",
        Schema::Boolean,
        "flow-schema-block-tag-deletion",
    ),
]);
const TAGS: Schema = Schema::Object(&[
    Field::new(
        "name_format",
        FORMAT,
        "flow-schema-tag-name-template-or-alternative-templates",
    )
    .constraints(&[ValueConstraint::GitTemplate]),
    Field::new(
        "policies",
        TAG_POLICIES,
        "flow-schema-tag-lifecycle-policies",
    ),
]);
const PULL_REQUEST_POLICIES: Schema = Schema::Object(&[
    Field::new(
        "require_assignee",
        Schema::Boolean,
        "flow-schema-require-an-assignee",
    ),
    Field::new(
        "require_linked_issue",
        Schema::Boolean,
        "flow-schema-require-a-linked-issue",
    ),
    Field::new(
        "block_wip_prefixes",
        STRING_LIST,
        "flow-schema-title-prefixes-that-block-merge",
    )
    .non_nullable(),
]);
const PULL_REQUESTS: Schema = Schema::Object(&[
    Field::new(
        "title_format",
        FORMAT,
        "flow-schema-pull-request-title-template-or-alternatives",
    )
    .constraints(&[ValueConstraint::GitTemplate]),
    Field::new(
        "policies",
        PULL_REQUEST_POLICIES,
        "flow-schema-pull-request-governance-policies",
    ),
]);
const CONTENT_RULE: Schema = Schema::Object(&[
    Field::new("name", Schema::String, "flow-schema-content-rule-name").required(),
    Field::new("include", STRING_LIST, "flow-schema-included-file-patterns").non_nullable(),
    Field::new("exclude", STRING_LIST, "flow-schema-excluded-file-patterns").non_nullable(),
    Field::new(
        "require_pattern",
        Schema::String,
        "flow-schema-required-content-pattern",
    )
    .constraints(&[ValueConstraint::Regex]),
    Field::new(
        "forbid_pattern",
        Schema::String,
        "flow-schema-forbidden-content-pattern",
    )
    .constraints(&[ValueConstraint::Regex]),
    Field::new(
        "error_message",
        Schema::String,
        "flow-schema-message-for-a-rule-violation",
    )
    .required(),
]);
const FILE_POLICIES: Schema = Schema::Object(&[Field::new(
    "block_paths",
    STRING_LIST,
    "flow-schema-file-paths-blocked-from-commits",
)
.non_nullable()]);
const FILES: Schema = Schema::Object(&[
    Field::new(
        "naming_convention",
        Schema::String,
        "flow-schema-file-naming-convention",
    ),
    Field::new("ignore", STRING_LIST, "flow-schema-ignored-paths").non_nullable(),
    Field::new(
        "content_rules",
        Schema::Sequence(&CONTENT_RULE),
        "flow-schema-file-content-rules",
    )
    .non_nullable(),
    Field::new("policies", FILE_POLICIES, "flow-schema-file-policies"),
]);
const LFS_POLICIES: Schema = Schema::Object(&[
    Field::new(
        "require_tracking_for",
        STRING_LIST,
        "flow-schema-extensions-that-require-git-lfs",
    )
    .non_nullable(),
    Field::new(
        "require_lfs_installed",
        Schema::Boolean,
        "flow-schema-require-git-lfs-locally",
    ),
]);
const LFS: Schema = Schema::Object(&[Field::new(
    "policies",
    LFS_POLICIES,
    "flow-schema-git-lfs-policies",
)]);
const SUBMODULE_POLICIES: Schema = Schema::Object(&[
    Field::new(
        "allow_creation",
        Schema::Boolean,
        "flow-schema-allow-new-submodules",
    ),
    Field::new(
        "allowed_hosts",
        STRING_LIST,
        "flow-schema-allowed-submodule-hosts",
    )
    .non_nullable(),
]);
const SUBMODULES: Schema = Schema::Object(&[Field::new(
    "policies",
    SUBMODULE_POLICIES,
    "flow-schema-submodule-policies",
)]);
const REMOTE_POLICIES: Schema = Schema::Object(&[
    Field::new(
        "allowed_push_remotes",
        STRING_LIST,
        "flow-schema-allowed-push-destinations",
    )
    .non_nullable(),
    Field::new(
        "block_force_push",
        STRING_LIST,
        "flow-schema-branches-protected-from-force-pushes",
    )
    .non_nullable(),
    Field::new(
        "require_up_to_date",
        Schema::Boolean,
        "flow-schema-require-an-up-to-date-branch-before-pushing",
    ),
]);
const REMOTES: Schema = Schema::Object(&[Field::new(
    "policies",
    REMOTE_POLICIES,
    "flow-schema-remote-push-policies",
)]);

pub const GITRULES_SCHEMA: Schema = Schema::Object(&[
    Field::new(
        "version",
        Schema::Integer,
        "flow-schema-gitrules-specification-version",
    )
    .non_nullable(),
    Field::new(
        "name",
        Schema::String,
        "flow-schema-repository-rule-set-name",
    )
    .non_nullable(),
    Field::new(
        "description",
        Schema::String,
        "flow-schema-repository-rule-set-description",
    ),
    Field::new(
        "commits",
        COMMITS,
        "flow-schema-commit-message-and-validation-rules",
    )
    .catalog("GitRules.commits"),
    Field::new(
        "branches",
        BRANCHES,
        "flow-schema-branch-naming-and-protection-rules",
    )
    .catalog("GitRules.branches"),
    Field::new(
        "releases",
        RELEASES,
        "flow-schema-release-and-package-policies",
    )
    .catalog("GitRules.releases"),
    Field::new(
        "authorship",
        AUTHORSHIP,
        "flow-schema-author-identity-policies",
    )
    .catalog("GitRules.authorship"),
    Field::new("history", HISTORY, "flow-schema-history-integrity-policies")
        .catalog("GitRules.history"),
    Field::new(
        "tags",
        TAGS,
        "flow-schema-tag-naming-and-lifecycle-policies",
    )
    .catalog("GitRules.tags"),
    Field::new(
        "pull_requests",
        PULL_REQUESTS,
        "flow-schema-pull-request-governance-policies",
    )
    .catalog("GitRules.pull_requests"),
    Field::new(
        "files",
        FILES,
        "flow-schema-file-naming-and-content-policies",
    )
    .catalog("GitRules.files"),
    Field::new("lfs", LFS, "flow-schema-git-lfs-policies").catalog("GitRules.lfs"),
    Field::new("submodules", SUBMODULES, "flow-schema-submodule-policies")
        .catalog("GitRules.submodules"),
    Field::new("remotes", REMOTES, "flow-schema-remote-push-policies").catalog("GitRules.remotes"),
]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::GitRulesRoot;
    use serde_json::{Value, json};

    fn example(schema: Schema) -> Value {
        match schema {
            Schema::Object(fields) => Value::Object(
                fields
                    .iter()
                    .map(|field| (field.name.into(), example(field.schema)))
                    .collect(),
            ),
            Schema::Map(item) => json!({ "sample": example(*item) }),
            Schema::Sequence(item) => json!([example(*item)]),
            Schema::Union(variants) => example(variants[0]),
            Schema::String => json!("sample"),
            Schema::Integer => json!(1),
            Schema::Number => json!(1.0),
            Schema::Boolean => json!(true),
            Schema::Enum(values) => json!(values[0]),
            Schema::Any => Value::Null,
        }
    }

    fn assert_shape(schema: Schema, value: &Value) {
        match schema {
            Schema::Object(fields) => {
                let object = value.as_object().unwrap();
                let declared = fields
                    .iter()
                    .map(|field| field.name)
                    .collect::<std::collections::BTreeSet<_>>();
                let serialized = object
                    .keys()
                    .map(String::as_str)
                    .collect::<std::collections::BTreeSet<_>>();
                assert_eq!(declared, serialized);
                for field in fields {
                    assert_shape(field.schema, &object[field.name]);
                }
            }
            Schema::Map(item) => {
                for value in value.as_object().unwrap().values() {
                    assert_shape(*item, value);
                }
            }
            Schema::Sequence(item) => {
                for value in value.as_array().unwrap() {
                    assert_shape(*item, value);
                }
            }
            Schema::Union(variants) => assert_shape(variants[0], value),
            _ => {}
        }
    }

    #[test]
    fn schema_field_names_and_types_match_serde_models() {
        let source = example(GITRULES_SCHEMA);
        let typed: GitRulesRoot = serde_json::from_value(source).unwrap();
        let serialized = serde_json::to_value(typed).unwrap();
        assert_shape(GITRULES_SCHEMA, &serialized);
    }
}
