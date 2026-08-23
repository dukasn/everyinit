# EVIN Default Template Localizations

init-template-gitrules =
    # EVIN GitRules Configuration
    # Defines the version control policies and conventions for your workspace.
    version: 1
    name: default

    commits:
      # The conventional commit format used for all commit messages.
      # Can be a single string or an array of acceptable formats.
      message_format:
        - "{"{type}({scope}?){breaking?}: {description}"}"
        - "{"{type}: {description}"}"

      # Commits matching these patterns will bypass validation.
      ignore_patterns:
        - "^Merge branch"
        - "^Revert"

      # Enforce specific rules for different parts of the commit message.
      fields:
        type:
          allowed_values: [feat, fix, docs, style, refactor, perf, test, build, ci, chore, revert]
          must_be: lowercase-with-hyphens

      policies:
        require_body: false
        require_ticket_reference: false

    branches:
      # The format required for branch names.
      name_format:
        - "{"{type}/{ticket}-{description}"}"
        - "{"{type}/{description}"}"

      fields:
        type:
          allowed_values: [feat, fix, chore, docs, refactor, release]
          must_be: lowercase-with-hyphens
        description:
          must_be: lowercase-with-hyphens

      policies:
        block_push_to_main: false

    # Policies for git tags (typically created during releases).
    tags:
      name_format: "v{"{version}"}"
      policies:
        require_annotated: true
        block_deletion: true

    # Formatting and validation rules for Pull Requests.
    pull_requests:
      title_format: "{"{type}({scope}?): {description}"}"
      policies:
        require_assignee: false
        require_linked_issue: false
        block_wip_prefixes: ["WIP:", "draft:", "wip:"]

init-template-workspace =
    # EVIN Workspace Configuration
    # Defines your project's identity, pipeline stages, and automation tasks.
    workspace:
      name: evin-project
      version: "0.1.0"
      rules: ".evin/gitrules.yaml" # Binds your pipeline to your version control policies.
      # execution: linear # Task execution mode: 'linear' (default) or 'parallel'.

    # Stages define the visual structure and execution order of your pipeline.
    stages:
      - Build
      - Test
    # - Publish

    tasks:
      build:
        stage: Build
        description: "Build the project"
        steps:
          - run: echo "Building project..."

      test:
        stage: Test
        description: "Test the project"
        needs: [build] # Dependencies ensure tasks run in the correct order.
        # parallel: true # Set to true to allow concurrent execution with other parallel tasks.
        steps:
          - run: echo "Testing project..."

init-template-module =
    # EVIN Module Configuration
    module:
      name: my-module
      version: "0.1.0"

    tasks:
      build:
        stage: Build
        description: "Build module"
        steps:
          - run: echo "Building module..."

init-template-environment =
    # EVIN Environment Configuration
    # Declares environment variable schemas, secret redaction rules, and stage scopes.
    version: 1

    presets:
      default: ["LOG_LEVEL", "PORT"]
      ci: ["LOG_LEVEL", "PORT", "DATABASE_URL"]

    environments:
      LOG_LEVEL:
        type: string
        default: "info"
        description: "Application logging verbosity"

      PORT:
        type: number
        default: "8080"
        description: "HTTP server port"

      DATABASE_URL:
        type: secret
        required: true
        description: "Database connection URI (redacted in logs)"
