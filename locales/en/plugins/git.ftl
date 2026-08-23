# Git plugin locale messages

identity-not-configured = Git identity is not configured (user.name / user.email missing)
tag-already-exists = Tag '{ $tag }' already exists. This usually indicates that no new releasable commits were found since the last release, or the tag was manually created.
invalid-tag-name = Invalid tag name '{ $tag }'
no-staged-changes = No staged changes to commit
tag-recovery-failed = Commit '{ $commit_id }' succeeded, but tagging '{ $tag }' failed: { $error }. The tag may be retried after correcting the problem.
calculated-tag-already-exists = The version bumper calculated a new version '{ $version }', but the tag '{ $tag }' already exists in the repository. This usually happens if the tag is orphaned or points to a commit outside the current branch history.
message-must-not-be-empty = message must not be empty
tag-must-not-be-empty-when-provided = tag must not be empty when provided

identity-not-configured-title = Git Identity Error
tag-already-exists-title = Tag Exists
invalid-tag-name-title = Invalid Tag
no-staged-changes-title = Nothing to Commit
tag-recovery-failed-title = Tag Recovery Failed
calculated-tag-already-exists-title = Tag Conflict
