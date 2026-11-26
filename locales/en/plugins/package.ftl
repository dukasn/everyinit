# Format-neutral messages shared by native package action crates.

package-name-invalid = package name contains unsupported characters
package-metadata-invalid = package { $field } must be non-empty and contain no line breaks
package-payload-required = package.payload must contain at least one source mapping
package-relation-invalid = package { $field } entries must be non-empty single-line strings
package-relation-duplicate = package { $field } contains '{ $value }' more than once
package-script-path-empty = package script path must not be empty
package-script-read-failed = failed to read package script: { $error }
package-release-reference-format-invalid = release must be exactly '${task.release}'
package-release-reference-missing-need = release references '{ $task }', but that task is not declared in needs
package-output-and-release-required = package output directory and release reference must not be empty
package-invalid-config-title = Invalid Native Package Configuration
package-release-output-invalid = invalid release output: { $error }
package-release-schema-unsupported = unsupported release schema { $schema }
package-release-revision-missing = release has no resolved '{ $target }' revision for package '{ $package }'
package-release-version-revision-invalid = release contains an invalid version or revision

package-path-invalid = invalid package path '{ $path }'
package-path-collision = package path collision at '{ $path }'
package-input-read-failed = could not read package input '{ $path }': { $error }
package-source-date-epoch-invalid = invalid SOURCE_DATE_EPOCH value '{ $value }'
package-archive-write-failed = could not write package archive: { $error }
package-path-invalid-title = Invalid Package Path
package-path-collision-title = Package Path Collision
package-input-read-failed-title = Package Input Read Failed
package-source-date-epoch-invalid-title = Invalid Build Timestamp
package-archive-write-failed-title = Package Archive Write Failed

artifact-dry-run-inputs-unverified = Artifact inputs could not be verified: { $includes }
artifact-source-date-epoch-invalid = Invalid SOURCE_DATE_EPOCH: { $error }
artifact-source-date-epoch-unavailable = SOURCE_DATE_EPOCH is absent and Git HEAD timestamp is unavailable: { $error }
artifact-blocking-task-failed = Blocking artifact task failed: { $error }
artifact-output-size-unavailable = Could not determine the byte size of '{ $path }': { $error }
