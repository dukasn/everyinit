# Release sync action messages

release-sync-tag-must-not-be-empty = tag must not be empty when provided
release-sync-title-must-not-be-empty = title must not be empty when provided
release-sync-state-path-must-not-be-empty = state file path must not be empty
release-sync-plan-out-path-must-not-be-empty = plan_out path must not be empty
release-sync-notes-dir-must-not-be-empty = notes_dir path must not be empty
release-sync-invalid-config-title = Invalid Release Sync Configuration
release-sync-schema-version-unsupported = Remote state schema version { $version } is not supported; expected version 1
release-sync-schema-version-unsupported-title = Unsupported Remote State Version
release-sync-state-file-not-found = Remote state file not found at '{ $path }'
release-sync-state-file-read-failed = Failed to read state file '{ $path }': { $error }
release-sync-state-json-invalid = Invalid state JSON: { $error }
release-sync-artifacts-directory-read-failed = Failed to read artifacts dir '{ $path }': { $error }
release-sync-artifact-metadata-read-failed = Failed to read metadata for artifact '{ $path }': { $error }
release-sync-artifact-open-failed = Failed to open artifact '{ $path }': { $error }
release-sync-artifact-hash-failed = Failed to hash artifact '{ $path }': { $error }
release-sync-git-tag-command-failed = Failed to run git tag: { $error }
release-sync-git-tag-failed = git tag failed: { $error }
release-sync-io-failed = Release synchronization I/O failed: { $error }
release-sync-configured-tag-missing = Configured release tag '{ $tag }' does not exist locally
release-sync-notes-directory-create-failed = Failed to create notes dir '{ $path }': { $error }
release-sync-note-write-failed = Failed to write note file '{ $path }': { $error }
release-sync-plan-directory-create-failed = Failed to create plan dir '{ $path }': { $error }
release-sync-plan-serialization-failed = Failed to serialize plan JSON: { $error }
release-sync-plan-write-failed = Failed to write plan file '{ $path }': { $error }
release-sync-io-failed-title = Release Synchronization I/O Failed
