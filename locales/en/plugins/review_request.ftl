# Review request action messages

review-request-field-must-not-be-empty = { $name } must not be empty
review-request-field-must-not-be-empty-when-provided = { $name } must not be empty when provided
review-request-title-required = title is required when a review request is wanted
review-request-body-path-must-stay-in-workspace = body_path must stay inside the workspace
review-request-context-key-invalid = context key '{ $key }' may only contain letters, digits, and underscores
review-request-wanted-must-be-boolean = wanted must be true or false, got '{ $value }'
review-request-title-resolved-empty = the review request title resolved to an empty string
review-request-body-path-unusable = body_path is not usable: { $error }
review-request-dry-run-skipped-lookup = The existing review request was not looked up in a dry run
review-request-command-required-for-state = a review request must be { $state } but commands.{ $command } is not configured
review-request-command-failed = command failed with exit status { $status }: { $command }
