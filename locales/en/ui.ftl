# UI locale messages

phase-resolving-configuration = Resolving configuration
phase-evaluating-git-environment = Verifying git repository
phase-workspace-synchronized = Finished!
phase-executing-tasks = Executing pipeline tasks
phase-aggregating-git-hooks = Analyzing workspace tasks
phase-synchronizing-native-actions = Caching native actions
phase-reconciling-release-lock = Reconciling release lock
phase-synchronizing-git-hooks = Synchronizing git hooks
phase-generating-sync-summary = Generating sync summary
phase-initialize-workspace = Initializing workspace

discovery-catalog-description = A guided view of this workspace's available commands.
discovery-modules-title = Modules
discovery-stages-title = Stages
discovery-hooks-title = Hooks
discovery-tasks-at-title = Tasks at
discovery-catalog-summary-title = Catalog Summary
discovery-steps-title = Steps
discovery-execution-plan-title = Execution Plan
discovery-finished = Finished!
discovery-failed = Failed!
discovery-no-description = (no description)
discovery-no-tasks = No tasks resolved
discovery-resolved-label = resolved
discovery-stage-label = stage
discovery-cwd-label = cwd
discovery-needs-label = needs
discovery-environment-label = environment
discovery-steps-label = steps
discovery-kind-task = task
discovery-kind-stage = stage
discovery-kind-hook = hook
discovery-kind-workspace = workspace
discovery-kind-unknown = unknown
discovery-task-count = { $count } { $count ->
    [one] task
   *[other] tasks
}
discovery-catalog-all-summary = { $count } { $count ->
    [one] target
   *[other] targets
} available
discovery-explain-summary = { $tasks } { $tasks ->
    [one] task
   *[other] tasks
} · { $steps } { $steps ->
    [one] step
   *[other] steps
} · nothing was executed

check-description = Statically validating repository workflow contracts, manifests, and task graphs.
check-checked-manifest = Checked { $path } ({ $tasks } { $tasks ->
    [one] task
   *[other] tasks
})
check-checked-rules = Checked { $path }
check-checked-env = Checked { $path }
check-summary-title = Validation Summary
check-finished-success = All workflow contracts are sound!
check-finished-failure = Failed!
check-errors-title = Validation failed
check-warnings-title = Validation warnings
check-status-label = Status
check-targets-label = Targets
check-graph-label = Graph
check-status-valid = Valid
check-status-invalid = Invalid
check-status-summary = { $status } ({ $errors } { $errors ->
    [one] error
   *[other] errors
}, { $warnings } { $warnings ->
    [one] warning
   *[other] warnings
})
check-result-summary = { $tasks } { $tasks ->
    [one] task
   *[other] tasks
} · { $errors } { $errors ->
    [one] error
   *[other] errors
} · { $warnings } { $warnings ->
    [one] warning
   *[other] warnings
} · nothing was executed
check-selection-summary = No manifests were checked
check-kind-unknown-module = Unknown module
check-kind-unknown-target = Unknown target
check-workspace-unresolved = (unresolved)
check-fence-aborted = aborted
check-fence-review = review
check-all-summary = { $tasks } { $tasks ->
    [one] task
   *[other] tasks
}, { $stages } { $stages ->
    [one] stage
   *[other] stages
}, { $modules } { $modules ->
    [one] module
   *[other] modules
}, { $namespaces } { $namespaces ->
    [one] namespace
   *[other] namespaces
}
check-graph-summary = { $nodes } nodes, { $edges } edges, { $cycles } cycles detected

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
