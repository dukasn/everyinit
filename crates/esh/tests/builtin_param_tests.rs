// Copyright (C) 2026 Eduardo Marques Santos
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

use esh::{Engine, ExecutionContext, ScopeManager};

#[tokio::test]
async fn unsupported_process_parameters_remain_literal() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str("printf '%s|%s|%s|%s\\n' $$ $! \"$$\" \"$!\"\n", &mut ctx)
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "$$|$!|$$|$!\n");
}

#[tokio::test]
async fn test_export_builtin() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        LOCAL_VAR="local_value"
        export LOCAL_VAR
        export DIRECT_VAR="direct_value"
        echo "L:$LOCAL_VAR D:$DIRECT_VAR"
    "#;

    let status = engine
        .eval_str(script, &mut ctx)
        .await
        .expect("eval failed");
    assert_eq!(status, 0);

    assert_eq!(ctx.env_vars().get("LOCAL_VAR").unwrap(), "local_value");
    assert_eq!(ctx.env_vars().get("DIRECT_VAR").unwrap(), "direct_value");

    let output = ctx.get_captured_stdout().unwrap();
    assert!(output.contains("L:local_value D:direct_value"));
}

#[tokio::test]
async fn local_rejects_top_level_use_without_creating_a_variable() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str("local ESH_LOCAL_OUTSIDE_FUNCTION=value\n", &mut ctx)
        .await
        .unwrap();

    assert_eq!(status, 2);
    assert_eq!(ctx.get_var("ESH_LOCAL_OUTSIDE_FUNCTION"), None);
    assert_eq!(
        ctx.get_captured_stderr().unwrap(),
        "esh: local: only valid in a function\n"
    );
}

#[tokio::test]
async fn local_without_assignment_starts_empty_and_is_visible_to_called_functions() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                ESH_LOCAL_INITIAL=outer
                read_local() {
                    printf 'called=<%s>|<%s>\n' "${ESH_LOCAL_INITIAL-unset}" "${ESH_LOCAL_INITIAL:-fallback}"
                    ESH_LOCAL_INITIAL=changed
                }
                owner() {
                    local ESH_LOCAL_INITIAL
                    printf 'initial=<%s>\n' "${ESH_LOCAL_INITIAL-unset}"
                    read_local
                    printf 'updated=<%s>\n' "$ESH_LOCAL_INITIAL"
                }
                owner
                printf 'outer=<%s>\n' "$ESH_LOCAL_INITIAL"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "initial=<>\ncalled=<>|<fallback>\nupdated=<changed>\nouter=<outer>\n"
    );
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}

#[tokio::test]
async fn local_rejects_invalid_identifiers_without_binding_them() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "invalid_local() { local BAD-NAME=value; printf 'local-status=%s\\n' \"$?\"; }\ninvalid_local\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_var("BAD-NAME"), None);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "local-status=2\n");
    assert_eq!(
        ctx.get_captured_stderr().unwrap(),
        "esh: local: invalid identifier: BAD-NAME\n"
    );
}

#[tokio::test]
async fn declare_exports_existing_values_and_rejects_unimplemented_attributes() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    assert_eq!(
        engine
            .eval_str(
                "ESH_DECLARE_EXPORT=present; declare -x ESH_DECLARE_EXPORT\n",
                &mut ctx
            )
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        ctx.env_vars().get("ESH_DECLARE_EXPORT").map(String::as_str),
        Some("present")
    );

    for option in ["-r", "-i", "-z"] {
        assert_eq!(
            engine
                .eval_str(
                    &format!("declare {option} ESH_DECLARE_UNSUPPORTED\n"),
                    &mut ctx
                )
                .await
                .unwrap(),
            2
        );
    }
    assert_eq!(ctx.get_var("ESH_DECLARE_UNSUPPORTED"), None);
    assert_eq!(
        ctx.get_captured_stderr().unwrap(),
        "esh: declare: unsupported option '-r'\nesh: declare: unsupported option '-i'\nesh: declare: unsupported option '-z'\n"
    );
}

#[tokio::test]
async fn typeset_alias_declares_scalars_arrays_maps_and_exports() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"typeset VALUE=scalar
typeset -a ITEMS
ITEMS=(first "second item")
typeset -A SETTINGS
SETTINGS[mode]=safe
typeset -x ESH_TYPESET_EXPORT=visible
typeset -- ESH_TYPESET_TERMINATED=literal
printf '%s|%s|%s|%s|%s\n' "$VALUE" "${ITEMS[1]}" "${SETTINGS[mode]}" "$ESH_TYPESET_EXPORT" "$ESH_TYPESET_TERMINATED"
"#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "scalar|second item|safe|visible|literal\n"
    );
    assert_eq!(
        ctx.env_vars().get("ESH_TYPESET_EXPORT").map(String::as_str),
        Some("visible")
    );
    assert!(!ctx.env_vars().contains_key("ESH_TYPESET_TERMINATED"));
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}

#[tokio::test]
async fn unset_removes_variables_functions_and_exported_environment_values() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                ESH_UNSET_EXPORT=before
                export ESH_UNSET_EXPORT
                unset -v ESH_UNSET_EXPORT
                REMOVE_ME() { :; }
                unset -f REMOVE_ME
                printf '<%s>\n' "${ESH_UNSET_EXPORT-unset}"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_var("ESH_UNSET_EXPORT"), None);
    assert!(!ctx.env_vars().contains_key("ESH_UNSET_EXPORT"));
    assert!(ctx.get_function("REMOVE_ME").is_none());
    assert_eq!(ctx.get_captured_stdout().unwrap(), "<unset>\n");
}

#[tokio::test]
async fn unset_local_hides_outer_value_until_scope_returns() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                ESH_UNSET_SCOPE=outer
                hide_outer() {
                    local ESH_UNSET_SCOPE=inner
                    unset ESH_UNSET_SCOPE
                    printf '<%s>\n' "${ESH_UNSET_SCOPE-unset}"
                }
                hide_outer
                printf '<%s>\n' "$ESH_UNSET_SCOPE"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "<unset>\n<outer>\n");
}

#[tokio::test]
async fn unset_local_restores_outer_export_when_scope_returns() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                ESH_UNSET_EXPORTED=outer
                export ESH_UNSET_EXPORTED
                hide_export() {
                    local ESH_UNSET_EXPORTED=inner
                    unset ESH_UNSET_EXPORTED
                    printf '<%s>\n' "${ESH_UNSET_EXPORTED-unset}"
                }
                hide_export
                printf '<%s>\n' "$ESH_UNSET_EXPORTED"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_var("ESH_UNSET_EXPORTED").as_deref(), Some("outer"));
    assert_eq!(ctx.env_vars().get("ESH_UNSET_EXPORTED").unwrap(), "outer");
    assert_eq!(ctx.get_captured_stdout().unwrap(), "<unset>\n<outer>\n");
}

#[tokio::test]
async fn export_rejects_options_and_malformed_identifiers() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let option_status = engine.eval_str("export -n", &mut ctx).await.unwrap();
    assert_eq!(option_status, 2);

    for script in ["export 1INVALID=value", "export BAD-NAME"] {
        assert_eq!(engine.eval_str(script, &mut ctx).await.unwrap(), 2);
    }
    assert!(!ctx.env_vars().contains_key("1INVALID"));
    assert!(!ctx.env_vars().contains_key("BAD-NAME"));

    assert_eq!(engine.eval_str("export", &mut ctx).await.unwrap(), 0);
    assert_eq!(
        ctx.get_captured_stderr().unwrap(),
        "esh: export: unsupported option: -n\nesh: export: invalid identifier: 1INVALID\nesh: export: invalid identifier: BAD-NAME\n"
    );
}

#[tokio::test]
async fn export_prints_sorted_shell_quoted_export_attributes() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    assert_eq!(
        engine
            .eval_str(
                "export ZED=last ALPHA='it'\\''s value' EMPTY='' ONLY_NAME\nexport -p\n",
                &mut ctx
            )
            .await
            .unwrap(),
        0
    );
    let output = ctx.get_captured_stdout().unwrap();
    let alpha = output.find("export ALPHA='it'\\''s value'\n").unwrap();
    let empty = output.find("export EMPTY=''\n").unwrap();
    let only_name = output.find("export ONLY_NAME\n").unwrap();
    let zed = output.find("export ZED='last'\n").unwrap();
    assert!(alpha < empty && empty < only_name && only_name < zed);

    let mut restored = ExecutionContext::new();
    assert_eq!(engine.eval_str(&output, &mut restored).await.unwrap(), 0);
    assert_eq!(restored.get_var("ALPHA").as_deref(), Some("it's value"));
    assert_eq!(restored.get_var("EMPTY").as_deref(), Some(""));
    assert_eq!(restored.get_var("ZED").as_deref(), Some("last"));
}

#[tokio::test]
async fn export_accepts_double_dash_before_names_and_assignments() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_var("ESH_EXPORT_END_OPTIONS", "existing");

    let status = engine
        .eval_str(
            "export -- ESH_EXPORT_END_OPTIONS ESH_EXPORT_END_OPTIONS_NEW=assigned\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.env_vars()
            .get("ESH_EXPORT_END_OPTIONS")
            .map(String::as_str),
        Some("existing")
    );
    assert_eq!(
        ctx.env_vars()
            .get("ESH_EXPORT_END_OPTIONS_NEW")
            .map(String::as_str),
        Some("assigned")
    );
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}

#[tokio::test]
async fn exported_name_created_in_temporary_command_scope_persists() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "ESH_EXPORT_PREFIX_SCOPE=temporary export ESH_EXPORT_CREATED_SCOPE=created\nprintf '%s:%s\\n' \"${ESH_EXPORT_PREFIX_SCOPE-unset}\" \"$ESH_EXPORT_CREATED_SCOPE\"\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_var("ESH_EXPORT_PREFIX_SCOPE"), None);
    assert_eq!(
        ctx.get_var("ESH_EXPORT_CREATED_SCOPE").as_deref(),
        Some("created")
    );
    assert!(!ctx.env_vars().contains_key("ESH_EXPORT_PREFIX_SCOPE"));
    assert_eq!(
        ctx.env_vars()
            .get("ESH_EXPORT_CREATED_SCOPE")
            .map(String::as_str),
        Some("created")
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "unset:created\n");
}

#[cfg(unix)]
#[tokio::test]
async fn function_created_export_persists_and_reaches_child_like_bash() {
    let script = "unset ESH_EXPORT_FROM_FUNCTION\ncreate_export() { export ESH_EXPORT_FROM_FUNCTION=created; }\ncreate_export\nprintf 'parent=%s\\n' \"$ESH_EXPORT_FROM_FUNCTION\"\nsh -c 'printf \"child=%s\\n\" \"$ESH_EXPORT_FROM_FUNCTION\"'\n";
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    assert_eq!(engine.eval_str(script, &mut ctx).await.unwrap(), 0);
    let expected = b"parent=created\nchild=created\n";
    assert_eq!(ctx.get_captured_stdout().unwrap().as_bytes(), expected);
    assert_eq!(
        ctx.env_vars()
            .get("ESH_EXPORT_FROM_FUNCTION")
            .map(String::as_str),
        Some("created")
    );

    let reference = std::process::Command::new("bash")
        .args(["--posix", "-c"])
        .arg(script)
        .output()
        .expect("Bash should be available for the export-scope comparison");
    assert!(reference.status.success());
    assert_eq!(reference.stdout, expected);
    assert_eq!(reference.stderr, b"");
}

#[tokio::test]
async fn builtin_usage_statuses_obey_errexit_as_ordinary_failures() {
    let engine = Engine::new();
    let mut continuing = ExecutionContext::new();
    continuing.enable_capture();

    assert_eq!(
        engine
            .eval_str(
                "export BAD-NAME; printf 'continued:%s\\n' \"$?\"",
                &mut continuing
            )
            .await
            .unwrap(),
        0
    );
    assert_eq!(continuing.get_captured_stdout().unwrap(), "continued:2\n");

    let mut stopping = ExecutionContext::new();
    stopping.enable_capture();
    assert_eq!(
        engine
            .eval_str("set -e; export BAD-NAME; printf unreachable", &mut stopping)
            .await
            .unwrap(),
        2
    );
    assert!(stopping.is_exiting());
    assert_eq!(stopping.get_captured_stdout().unwrap(), "");
    assert!(
        stopping
            .get_captured_stderr()
            .unwrap()
            .contains("invalid identifier: BAD-NAME")
    );
}

#[tokio::test]
async fn exported_variables_track_later_assignments_and_unset_exports() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                ESH_EXPORT_UPDATE=before
                export ESH_EXPORT_UPDATE
                ESH_EXPORT_UPDATE=after
                export ESH_EXPORT_LATER
                ESH_EXPORT_LATER=created
                ESH_TEMP_PREFIX=temporary env
                ESH_TEMP_PREFIX=local
                printf '%s\n' "$ESH_EXPORT_UPDATE"
                printf '%s\n' "$ESH_EXPORT_LATER"
                printf '%s\n' "$ESH_TEMP_PREFIX"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    let output = ctx.get_captured_stdout().unwrap();
    assert!(
        output
            .lines()
            .any(|line| line == "ESH_TEMP_PREFIX=temporary")
    );
    assert!(output.ends_with("after\ncreated\nlocal\n"));
    assert_eq!(
        ctx.env_vars().get("ESH_EXPORT_UPDATE").map(String::as_str),
        Some("after")
    );
    assert_eq!(
        ctx.env_vars().get("ESH_EXPORT_LATER").map(String::as_str),
        Some("created")
    );
    assert!(!ctx.env_vars().contains_key("ESH_TEMP_PREFIX"));
}

#[tokio::test]
async fn function_scopes_restore_exported_values_and_attributes() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_env("ESH_LOCAL_EXPORT_SCOPE", "outer");
    ctx.set_env("ESH_GLOBAL_EXPORT_SCOPE", "before");

    let status = engine
        .eval_str(
            r#"
                local_export() {
                    local ESH_LOCAL_EXPORT_SCOPE=inner
                    printf '%s\n' "$ESH_LOCAL_EXPORT_SCOPE"
                    export ESH_FUNCTION_EXPORT=inside
                    printf '%s\n' "$ESH_FUNCTION_EXPORT"
                }
                update_global() { ESH_GLOBAL_EXPORT_SCOPE=after; }
                local_export
                update_global
                printf '%s\n' "$ESH_LOCAL_EXPORT_SCOPE"
                printf '%s\n' "$ESH_GLOBAL_EXPORT_SCOPE"
                printf '%s\n' "${ESH_FUNCTION_EXPORT-not-exported}"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "inner\ninside\nouter\nafter\ninside\n"
    );
    assert_eq!(
        ctx.env_vars()
            .get("ESH_LOCAL_EXPORT_SCOPE")
            .map(String::as_str),
        Some("outer")
    );
    assert_eq!(
        ctx.env_vars()
            .get("ESH_FUNCTION_EXPORT")
            .map(String::as_str),
        Some("inside")
    );
    assert_eq!(
        ctx.env_vars()
            .get("ESH_GLOBAL_EXPORT_SCOPE")
            .map(String::as_str),
        Some("after")
    );
}

#[tokio::test]
async fn local_optind_restores_the_callers_getopts_cluster_cursor() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    let script = r#"set -- -abc
OPTIND=1
getopts abc OPTION
FIRST=$OPTION
consume_local_options() {
    set -a
    local OPTIND=1
    set +a
    getopts xyz OPTION
    getopts xyz OPTION
}
consume_local_options -xyz
getopts abc OPTION
NEXT=$OPTION
printf '%s:%s:%s\n' "$FIRST" "$NEXT" "$OPTIND"
"#;

    let status = engine.eval_str(script, &mut ctx).await.unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "a:b:1\n");

    #[cfg(unix)]
    if let Ok(reference) = std::process::Command::new("bash")
        .args(["--posix", "-c", script])
        .output()
    {
        assert!(reference.status.success());
        assert_eq!(reference.stdout, b"a:b:1\n");
    }
}

#[tokio::test]
async fn prefix_optind_restores_the_callers_getopts_cluster_cursor() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"set -- -abc
OPTIND=1
OPTIND=1 getopts xyz OPTION -xyz
getopts abc OPTION
printf '%s:%s\n' "$OPTION" "$OPTIND"
"#;
    let status = engine.eval_str(script, &mut ctx).await.unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "a:1\n");

    #[cfg(unix)]
    if let Ok(reference) = std::process::Command::new("bash")
        .args(["--posix", "-c", script])
        .output()
    {
        assert!(reference.status.success());
        assert_eq!(reference.stdout, b"a:1\n");
    }
}

#[tokio::test]
async fn test_set_and_shift_builtins() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        set -- "alpha" "beta" "gamma" "delta"
        echo "count: $# first: $1 all: $@"
        shift
        echo "after shift 1: count: $# first: $1"
        shift 2
        echo "after shift 2: count: $# first: $1"
    "#;

    let status = engine
        .eval_str(script, &mut ctx)
        .await
        .expect("eval failed");
    assert_eq!(status, 0);

    let output = ctx.get_captured_stdout().unwrap();
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(
        lines[0],
        "count: 4 first: alpha all: alpha beta gamma delta"
    );
    assert_eq!(lines[1], "after shift 1: count: 3 first: beta");
    assert_eq!(lines[2], "after shift 2: count: 1 first: delta");
}

#[tokio::test]
async fn prefix_assignments_restore_around_stateful_special_builtins() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();

    ctx.set_positional_args(vec!["script.esh".into()]);
    let status = engine
        .eval_str(
            "ESH_PREFIX_TEMP=set set -- alpha beta\nESH_PREFIX_TEMP=shift shift",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.positional_args(), &["beta"]);
    assert_eq!(ctx.get_var("ESH_PREFIX_TEMP"), None);
}

#[tokio::test]
async fn colon_uses_esh_prefix_assignment_scope_instead_of_posix_persistence() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "TEMP=outer\nTEMP=prefix :\nprintf '%s\\n' \"$TEMP\"\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_var("TEMP").as_deref(), Some("outer"));
    assert_eq!(ctx.get_captured_stdout().unwrap(), "outer\n");
}

#[tokio::test]
async fn true_and_false_return_statuses_without_external_commands() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "true ignored operands\nTRUE_STATUS=$?\nfalse ignored operands\nFALSE_STATUS=$?\nprintf '%s:%s\\n' \"$TRUE_STATUS\" \"$FALSE_STATUS\"\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "0:1\n");
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}

#[tokio::test]
async fn shift_validates_count_and_preserves_arguments_on_error() {
    let engine = Engine::new();

    for script in ["shift nope", "shift -1", "shift 1 2", "shift 4"] {
        let mut ctx = ExecutionContext::new();
        ctx.enable_capture();
        ctx.set_positional_args(vec![
            "script.esh".to_string(),
            "alpha".to_string(),
            "beta".to_string(),
            "gamma".to_string(),
        ]);
        assert_eq!(
            engine.eval_str(script, &mut ctx).await.unwrap(),
            2,
            "{script}"
        );
        assert_eq!(ctx.positional_args(), &["alpha", "beta", "gamma"]);
    }

    let mut ctx = ExecutionContext::new();
    ctx.set_positional_args(vec![
        "script.esh".to_string(),
        "alpha".to_string(),
        "beta".to_string(),
    ]);
    assert_eq!(engine.eval_str("shift 0", &mut ctx).await.unwrap(), 0);
    assert_eq!(ctx.positional_args(), &["alpha", "beta"]);
    assert_eq!(engine.eval_str("shift +1", &mut ctx).await.unwrap(), 0);
    assert_eq!(ctx.positional_args(), &["beta"]);
}

#[tokio::test]
async fn getopts_handles_option_clusters_arguments_and_explicit_operands() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        set -- -abc -- tail
        OPTIND=1
        while getopts abc option; do
            echo "$option:$OPTIND"
        done
        echo "end:$OPTIND"

        OPTIND=1
        getopts n: option -nvalue tail
        echo "explicit:$option:$OPTARG:$OPTIND"

        OPTIND=1
        getopts :a: option -a
        echo "missing:$option:$OPTARG:$OPTIND"
    "#;
    let status = engine.eval_str(script, &mut ctx).await.unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "a:1\nb:1\nc:2\nend:3\nexplicit:n:value:2\nmissing:::a:2\n"
    );
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}

#[tokio::test]
async fn getopts_reports_malformed_invocations_as_usage_errors() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    assert_eq!(engine.eval_str("getopts", &mut ctx).await.unwrap(), 2);
    assert_eq!(
        engine
            .eval_str("getopts a 1INVALID", &mut ctx)
            .await
            .unwrap(),
        2
    );
    assert_eq!(ctx.get_var("1INVALID"), None);
    assert_eq!(
        ctx.get_captured_stderr().unwrap(),
        "esh: getopts: usage: getopts optstring name [arg ...]\nesh: getopts: invalid variable name: 1INVALID\n"
    );
}

#[tokio::test]
async fn getopts_unsets_optarg_on_end_and_non_silent_errors() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                set -- operand
                option=stale
                OPTARG=stale
                OPTIND=1
                getopts a option
                printf 'end:<%s><%s><%s>\n' "$option" "${OPTARG-unset}" "$OPTIND"

                set -- -z
                option=stale
                OPTARG=stale
                OPTIND=1
                getopts a option
                printf 'unknown:<%s><%s><%s>\n' "$option" "${OPTARG-unset}" "$OPTIND"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "end:<?><unset><1>\nunknown:<?><unset><2>\n"
    );
    assert_eq!(
        ctx.get_captured_stderr().unwrap(),
        "esh: getopts: illegal option -- z\n"
    );
}

#[tokio::test]
async fn getopts_silent_unknown_cluster_option_sets_optarg_and_keeps_cluster_state() {
    let script = r#"set -- -az
OPTIND=1
getopts :a option
printf 'first=%s:<%s>:%s:%s\n' "$option" "${OPTARG-unset}" "$OPTIND" "$?"
getopts :a option
printf 'unknown=%s:<%s>:%s:%s\n' "$option" "${OPTARG-unset}" "$OPTIND" "$?"
getopts :a option
printf 'end=%s:<%s>:%s:%s\n' "$option" "${OPTARG-unset}" "$OPTIND" "$?"
"#;
    let expected = concat!(
        "first=a:<unset>:1:0\n",
        "unknown=?:<z>:2:0\n",
        "end=?:<unset>:2:1\n"
    );
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    assert_eq!(engine.eval_str(script, &mut ctx).await.unwrap(), 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), expected);
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");

    #[cfg(unix)]
    if let Ok(reference) = std::process::Command::new("bash")
        .args(["--posix", "-c", script])
        .output()
    {
        assert!(reference.status.success());
        assert_eq!(reference.stdout, expected.as_bytes());
        assert_eq!(reference.stderr, b"");
    }
}

#[tokio::test]
async fn getopts_continues_after_silent_unknown_option_inside_cluster() {
    let script = r#"set -- -azbVALUE
OPTIND=1
getopts :ab: option
printf 'first=%s:<%s>:%s\n' "$option" "${OPTARG-unset}" "$OPTIND"
getopts :ab: option
printf 'unknown=%s:<%s>:%s\n' "$option" "${OPTARG-unset}" "$OPTIND"
getopts :ab: option
printf 'argument=%s:<%s>:%s\n' "$option" "$OPTARG" "$OPTIND"
getopts :ab: option
printf 'end=%s:<%s>:%s:%s\n' "$option" "${OPTARG-unset}" "$OPTIND" "$?"
"#;
    let expected = concat!(
        "first=a:<unset>:1\n",
        "unknown=?:<z>:1\n",
        "argument=b:<VALUE>:2\n",
        "end=?:<unset>:2:1\n"
    );
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    assert_eq!(engine.eval_str(script, &mut ctx).await.unwrap(), 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), expected);
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");

    #[cfg(unix)]
    if let Ok(reference) = std::process::Command::new("bash")
        .args(["--posix", "-c", script])
        .output()
    {
        assert!(reference.status.success());
        assert_eq!(reference.stdout, expected.as_bytes());
        assert_eq!(reference.stderr, b"");
    }
}

#[tokio::test]
async fn getopts_non_silent_missing_argument_sets_question_and_advances_optind() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "set -- -b\nOPTIND=1\nOPTARG=stale\ngetopts b: option\nRESULT=$?\nprintf '%s:<%s>:%s:%s\\n' \"$option\" \"${OPTARG-unset}\" \"$OPTIND\" \"$RESULT\"\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "?:<unset>:2:0\n");
    assert_eq!(
        ctx.get_captured_stderr().unwrap(),
        "esh: getopts: option requires an argument -- b\n"
    );
}

#[tokio::test]
async fn getopts_rejects_malformed_option_specifications() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    for optstring in ["a?b", "a::", "aba"] {
        assert_eq!(
            engine
                .eval_str(&format!("getopts '{optstring}' option -a\n"), &mut ctx)
                .await
                .unwrap(),
            2
        );
    }
    assert_eq!(ctx.get_var("option"), None);
    assert_eq!(
        ctx.get_captured_stderr().unwrap(),
        "esh: getopts: invalid option specification: a?b\nesh: getopts: invalid option specification: a::\nesh: getopts: invalid option specification: aba\n"
    );
}
