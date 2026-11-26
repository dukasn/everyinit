# Issue locale messages

issue-sync-workspace-not-loaded = Workspace configuration not loaded
issue-sync-execution-failed = { $error }
issue-env-syntax-error = Invalid key-value format. Expected KEY=VALUE
issue-yaml-parse-failed = { $error }
flow-syntax-error-title = Flow Syntax Error
flow-configuration-error-title = Configuration Error
issue-evin-header-invalid = Invalid Evin document header: { $error }
issue-policy-validation-failed = Invalid policy '{ $policy }': { $error }
issue-watcher-creation-failed = Failed to create watcher: { $error }
issue-sync-hook-update-failed = Failed to update hook '{ $hook }': { $error }
issue-sync-hook-script-write-failed = Failed to write script for hook '{ $hook }': { $error }
issue-sync-hook-script-permission-failed = Failed to set permissions for script for hook '{ $hook }': { $error }
issue-sync-file-copy-failed = Failed to copy file from '{ $source }' to '{ $destination }': { $error }
issue-sync-directory-creation-failed = Failed to create directory '{ $path }': { $error }
issue-sync-hooks-directory-cleanup-failed = Failed to clean up hooks directory: { $error }
issue-git-command-failed = Failed to execute { $command }: { $error }
issue-init-directory-create-failed = Failed to create directory '{ $path }': { $error }
issue-init-file-write-failed = Failed to write file '{ $path }': { $error }
issue-init-file-already-exists = File '{ $path }' already exists
issue-init-interact-failed = Interaction failed: { $error }

issue-changelog-version-invalid = version is invalid: { $error }
issue-archive-path-invalid = path is invalid: { $error }
issue-archive-prefix-invalid = prefix is invalid: { $error }
issue-archive-safe-path-failed = Safe path resolution failed for { $context }: { $error }
package-payload-source-prefix-required = package payload source and strip_prefix must not be empty
package-payload-mode-invalid = package payload mode must be at most 0777
package-payload-source-discovery-failed = could not resolve package payload source '{ $source }': { $error }
package-payload-outside-strip-prefix = payload '{ $source }' does not lie below strip_prefix '{ $prefix }'
package-payload-safe-path-failed = could not safely resolve package payload source '{ $source }': { $error }
package-payload-collision = package payload collision at '{ $destination }'
issue-version-bumper-unsupported-template = unsupported local template '${ $template }'
issue-ast-not-found = Input AST file '{ $path }' does not exist. Run the docs parse step first.
issue-docs-server-bind-failed = Could not bind documentation server to any available port.
issue-invocation-flag-unknown = Unknown invocation flag '{ $flag }' for the selected target.
issue-invocation-flag-ambiguous = Invocation flag '{ $flag }' is declared by both '{ $first_action }' and '{ $second_action }'.
issue-docs-tape-failed = Terminal tape recording failed: { $error }
issue-start-page-invalid = Invalid documentation start page: { $error }
start-page-invalid = Invalid documentation start page:
    { $error }
issue-watch-config-invalid = Invalid watch configuration for task '{ $task }': { $error }

issue-cycle-detected = Circular dependency detected between tasks: { $path }
issue-unhandled-dag-error = Could not prepare task dependencies: { $msg }
issue-invalid-target = Task '{ $target }' is not defined in the workspace registry.
issue-invalid-stage-target = Stage '{ $stage }' not found or contains no tasks.
issue-invalid-hook = Trigger hook '{ $hook }' is not defined in the workspace.
issue-action-config-invalid =
    Task '{ $task }' has an invalid configuration.

    { $clean_error }
issue-invalid-stage = Task '{ $task_name }' requested invalid stage '{ $stage }'. Valid stages are: { $valid_stages }
issue-invalid-stage-casing = Stage '{ $stage }' is invalid. Stages must use PascalCase (e.g., 'Build', 'TestAll').
issue-invalid-task-casing = Task '{ $task_name }' is invalid. Tasks must be entirely lowercase (e.g., 'lint', 'build-bin').
issue-parse-failed = Could not read configuration for task '{ $task_name }': { $error }
issue-file-parse-failed = Could not read file { $file_path }: { $error }
issue-missing-dependency = Task '{ $task }' requires '{ $dep }', but it does not exist in the registry.
issue-check-workspace-warning = { $message }
issue-check-unknown-module = Module or namespace '{ $module }' is not defined in this workspace.
issue-module-manifest-missing = Module directory '{ $module_dir }' has no module manifest.
issue-module-manifest-missing-title = Module Manifest Missing
issue-namespace-manifest-missing = Namespace file '{ $path }' does not exist.
issue-namespace-manifest-missing-title = Namespace File Missing
issue-duplicate-stage = Stage '{ $stage }' is declared more than once.
issue-duplicate-stage-title = Duplicate Stage
issue-duplicate-task = Task '{ $task_name }' is declared more than once.
issue-duplicate-task-title = Duplicate Task
issue-duplicate-module = Module '{ $name }' is declared more than once.
issue-duplicate-module-title = Duplicate Module
issue-duplicate-namespace = Namespace '{ $name }' is declared more than once.
issue-duplicate-namespace-title = Duplicate Namespace
issue-unknown-environment-preset = Task '{ $task }' selects undefined environment preset '{ $preset }'.
issue-unknown-environment-preset-title = Unknown Environment Preset
issue-missing-exposed-task = Exposed task '{ $task }' is not defined in this workspace.
issue-missing-exposed-task-title = Undefined Exposed Task
issue-invalid-task-definition = Invalid task definition: { $msg }
issue-task-defines-run-and-steps = Task '{ $task_name }' defines both 'run' and 'steps', which are mutually exclusive
issue-task-panicked = Task panicked: { $error }
issue-execution-interrupted = Execution interrupted by user (SIGINT)
issue-unknown-native-action = Unregistered or unknown native action: { $action }
issue-process-spawn-failed = Failed to spawn process: { $error }
issue-process-wait-failed = Process wait failed: { $error }
issue-task-state-output-failed = Failed to capture task state output: { $error }
issue-task-exit-failed = Process '{ $task_name }' terminated with exit code { $status }
issue-git-hook-parse-failed = Failed to parse git hook: { $hook }
issue-io-error = I/O error: { $error }
issue-git-directory-not-found = Failed to find .git directory: { $error }
issue-hooks-directory-create-failed = Failed to create hooks dir: { $error }
issue-config-syntax-error = Configuration Error
issue-commit-rejected = Commit Rejected
issue-push-rejected = Push Rejected
issue-lfs-requirement-unmet = LFS Requirement Unmet
issue-invalid-submodule = Invalid Submodule Configuration
issue-push-restricted = Push Restricted
issue-empty-commits-not-allowed = Cannot create an empty commit.
issue-empty-commits-hint = Stage your changes using `git add` before committing, or update the `allow_empty_commits` policy.
issue-missing-env-var = Required environment variable '{ $key }' is missing.
issue-invalid-env-format = Environment variable '{ $key }' has an invalid format. Expected pattern: '{ $expected }'
issue-invalid-env-type = Environment variable '{ $key }' has an invalid type. Expected: '{ $expected_type }'

# Internal & Policy issues
internal-error = { $message }
internal-error-title = Internal Error
action-failed-title = Action Failed
policy-git-error = { $message }
policy-git-error-title = Git Error
policy-missing-argument = { $message }
policy-missing-argument-title = Missing Argument

# Issue titles
issue-invalid-hook-title = Unknown Hook
issue-invalid-target-title = Invalid Target
issue-invalid-stage-target-title = Invalid Stage
issue-commit-rejected-title = Commit Rejected
issue-push-rejected-title = Push Rejected
issue-config-syntax-error-title = Configuration Error
issue-execution-failed-title = Execution Failed

issue-sync-workspace-not-loaded-title = Workspace Error
issue-sync-execution-failed-title = Execution Failed
issue-env-syntax-error-title = Environment Error
issue-yaml-parse-failed-title = Parse Error
issue-evin-header-invalid-title = Evin Document Header
issue-policy-validation-failed-title = Policy Violation
issue-watcher-creation-failed-title = Watcher Error
issue-git-command-failed-title = Git Error
issue-io-error-title = I/O Error
issue-git-directory-not-found-title = Git Error
issue-hooks-directory-create-failed-title = Git Error

issue-sync-hook-update-failed-title = Sync Failed
issue-sync-hook-script-write-failed-title = Sync Failed
issue-sync-hook-script-permission-failed-title = Sync Failed
issue-sync-file-copy-failed-title = Sync Failed
issue-sync-directory-creation-failed-title = Sync Failed
issue-sync-hooks-directory-cleanup-failed-title = Sync Failed

issue-init-directory-create-failed-title = Initialization Error
issue-init-file-write-failed-title = Initialization Error
issue-init-file-already-exists-title = File Exists
issue-init-interact-failed-title = Initialization Error

issue-cycle-detected-title = Dependency Cycle
issue-unhandled-dag-error-title = Pipeline Error
issue-invalid-stage-title = Stage Error
issue-invalid-stage-casing-title = Stage Naming Error
issue-invalid-task-casing-title = Task Naming Error
issue-parse-failed-title = Configuration Error
issue-file-parse-failed-title = File Error
issue-missing-dependency-title = Missing Dependency
issue-check-workspace-warning-title = Workspace Warning
issue-check-unknown-module-title = Unknown Module
issue-invalid-task-definition-title = Invalid Task
issue-task-defines-run-and-steps-title = Configuration Error
issue-action-config-invalid-title = Configuration Error
issue-task-panicked-title = Task Panicked
issue-task-exit-failed-title = Task Failed
issue-execution-interrupted-title = Execution Interrupted
issue-unknown-native-action-title = Unknown Action
issue-process-spawn-failed-title = Process Error
issue-process-wait-failed-title = Process Error
issue-task-state-output-failed-title = Task State Output Error
issue-git-hook-parse-failed-title = Hook Parse Error
issue-lfs-requirement-unmet-title = LFS Requirement
issue-invalid-submodule-title = Submodule Error
issue-push-restricted-title = Push Restricted
issue-empty-commits-not-allowed-title = Empty Commit

issue-missing-env-var-title = Missing Variable
issue-invalid-env-format-title = Invalid Format
issue-invalid-env-type-title = Invalid Type

issue-changelog-version-invalid-title = Changelog Error
issue-archive-path-invalid-title = Archive Error
issue-archive-prefix-invalid-title = Archive Error
issue-archive-safe-path-failed-title = Archive Error
issue-version-bumper-unsupported-template-title = Version Bumper Error
issue-ast-not-found-title = AST Not Found
issue-docs-server-bind-failed-title = Documentation Server Error
issue-docs-tape-failed-title = Terminal Tape Error
issue-start-page-invalid-title = Invalid Start Page
start-page-invalid-title = Invalid Start Page
issue-watch-config-invalid-title = Watch Configuration Error
issue-task-output-missing-dependency = Task '{ $task }' reads output '{ $reference }' from '{ $producer }', but that task is not a direct dependency. Add '{ $producer }' to needs:; transitive dependencies do not expose outputs.
issue-task-output-missing-dependency-title = Missing Output Dependency
