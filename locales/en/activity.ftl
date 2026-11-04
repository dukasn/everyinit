# Typed action activity, status, and dry-run messages.
# Keys and variables in this fallback catalog are part of the action SDK protocol.

activity-warning-label = Warning
activity-error-label = Error

activity-docs-scan-started = Scanning documentation files in '{ $source }'...
activity-docs-source-missing = Documentation source '{ $source }' does not exist; generating an empty site
activity-docs-read-failed = Could not read '{ $source }': { $error }
activity-docs-audit-started = Auditing internal links and anchor IDs...
activity-docs-link-issue = { $issue }
activity-docs-css-fallback = CSS minification failed; using the unminified stylesheet: { $error }
activity-docs-generated = Static site generated successfully at '{ $output }'
activity-docs-ready = Documentation site generated
activity-docs-changed = { $path }
activity-docs-rebuild-succeeded = Documentation rebuilt in { $duration_ms } ms
activity-docs-reloaded = Browser reload requested
activity-docs-rebuild-failed = Documentation rebuild failed: { $reason }
activity-docs-serve-ready = Documentation site ready at { $url }
activity-archive-compressing = Compressing { $files } files into { $path }...
activity-archive-created = Created { $format } archive at '{ $path }' from { $input_files } input files ({ $input_size }); output { $output_size }

status-docs-server-running = Documentation server running at { $url }
status-docs-watching = Watching documentation files
status-docs-rebuilding = Rebuilding documentation
status-docs-watching-succeeded = Watching; last rebuild succeeded
status-docs-watching-failed = Watching; last rebuild failed

plan-docs-server = Start the documentation development server
plan-docs-build = Generate the documentation site at { $output }
plan-archive-unresolved = Create archive at { $path } (input files resolve during execution)
plan-archive = Create archive from { $files } files at { $path }
plan-changelog = Generate changelog at { $path } with sections { $sections }
plan-version-update = Update { $path } to version { $version }
plan-version-commit = Create version commit: { $message }
plan-version-tag = Create version tag { $tag }
plan-shell-command = Run shell command: { $script }
plan-stage-files = Stage files: { $files }
plan-git-commit = Create Git commit '{ $message }' (signed={ $signed })
plan-git-tag = Create Git tag { $tag }
plan-git-push = Push { $refs } to { $remote } (force={ $force })
plan-git-prune-tags = Delete stale release tags from { $remote }
plan-write-file = Write file { $path }
