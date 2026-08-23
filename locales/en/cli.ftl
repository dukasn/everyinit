# CLI locale messages

app-description = EVIN - Native Execution Engine
init-command-help = Initialize an EVIN workspace and generate default configurations
init-workspace-help = Generate a default .evin/workspace.yaml
init-gitrules-help = Generate a default .evin/gitrules.yaml
init-env-help = Generate a default .evin/environment.yaml
init-module-help = Generate a default .evin/module.yaml in the specified path
init-all-help = Generate all default workspace configuration files
init-path-help = Target directory path for workspace initialization

sync-command-help = Synchronizes workspace configurations and git hooks

run-command-help = Runs a specific target task manually
run-target-help = The name of the target task to run
run-args-help = Arguments passed to the task after '--'
run-parallel-help = Force parallel execution of ready tasks
run-sequential-help = Force linear (sequential) execution of tasks
dry-run-help = Plan the task graph without filesystem, Git, or shell side effects
debug-help = Enable detailed debug logging and pipeline metrics
verbose-help = Show output of all tasks even on success
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
