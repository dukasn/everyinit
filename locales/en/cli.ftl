# CLI locale messages

app-description = Evin — Development, orchestrated.
init-command-help = Initialize .evin configuration for this repository
init-workspace-help = Generate a default .evin/workspace.flow
init-gitrules-help = Generate a default .evin/gitrules.flow
init-env-help = Generate a default .evin/environment.flow
init-module-help = Generate a default .evin/module.flow in the specified path
init-all-help = Generate all default workspace configuration files
init-path-help = Target directory path for workspace initialization

sync-command-help = Install and update Git hooks declared by this repository
list-command-help = List declared tasks, stages, and Git hooks
explain-command-help = Explain a target and its resolved dependency plan
explain-target-help = Task, stage, or Git hook to explain
check-command-help = Validate workspace configuration, task graphs, and dependencies without running them
check-workspace-help = Validate only the root workspace configuration and tasks
check-module-help = Validate only the specified module or namespace
check-target-help = Task, stage, or Git hook target to validate

run-command-help = Run a declared task, stage, or Git-hook workflow
run-target-help = Task, stage, or Git hook to run
run-args-help = Arguments passed to the target after '--'
run-parallel-help = Run ready tasks in parallel
run-sequential-help = Run ready tasks sequentially
dry-run-help = Plan the workflow without filesystem, Git, or shell side effects
debug-help = Show detailed workflow diagnostics and metrics
verbose-help = Show successful task output too
phase-resolving-configuration = Resolving configuration
phase-checking-workspace = Checking workspace
phase-initialize-workspace = Initializing workspace



suggestion-hint = Did you mean '{ $suggestion }'?
invalid-target-title = Invalid Task
unknown-hook-title = Unknown Hook
invalid-git-hook = Invalid git hook specified in task '{ $task_name }': '{ $hook }'
git-hooks-preview = Workspace synchronization preview
hook-change-create = create
hook-change-update = update
hook-change-unchanged = unchanged
sync-change-stale = stale cache
schema-preview = schema { $schema } — { $change }
hook-preview = { $hook } — { $change } ({ $count ->
    [one] { $count } task
   *[other] { $count } tasks
})
hook-task-count = { $hook } ({ $count ->
    [one] { $count } task
   *[other] { $count } tasks
})
hook-install-failed = Failed to install hook '{ $hook }': { $error }
hook-install-failed-title = Hook Installation Failed
