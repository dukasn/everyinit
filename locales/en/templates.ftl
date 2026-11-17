# EVIN Default Template Localizations

init-template-workspace =
    #!evin/v1
    # EVIN Workspace Configuration
    # Defines your project's identity, pipeline stages, and automation tasks.
    workspace:
      name: evin-project
      version: "0.1.0"
      policy: ".evin/policy.json" # Binds your pipeline to your version control policies.
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
    #!evin/v1
    # EVIN Module Configuration
    module:
      name: my-module
      # version: "0.1.0" # Optional independent version; otherwise inherit the workspace.

    tasks:
      build:
        stage: Build
        description: "Build module"
        steps:
          - run: echo "Building module..."
