# CLI locale messages

app-description = Evin — The Workflow Engine
init-command-help = Initialize .evin configuration for this repository
init-workspace-help = Generate a default .evin/workspace.yaml
init-gitrules-help = Generate a default .evin/gitrules.yaml
init-env-help = Generate a default .evin/environment.yaml
init-module-help = Generate a default .evin/module.yaml in the specified path
init-all-help = Generate all default workspace configuration files
init-path-help = Target directory path for workspace initialization

sync-command-help = Install and update Git hooks declared by this repository

run-command-help = Run a declared task, stage, or Git-hook workflow
run-target-help = Task, stage, or Git hook to run
run-args-help = Arguments passed to the target after '--'
run-parallel-help = Run ready tasks in parallel
run-sequential-help = Run ready tasks sequentially
dry-run-help = Plan the workflow without filesystem, Git, or shell side effects
debug-help = Show detailed workflow diagnostics and metrics
verbose-help = Show successful task output too
phase-resolving-configuration = Resolving configuration
phase-initialize-workspace = Initializing workspace



suggestion-hint = Hint: Did you mean '{ $suggestion }'?
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
