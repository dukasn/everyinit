# UI locale messages

phase-resolving-configuration = Resolving configuration
phase-evaluating-git-environment = Verifying git repository
phase-workspace-synchronized = Finished!
phase-executing-tasks = Executing pipeline tasks
phase-aggregating-git-hooks = Analyzing workspace tasks
phase-synchronizing-native-actions = Caching native actions
phase-synchronizing-git-hooks = Synchronizing git hooks
phase-generating-sync-summary = Generating sync summary
phase-initialize-workspace = Initializing workspace

intro-title = {" "} { $category } › { $target }{" "}
summary-title = Pipeline Summary
dry-run-summary-title = Dry Run Summary
execution-finished = Finished!
dry-run-completed-successfully = Dry run completed successfully
dry-run-completed-with-warnings = Dry run completed with warnings
dry-run-outcome-summary = { $planned } planned, { $unverified } unverified, { $failed } failed
pipeline-rejected = Execution aborted
watch-session-failed = Execution failed; watching for the next change
watching-for-changes = watching for file changes...
verifying-policy = Verifying workspace policies
workspace-sync-cancelled = Workspace synchronization stopped
resolving-configuration = Resolving configuration
configuration-resolution-failed = Configuration resolution failed
task-failed = [{ $task_name }] failed
task-unverified = [{ $task_name }] unverified

msg-gitrules-local = Loaded local custom gitrules from { $path }
msg-gitrules-remote-cached = Loaded remote cached custom gitrules from { $url }
msg-gitrules-remote-fetched = Loaded remote fetched custom gitrules from { $url }
msg-gitrules-fallback = Loaded default fallback gitrules
msg-workspace-modules = Detected { $count } workspace modules: { $formatted }
msg-workspace-value-single = Registered 1 workspace value
msg-workspace-values-plural = Registered { $count } workspace values
msg-config-verified = Configuration syntax successfully verified

evaluating-git-environment = Verifying git repository
executing-tasks = Executing pipeline tasks

severity-label-error = error
severity-label-warning = warning
severity-label-info = info
