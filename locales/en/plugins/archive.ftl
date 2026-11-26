# Archive plugin locale messages

invalid-config = Invalid Archive configuration: { $error }
no-files-matched = No input files matched the configured sources.
unsupported-format = Format '{ $format }' is not supported.
safe-path-resolution-failed-path = Safe path resolution failed for archive path: { $error }
create-output-dir-failed = Failed to create output directory: { $error }
invalid-glob = Invalid glob pattern: { $error }
invalid-file = Invalid file: { $path }
safe-path-resolution-failed-source = Safe path resolution failed for source: { $error }
glob-error = Glob error: { $error }
create-output-file-failed = Failed to create output file: { $error }
archive-failed = Failed to archive { $path }: { $error }
finalize-archive-failed = Failed to finalize tar.gz archive: { $error }
path-collision = Archive collision: '{ $destination }' is targeted by both '{ $source1 }' and '{ $source2 }'
relative-path-escape = File '{ $path }' is outside the task directory, which is incompatible with 'layout: relative'. Use 'layout: flat' or restrict includes to the task directory.
archive-path-must-not-be-empty = path must not be empty
include-must-contain-at-least-one-pattern = include must contain at least one pattern
include-patterns-must-not-be-empty = include patterns must not be empty
exclude-patterns-must-not-be-empty = exclude patterns must not be empty
prefix-must-not-be-empty-when-provided = prefix must not be empty when provided
compression-level-invalid = compression_level must be between 1 and 9

invalid-config-title = Invalid Archive Configuration
no-files-matched-title = No Files Matched
unsupported-format-title = Unsupported Format
create-output-dir-failed-title = Archive Failed
create-output-file-failed-title = Archive Failed
archive-failed-title = Archive Failed
finalize-archive-failed-title = Archive Failed
path-collision-title = Path Collision
relative-path-escape-title = Path Escape
archive-path-must-not-be-empty-title = Invalid Archive Configuration
invalid-glob-title = Invalid Glob
compression-level-invalid-title = Invalid Configuration
