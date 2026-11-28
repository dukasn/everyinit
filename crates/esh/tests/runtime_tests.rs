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

use esh::runtime::traits::ScopeManager;
mod common;

use esh::{Engine, ExecutionContext};
use std::time::Duration;
use tempfile::tempdir;

#[tokio::test]
async fn test_runtime_echo_and_capture() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str("echo 'Hello' 'World'", &mut ctx)
        .await
        .expect("eval failed");

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "Hello World\n");
}

#[tokio::test]
async fn echo_extension_flags_have_stable_esh_semantics() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "echo -n 'plain\\ntext'\necho -e 'escaped\\ntext'\necho -E 'literal\\ntext'\necho -f '%s:%d' item 7\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "plain\\ntextescaped\ntext\nliteral\\ntext\nitem:7\n"
    );
}

#[tokio::test]
async fn echo_accepts_documented_combined_flags_and_attached_format() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "echo -ne 'one\\n'\necho -en 'two\\t'\necho -f%s value\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "one\ntwo\tvalue\n");
}

#[tokio::test]
async fn echo_appends_its_record_newline_after_operand_newlines() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str("echo 'line\n'\necho -e 'escaped\\n'\n", &mut ctx)
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "line\n\nescaped\n\n");
}

#[tokio::test]
async fn assignment_only_commands_expand_the_previous_exit_status() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "test 1 -eq 2\nSAVED_STATUS=$?\necho $SAVED_STATUS",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "1\n");
}

#[tokio::test]
async fn mixed_pipeline_preserves_compound_final_stage_state_and_pipefail_status() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"set -o pipefail
fail_with_seven() { printf 'payload\n'; return 7; }
fail_with_seven | { read RESULT; }
PIPE_STATUS=$?
printf '%s:%s\n' "$RESULT" "$PIPE_STATUS"
"#,
            &mut ctx,
        )
        .await
        .expect("mixed pipeline execution failed");

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "payload:7\n");
}

#[tokio::test]
async fn final_pipeline_stage_adopts_variables_as_an_esh_extension() {
    let script = "VALUE=outer\nprintf input | { VALUE=stage; }\nprintf '%s\\n' \"$VALUE\"\n";
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    assert_eq!(engine.eval_str(script, &mut ctx).await.unwrap(), 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "stage\n");

    #[cfg(unix)]
    for (shell, args) in [("bash", vec!["--posix", "-c"]), ("dash", vec!["-c"])] {
        if let Ok(reference) = std::process::Command::new(shell)
            .args(args)
            .arg(script)
            .output()
        {
            assert!(
                reference.status.success(),
                "{shell} failed to run the pipeline"
            );
            assert_eq!(
                reference.stdout, b"outer\n",
                "{shell} should isolate variable assignments in pipeline stages"
            );
        }
    }
}

#[tokio::test]
async fn pipeline_adopts_final_stage_export_attributes_with_variable_values() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "unset ESH_PIPELINE_EXPORT\nESH_PIPELINE_EXPORT=adopted\nprintf x | export ESH_PIPELINE_EXPORT\nexport -p | grep '^export ESH_PIPELINE_EXPORT='\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "export ESH_PIPELINE_EXPORT='adopted'\n"
    );
}

#[tokio::test]
async fn pipeline_adopts_final_stage_variables_without_leaking_function_definitions() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"VALUE=outer
existing() { printf 'outer-function\n'; }
printf input | {
    VALUE=adopted
    existing() { printf 'pipeline-function\n'; }
    pipeline_only() { printf 'leaked-function\n'; }
}
existing
if pipeline_only; then
    printf 'new-function-leaked\n'
else
    printf 'new-function-isolated\n'
fi
printf 'value=%s\n' "$VALUE"
"#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "outer-function\nnew-function-isolated\nvalue=adopted\n"
    );
}

#[tokio::test]
async fn pipeline_adopts_getopts_cluster_cursor_with_optind_and_option_variable() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"set -- -abc
OPTIND=1
getopts abc OPTION
FIRST=$OPTION
printf x | getopts abc OPTION
PIPELINE=$OPTION
getopts abc OPTION
NEXT=$OPTION
printf '%s:%s:%s:%s\n' "$FIRST" "$PIPELINE" "$NEXT" "$OPTIND"
"#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "a:b:c:2\n");
}

#[tokio::test]
async fn assignment_only_command_status_comes_from_last_command_substitution() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "PLAIN=value\nPLAIN_STATUS=$?\nFAILED=$(false)\nFAILED_STATUS=$?\nFIRST=$(false) LAST=$(true)\nMULTI_STATUS=$?\nFIRST=$(true) LAST=$(false)\nREVERSE_STATUS=$?\nX=1 <\"$(printf /dev/null; false)\"\nREDIRECTION_STATUS=$?\nprintf '%s:%s:%s:%s:%s\\n' \"$PLAIN_STATUS\" \"$FAILED_STATUS\" \"$MULTI_STATUS\" \"$REVERSE_STATUS\" \"$REDIRECTION_STATUS\"",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "0:1:0:1:0\n");
}

#[tokio::test]
async fn test_external_command_stdout_and_stderr_are_captured() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let esh = env!("CARGO_BIN_EXE_esh");
    let script = format!("'{esh}' --version; '{esh}' help definitely-not-a-builtin || true");
    let status = engine
        .eval_str(&script, &mut ctx)
        .await
        .expect("eval failed");

    assert_eq!(status, 0);
    assert!(ctx.get_captured_stdout().unwrap().starts_with("esh "));
    assert!(
        ctx.get_captured_stderr()
            .unwrap()
            .contains("no help documentation found")
    );
}

#[tokio::test]
async fn test_runtime_filesystem_builtins() {
    let tmp = tempdir().unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(tmp.path()).unwrap();

    let script = r#"
        mkdir -p sub/nested
        touch sub/nested/test.txt
        cp sub/nested/test.txt sub/copy.txt
        mv sub/copy.txt sub/renamed.txt
    "#;

    let status = engine
        .eval_str(script, &mut ctx)
        .await
        .expect("eval failed");
    assert_eq!(status, 0);

    assert!(tmp.path().join("sub/nested/test.txt").exists());
    assert!(tmp.path().join("sub/renamed.txt").exists());
    assert!(!tmp.path().join("sub/copy.txt").exists());

    // Clean up with rm -rf
    let rm_script = "rm -rf sub";
    let status_rm = engine
        .eval_str(rm_script, &mut ctx)
        .await
        .expect("eval failed");
    assert_eq!(status_rm, 0);
    assert!(!tmp.path().join("sub").exists());
}

#[tokio::test]
async fn test_runtime_variables_and_expansion() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_local("USER_NAME", "EvinDev".to_string());

    let script = r#"
        echo "User is ${USER_NAME}"
        GREETING=Hello echo "$GREETING $USER_NAME"
    "#;

    let status = engine
        .eval_str(script, &mut ctx)
        .await
        .expect("eval failed");
    assert_eq!(status, 0);

    let output = ctx.get_captured_stdout().unwrap();
    assert!(output.contains("User is EvinDev"));
    assert!(output.contains("Hello EvinDev"));
}

#[tokio::test]
async fn test_runtime_command_substitution() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = "echo \"Current status: $(echo active)\"";
    let status = engine
        .eval_str(script, &mut ctx)
        .await
        .expect("eval failed");
    assert_eq!(status, 0);

    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "Current status: active\n"
    );
}

#[tokio::test]
async fn crlf_source_lines_treat_carriage_returns_as_whitespace() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    let script = "VALUE=first\r\nprintf '<%s>\\n' \"$VALUE\"\r\n";
    let expected = b"<first>\n";

    assert_eq!(engine.eval_str(script, &mut ctx).await.unwrap(), 0);
    assert_eq!(ctx.get_captured_stdout().unwrap().as_bytes(), expected);
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");

    #[cfg(unix)]
    if let Ok(reference) = std::process::Command::new("bash")
        .args(["--posix", "-c"])
        .arg(script)
        .output()
    {
        assert!(reference.status.success(), "Bash failed on CRLF source");
        assert_eq!(reference.stdout, b"<first\r\r>\n");
    }
}

#[tokio::test]
async fn crlf_source_lines_preserve_compound_control_flow_boundaries() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let source = "VALUE=first\nif true; then\n  printf 'if:%s\\n' \"$VALUE\"\nfi\nfor ITEM in first second; do\n  printf 'for:%s\\n' \"$ITEM\"\ndone\ncase \"$VALUE\" in\n  first) printf 'case:%s\\n' \"$VALUE\" ;;\nesac\n";
    let script = source.replace('\n', "\r\n");

    assert_eq!(engine.eval_str(&script, &mut ctx).await.unwrap(), 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "if:first\nfor:first\nfor:second\ncase:first\n"
    );
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}

#[tokio::test]
async fn command_substitution_removes_trailing_linefeeds_and_keeps_embedded_bytes() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    let script = r#"VALUE=$(printf 'first\nmiddle\r\n\n')
LEGACY=`printf 'left\nright\n\n'`
EMPTY=$(printf '')
printf '<%s>|<%s>|<%s>\n' "$VALUE" "$LEGACY" "$EMPTY"
"#;
    let expected = b"<first\nmiddle\r>|<left\nright>|<>\n";

    assert_eq!(engine.eval_str(script, &mut ctx).await.unwrap(), 0);
    assert_eq!(ctx.get_captured_stdout_bytes().unwrap(), expected);
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");

    #[cfg(unix)]
    for (shell, args) in [("bash", vec!["--posix", "-c"]), ("dash", vec!["-c"])] {
        if let Ok(reference) = std::process::Command::new(shell)
            .args(args)
            .arg(script)
            .output()
        {
            assert!(
                reference.status.success(),
                "{shell} failed command substitution"
            );
            assert_eq!(reference.stdout, expected, "{shell} substitution bytes");
        }
    }
}

#[tokio::test]
async fn test_runtime_logical_chains() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        cd /nonexistent_dir_path_98765 || echo "Fallback triggered"
        echo "Step 1" && echo "Step 2"
    "#;

    let status = engine
        .eval_str(script, &mut ctx)
        .await
        .expect("eval failed");
    assert_eq!(status, 0);

    let output = ctx.get_captured_stdout().unwrap();
    assert!(output.contains("Fallback triggered"));
    assert!(output.contains("Step 1"));
    assert!(output.contains("Step 2"));
}

#[tokio::test]
async fn standalone_ampersand_rejects_the_script_before_any_command_runs() {
    let engine = Engine::new();

    for invalid in [
        "echo background > background.marker &",
        "echo background > background.marker & echo after > after.marker",
        "; & echo after > after.marker",
    ] {
        let tmp = tempdir().unwrap();
        let mut ctx = ExecutionContext::new();
        ctx.set_cwd(tmp.path()).unwrap();
        ctx.enable_capture();

        let script = format!("echo prefix > prefix.marker\n{invalid}\n");
        let error = engine
            .eval_str(&script, &mut ctx)
            .await
            .expect_err("standalone '&' should be rejected during script compilation");

        assert!(
            error.to_string().contains("Unexpected single '&' operator"),
            "{invalid}: {error}"
        );
        assert!(!tmp.path().join("prefix.marker").exists(), "{invalid}");
        assert!(!tmp.path().join("background.marker").exists(), "{invalid}");
        assert!(!tmp.path().join("after.marker").exists(), "{invalid}");
        assert_eq!(ctx.get_captured_stdout().unwrap(), "", "{invalid}");
    }
}

#[tokio::test]
async fn cd_dash_toggles_exported_pwd_and_prints_the_destination() {
    let engine = Engine::new();
    let temp = tempdir().unwrap();
    let temp_path = std::fs::canonicalize(temp.path()).unwrap();
    let child = temp_path.join("child");
    std::fs::create_dir(&child).unwrap();

    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(&temp_path).unwrap();
    ctx.enable_capture();

    let script = "cd child\nprintf '<%s>\\n' \"$PWD\"\ncd -\nprintf '<%s>\\n' \"$OLDPWD\"\ncd -\n";
    let status = engine.eval_str(script, &mut ctx).await.unwrap();

    let temp_path = common::presented_path(&temp_path);
    let child_path = common::presented_path(&child);
    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        format!("<{child_path}>\n{temp_path}\n<{child_path}>\n{child_path}\n")
    );
    assert_eq!(ctx.cwd(), child.as_path());
    assert_eq!(ctx.get_var("PWD").as_deref(), Some(child_path.as_str()));
    assert_eq!(ctx.get_var("OLDPWD").as_deref(), Some(temp_path.as_str()));
    assert!(ctx.env_vars().contains_key("PWD"));
    assert!(ctx.env_vars().contains_key("OLDPWD"));
}

#[tokio::test]
async fn prefixed_pwd_assignments_do_not_stale_cd_state() {
    let engine = Engine::new();
    let temp = tempdir().unwrap();
    let temp_path = std::fs::canonicalize(temp.path()).unwrap();
    let child = temp_path.join("child");
    std::fs::create_dir(&child).unwrap();

    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(&temp_path).unwrap();
    ctx.set_var("PWD", temp_path.to_string_lossy().as_ref());
    ctx.set_var("OLDPWD", "/previous");
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "PWD=/temporary OLDPWD=/temporary cd child\nprintf '%s:%s\\n' \"$PWD\" \"$OLDPWD\"\n",
            &mut ctx,
        )
        .await
        .unwrap();

    let child_path = std::fs::canonicalize(child).unwrap();
    assert_eq!(status, 0);
    assert_eq!(ctx.cwd(), child_path.as_path());
    assert_eq!(
        ctx.get_var("PWD").as_deref(),
        Some(common::presented_path(&child_path).as_str())
    );
    assert_eq!(
        ctx.get_var("OLDPWD").as_deref(),
        Some(common::presented_path(&temp_path).as_str())
    );
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        format!(
            "{}:{}\n",
            common::presented_path(&child_path),
            common::presented_path(&temp_path)
        )
    );
}

#[tokio::test]
async fn cd_uses_physical_pwd_when_the_shell_pwd_is_invalid() {
    let engine = Engine::new();
    let temp = tempdir().unwrap();
    let child = temp.path().join("child");
    std::fs::create_dir(&child).unwrap();

    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(temp.path()).unwrap();
    ctx.set_var("PWD", "relative/not-the-current-directory");
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "cd child\nprintf '%s:%s\\n' \"$OLDPWD\" \"$PWD\"\n",
            &mut ctx,
        )
        .await
        .unwrap();

    let physical = std::fs::canonicalize(temp.path()).unwrap();
    let child_physical = std::fs::canonicalize(child).unwrap();
    assert_eq!(status, 0);
    assert_eq!(ctx.cwd(), child_physical.as_path());
    assert_eq!(
        ctx.get_var("OLDPWD").as_deref(),
        Some(common::presented_path(&physical).as_str())
    );
    assert_eq!(
        ctx.get_var("PWD").as_deref(),
        Some(common::presented_path(&child_physical).as_str())
    );
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        format!(
            "{}:{}\n",
            common::presented_path(&physical),
            common::presented_path(&child_physical)
        )
    );
}

#[tokio::test]
async fn cd_without_operand_uses_the_shell_home_variable() {
    let engine = Engine::new();
    let temp = tempdir().unwrap();
    let temp_path = std::fs::canonicalize(temp.path()).unwrap();
    let home = temp_path.join("home");
    std::fs::create_dir(&home).unwrap();

    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(&temp_path).unwrap();
    ctx.set_var("HOME", home.to_string_lossy().as_ref());

    let status = engine.eval_str("cd", &mut ctx).await.unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.cwd(), home.as_path());
}

#[tokio::test]
async fn cd_without_home_variables_stays_in_the_execution_context_directory() {
    let engine = Engine::new();
    let temp = tempdir().unwrap();
    let temp_path = std::fs::canonicalize(temp.path()).unwrap();
    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(&temp_path).unwrap();
    ctx.unset_var("HOME");
    ctx.unset_var("USERPROFILE");

    let status = engine.eval_str("cd\ncd --\n", &mut ctx).await.unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.cwd(), temp_path.as_path());
    assert_eq!(
        ctx.get_var("PWD").as_deref(),
        Some(common::presented_path(&temp_path).as_str())
    );
    assert_eq!(
        ctx.get_var("OLDPWD").as_deref(),
        Some(common::presented_path(&temp_path).as_str())
    );
}

#[tokio::test]
async fn cd_with_empty_home_falls_back_to_nonempty_userprofile_or_current_directory() {
    let engine = Engine::new();
    let temp = tempdir().unwrap();
    let temp_path = std::fs::canonicalize(temp.path()).unwrap();
    let profile = temp_path.join("profile");
    std::fs::create_dir(&profile).unwrap();

    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(&temp_path).unwrap();
    ctx.set_var("HOME", "");
    ctx.set_var("USERPROFILE", profile.to_string_lossy().as_ref());

    assert_eq!(engine.eval_str("cd\n", &mut ctx).await.unwrap(), 0);
    assert_eq!(ctx.cwd(), profile.as_path());

    ctx.set_cwd(&temp_path).unwrap();
    ctx.set_var("HOME", "");
    ctx.unset_var("USERPROFILE");
    assert_eq!(engine.eval_str("cd --\n", &mut ctx).await.unwrap(), 0);
    assert_eq!(ctx.cwd(), temp_path.as_path());
}

#[tokio::test]
async fn cd_double_dash_ends_options_and_alone_selects_home() {
    let engine = Engine::new();
    let temp = tempdir().unwrap();
    let temp_path = std::fs::canonicalize(temp.path()).unwrap();
    let home = temp_path.join("home");
    let dash_directory = temp_path.join("-dash");
    let literal_dash_directory = dash_directory.join("-");
    std::fs::create_dir(&home).unwrap();
    std::fs::create_dir(&dash_directory).unwrap();
    std::fs::create_dir(&literal_dash_directory).unwrap();

    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(&temp_path).unwrap();
    ctx.set_var("HOME", home.to_string_lossy().as_ref());
    ctx.enable_capture();

    let status = engine
        .eval_str("cd -- -dash\nFIRST=$PWD\ncd -- -\nSECOND=$PWD\ncd --\nprintf '%s:%s:%s\\n' \"$FIRST\" \"$SECOND\" \"$PWD\"\n", &mut ctx)
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.cwd(), home.as_path());
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        format!(
            "{}:{}:{}\n",
            common::presented_path(&dash_directory),
            common::presented_path(&literal_dash_directory),
            common::presented_path(&home)
        )
    );
    assert_eq!(
        ctx.get_var("OLDPWD").as_deref(),
        Some(common::presented_path(&literal_dash_directory).as_str())
    );
}

#[tokio::test]
async fn cd_rejects_an_empty_directory_operand_without_changing_directory() {
    let engine = Engine::new();
    let temp = tempdir().unwrap();
    let temp_path = std::fs::canonicalize(temp.path()).unwrap();
    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(&temp_path).unwrap();
    ctx.enable_capture();

    let status = engine.eval_str("cd ''\n", &mut ctx).await.unwrap();

    assert_ne!(status, 0);
    assert_eq!(ctx.cwd(), temp_path.as_path());
    assert!(!ctx.get_captured_stderr().unwrap().is_empty());
}

#[tokio::test]
async fn cd_searches_cdpath_in_order_and_prints_selected_directory() {
    let engine = Engine::new();
    let temp = tempdir().unwrap();
    let temp_path = std::fs::canonicalize(temp.path()).unwrap();
    let first = temp_path.join("first");
    let second = temp_path.join("second");
    std::fs::create_dir_all(first.join("shared")).unwrap();
    std::fs::create_dir_all(second.join("shared")).unwrap();

    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(&temp_path).unwrap();
    ctx.set_var("CDPATH", "first:second");
    ctx.enable_capture();

    let status = engine.eval_str("cd shared\n", &mut ctx).await.unwrap();

    let first_shared = first.join("shared");
    assert_eq!(status, 0);
    assert_eq!(ctx.cwd(), first_shared.as_path());
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        format!("{}\n", common::presented_path(&first_shared))
    );
    assert_eq!(
        ctx.get_var("OLDPWD").as_deref(),
        Some(common::presented_path(&temp_path).as_str())
    );
}

#[tokio::test]
async fn cd_uses_current_directory_fallback_and_skips_cdpath_for_explicit_paths() {
    let engine = Engine::new();
    let temp = tempdir().unwrap();
    let temp_path = std::fs::canonicalize(temp.path()).unwrap();
    let local = temp_path.join("local");
    let local_child = local.join("child");
    let search = temp_path.join("search");
    std::fs::create_dir_all(&local_child).unwrap();
    std::fs::create_dir_all(search.join("child")).unwrap();

    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(&temp_path).unwrap();
    ctx.set_var("CDPATH", "search");
    ctx.enable_capture();

    let status = engine
        .eval_str("cd local\ncd ./child\n", &mut ctx)
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.cwd(), local_child.as_path());
    assert_eq!(ctx.get_captured_stdout().unwrap(), "");
}

#[tokio::test]
async fn cd_rejects_missing_oldpwd_and_extra_operands_without_changing_directory() {
    let engine = Engine::new();
    let temp = tempdir().unwrap();
    let temp_path = std::fs::canonicalize(temp.path()).unwrap();
    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(&temp_path).unwrap();
    ctx.unset_var("OLDPWD");
    ctx.enable_capture();

    let missing_oldpwd = engine.eval_str("cd -", &mut ctx).await.unwrap();
    assert_eq!(missing_oldpwd, 1);
    assert_eq!(ctx.cwd(), temp_path.as_path());

    let extra_operands = engine.eval_str("cd one two", &mut ctx).await.unwrap();
    assert_eq!(extra_operands, 2);
    assert_eq!(ctx.cwd(), temp_path.as_path());
    assert_eq!(
        ctx.get_captured_stderr().unwrap(),
        "esh: cd: OLDPWD is not set\nesh: cd: too many operands\n"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn pwd_logical_and_physical_modes_respect_the_current_directory() {
    let engine = Engine::new();
    let temp = tempdir().unwrap();
    let temp_path = std::fs::canonicalize(temp.path()).unwrap();
    let physical = temp_path.join("real");
    let logical = temp_path.join("alias");
    std::fs::create_dir(&physical).unwrap();
    std::os::unix::fs::symlink(&physical, &logical).unwrap();

    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(&physical).unwrap();
    ctx.set_var("PWD", logical.to_string_lossy().as_ref());
    ctx.enable_capture();

    let status = engine
        .eval_str("pwd -L\npwd -P\npwd --\npwd -LP\npwd -PL\n", &mut ctx)
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        format!(
            "{}\n{}\n{}\n{}\n{}\n",
            logical.display(),
            physical.display(),
            logical.display(),
            physical.display(),
            logical.display()
        )
    );
}

#[tokio::test]
async fn pwd_logical_mode_falls_back_when_pwd_is_invalid() {
    let engine = Engine::new();
    let temp = tempdir().unwrap();
    let invalid_absolute_pwd = temp.path().join("not-current");
    let dot_pwd = format!("{}{}.", temp.path().display(), std::path::MAIN_SEPARATOR);
    let dotdot_pwd = temp
        .path()
        .join("..")
        .join(temp.path().file_name().unwrap());

    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(temp.path()).unwrap();
    ctx.enable_capture();

    ctx.set_var("PWD", "relative/path");
    assert_eq!(engine.eval_str("pwd -L\n", &mut ctx).await.unwrap(), 0);

    ctx.set_var("PWD", invalid_absolute_pwd.to_string_lossy().as_ref());
    assert_eq!(engine.eval_str("pwd -L\n", &mut ctx).await.unwrap(), 0);

    ctx.set_var("PWD", &dot_pwd);
    assert_eq!(engine.eval_str("pwd -L\n", &mut ctx).await.unwrap(), 0);

    ctx.set_var("PWD", dotdot_pwd.to_string_lossy().as_ref());
    assert_eq!(engine.eval_str("pwd -L\n", &mut ctx).await.unwrap(), 0);

    let physical = std::fs::canonicalize(temp.path()).unwrap();
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        format!(
            "{}\n{}\n{}\n{}\n",
            common::presented_path(&physical),
            common::presented_path(&physical),
            common::presented_path(&physical),
            common::presented_path(&physical)
        )
    );
}

#[tokio::test]
async fn pwd_rejects_unsupported_operands_instead_of_ignoring_them() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine.eval_str("pwd unexpected", &mut ctx).await.unwrap();

    assert_eq!(status, 2);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "");
    assert_eq!(
        ctx.get_captured_stderr().unwrap(),
        "esh: pwd: unsupported operand 'unexpected'\n"
    );
}

#[tokio::test]
async fn test_runtime_fail_fast_block_telemetry() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        {
            echo "Entering try block"
            cd /nonexistent_dir_path_98765
            echo "Unreachable line"
        } || {
            echo "Caught failure at $ESH_FAILED_CMD (exit code: $?)"
        }
    "#;

    let status = engine
        .eval_str(script, &mut ctx)
        .await
        .expect("eval failed");
    assert_eq!(status, 0);

    let output = ctx.get_captured_stdout().unwrap();
    assert!(output.contains("Entering try block"));
    assert!(!output.contains("Unreachable line"));
    assert!(output.contains("Caught failure at cd (exit code: 1)"));
}

#[tokio::test]
async fn fail_fast_brace_groups_preserve_their_surrounding_scope() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                VALUE=root
                {
                    TRY_ONLY=try
                    false
                } || {
                    FALLBACK_ONLY=fallback
                }
                set -u
                {
                    printf '%s' "$MISSING"
                } || {
                    EXPANSION_FALLBACK=visible
                }
                set +u
                printf 'try=%s fallback=%s error-fallback=%s value=%s\n' "$TRY_ONLY" "$FALLBACK_ONLY" "$EXPANSION_FALLBACK" "$VALUE"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "try=try fallback=fallback error-fallback=visible value=root\n"
    );
}

#[tokio::test]
async fn test_runtime_parallel_joins() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = "echo branch1 &| echo branch2 &| echo branch3";
    let status = engine
        .eval_str(script, &mut ctx)
        .await
        .expect("eval failed");
    assert_eq!(status, 0);

    let output = ctx.get_captured_stdout().unwrap();
    assert_eq!(output, "branch1\nbranch2\nbranch3\n");
}

#[tokio::test]
async fn test_parallel_join_orders_output_and_failure_by_source_order() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        success_branch() { printf 'success-out\n'; printf 'success-err\n' >&2; }
        slow_branch() { sleep 30ms; printf 'slow-out\n'; printf 'slow-err\n' >&2; exit 7; }
        fast_branch() { printf 'fast-out\n'; printf 'fast-err\n' >&2; false; }
        last_branch() { printf 'last-out\n'; printf 'last-err\n' >&2; exit 9; }
        success_branch &| slow_branch &| fast_branch &| last_branch
        JOIN_STATUS=$?
        printf 'parent-alive:%s\n' "$JOIN_STATUS"
        exit "$JOIN_STATUS"
    "#;
    let status = engine.eval_str(script, &mut ctx).await.unwrap();

    assert_eq!(status, 7);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "success-out\nslow-out\nfast-out\nlast-out\nparent-alive:7\n"
    );
    assert_eq!(
        ctx.get_captured_stderr().unwrap(),
        "success-err\nslow-err\nfast-err\nlast-err\n"
    );
}

#[tokio::test]
async fn parallel_join_exposes_first_branch_failure_telemetry_to_fallback() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "test alpha = beta &| false || printf 'failed=%s line=%s\\n' \"${ESH_FAILED_CMD-unset}\" \"${ESH_FAILED_LINE-unset}\"\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "failed=test line=1\n");
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}

#[tokio::test]
async fn parallel_join_propagates_available_failure_message_to_fallback() {
    let engine = Engine::new();
    let temp = tempdir().unwrap();
    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(temp.path()).unwrap();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "printf x >missing/out &| false || printf 'failed=%s line=%s message=%s\\n' \"${ESH_FAILED_CMD-unset}\" \"${ESH_FAILED_LINE-unset}\" \"${ESH_FAILED_MSG-unset}\"\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    let output = ctx.get_captured_stdout().unwrap();
    assert!(
        output.starts_with("failed=printf line=1 message="),
        "{output}"
    );
    assert!(output.trim_end().len() > "failed=printf line=1 message=".len());
    let stderr = ctx.get_captured_stderr().unwrap();
    assert!(stderr.contains("missing"));
    assert!(stderr.contains("out"));
}

#[tokio::test]
async fn parallel_join_starts_all_branches_before_waiting_for_results() {
    let engine = Engine::new();
    let temp = tempdir().unwrap();
    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(temp.path()).unwrap();
    ctx.enable_capture();

    let script = r#"{
touch left.ready
sleep 500ms
if test -f right.ready; then printf 'left-ready\n'; else exit 71; fi
} &| {
touch right.ready
sleep 500ms
if test -f left.ready; then printf 'right-ready\n'; else exit 72; fi
}
"#;
    let status = engine
        .eval_str(script, &mut ctx)
        .await
        .expect("parallel rendezvous should complete");

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "left-ready\nright-ready\n"
    );
}

#[tokio::test]
async fn test_runtime_pipeline_stream() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = "echo -n 'hello piped world' | echo 'done'";
    let status = engine
        .eval_str(script, &mut ctx)
        .await
        .expect("eval failed");
    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "done\n");
}

#[cfg(unix)]
#[tokio::test]
async fn external_pipeline_streams_while_the_consumer_is_running() {
    let find_executable = |name: &str| {
        std::env::split_paths(&std::env::var_os("PATH").unwrap())
            .map(|directory| directory.join(name))
            .find(|candidate| candidate.is_file())
            .unwrap_or_else(|| panic!("could not find {name} on PATH"))
    };
    let yes = find_executable("yes");
    let head = find_executable("head");
    assert!(
        !yes.to_string_lossy().contains(char::is_whitespace)
            && !head.to_string_lossy().contains(char::is_whitespace),
        "test executables must have paths that can be written as shell words"
    );

    let tmp = tempdir().unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();
    let script = format!("{} | {} -n 1\n", yes.display(), head.display());

    let status = tokio::time::timeout(Duration::from_secs(3), engine.eval_str(&script, &mut ctx))
        .await
        .expect("the pipeline should stop its infinite producer after head exits")
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "y\n");
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}

#[cfg(unix)]
#[tokio::test]
async fn external_pipeline_streams_into_builtin_head_and_stops_producer() {
    let yes = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|directory| directory.join("yes"))
        .find(|candidate| candidate.is_file())
        .expect("could not find yes on PATH");
    assert!(!yes.to_string_lossy().contains(char::is_whitespace));

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    let script = format!("{} | head -n 1\n", yes.display());

    let status = tokio::time::timeout(Duration::from_secs(3), engine.eval_str(&script, &mut ctx))
        .await
        .expect("the builtin consumer should stop its infinite producer")
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "y\n");
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}

#[cfg(unix)]
#[tokio::test]
async fn external_pipeline_streams_into_head_in_a_compound_stage() {
    let yes = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|directory| directory.join("yes"))
        .find(|candidate| candidate.is_file())
        .expect("could not find yes on PATH");
    assert!(!yes.to_string_lossy().contains(char::is_whitespace));

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    let script = format!(
        "{} | {{ head -n 1; }}\n{} | {{ grep -q y; }}\n",
        yes.display(),
        yes.display()
    );

    let status = tokio::time::timeout(Duration::from_secs(3), engine.eval_str(&script, &mut ctx))
        .await
        .expect("the compound consumer should stop its infinite producer")
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "y\n");
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}

#[cfg(unix)]
#[tokio::test]
async fn external_pipeline_streams_through_builtin_cat() {
    let yes = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|directory| directory.join("yes"))
        .find(|candidate| candidate.is_file())
        .expect("could not find yes on PATH");
    assert!(!yes.to_string_lossy().contains(char::is_whitespace));

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    let script = format!(
        "set -o pipefail\n{} | cat | head -n 1\nSTATUS=$?\nprintf 'status=%s\\n' \"$STATUS\"\n",
        yes.display()
    );

    let status = tokio::time::timeout(Duration::from_secs(3), engine.eval_str(&script, &mut ctx))
        .await
        .expect("builtin cat should preserve the streaming pipeline")
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "y\nstatus=141\n");
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}

#[cfg(unix)]
#[tokio::test]
async fn user_function_named_cat_keeps_precedence_in_a_streaming_pipeline() {
    let printf = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|directory| directory.join("printf"))
        .find(|candidate| candidate.is_file())
        .expect("could not find printf on PATH");
    assert!(!printf.to_string_lossy().contains(char::is_whitespace));

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    let script = format!(
        "cat() {{ printf 'custom-cat\\n'; }}\n{} 'input\\n' | cat | head -n 1\n",
        printf.display()
    );

    assert_eq!(engine.eval_str(&script, &mut ctx).await.unwrap(), 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "custom-cat\n");
}

#[cfg(unix)]
#[tokio::test]
async fn external_pipeline_streams_into_quiet_builtin_grep_and_stops_producer() {
    let yes = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|directory| directory.join("yes"))
        .find(|candidate| candidate.is_file())
        .expect("could not find yes on PATH");
    assert!(!yes.to_string_lossy().contains(char::is_whitespace));

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    let script = format!(
        "{} | grep -q y\nDEFAULT=$?\nset -o pipefail\n{} | grep -q y\nPIPEFAIL=$?\nprintf '%s:%s\\n' \"$DEFAULT\" \"$PIPEFAIL\"\n",
        yes.display(),
        yes.display()
    );

    let status = tokio::time::timeout(Duration::from_secs(5), engine.eval_str(&script, &mut ctx))
        .await
        .expect("quiet grep should stop its infinite producer after a match")
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "0:141\n");
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}

#[cfg(unix)]
#[tokio::test]
async fn user_function_named_head_keeps_precedence_in_a_pipeline() {
    let printf = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|directory| directory.join("printf"))
        .find(|candidate| candidate.is_file())
        .expect("could not find printf on PATH");
    assert!(!printf.to_string_lossy().contains(char::is_whitespace));

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    let script = format!(
        "head() {{ cat; }}\n{} 'first\\nsecond\\n' | head -n 1\n",
        printf.display()
    );

    assert_eq!(engine.eval_str(&script, &mut ctx).await.unwrap(), 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "first\nsecond\n");
}

#[cfg(unix)]
#[tokio::test]
async fn streaming_external_pipeline_keeps_default_and_pipefail_status_rules() {
    let find_executable = |name: &str| {
        std::env::split_paths(&std::env::var_os("PATH").unwrap())
            .map(|directory| directory.join(name))
            .find(|candidate| candidate.is_file())
            .unwrap_or_else(|| panic!("could not find {name} on PATH"))
    };
    let false_command = find_executable("false");
    let true_command = find_executable("true");

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    let script = format!(
        "{} | {}\nDEFAULT=$?\nset -o pipefail\n{} | {}\nPIPEFAIL=$?\nprintf '%s:%s\\n' \"$DEFAULT\" \"$PIPEFAIL\"\n",
        false_command.display(),
        true_command.display(),
        false_command.display(),
        true_command.display()
    );

    assert_eq!(engine.eval_str(&script, &mut ctx).await.unwrap(), 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "0:1\n");
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}

#[cfg(unix)]
#[tokio::test]
async fn pipefail_reports_rightmost_failure_across_pipeline_execution_paths() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"set -o pipefail
exit 5 | exit 7 | true
BUILTIN_PIPELINE=$?
sh -c 'exit 5' | sh -c 'exit 7' | sh -c 'exit 0'
EXTERNAL_PIPELINE=$?
sh -c 'exit 5' | sh -c 'printf match; exit 7' | grep -q match
STREAMING_CONSUMER_PIPELINE=$?
printf '%s:%s:%s\n' "$BUILTIN_PIPELINE" "$EXTERNAL_PIPELINE" "$STREAMING_CONSUMER_PIPELINE"
"#;

    assert_eq!(engine.eval_str(script, &mut ctx).await.unwrap(), 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "7:7:7\n");
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");

    let reference = std::process::Command::new("bash")
        .args(["--posix", "-c", script])
        .output()
        .expect("Bash should be available for the pipefail comparison");
    assert!(reference.status.success());
    assert_eq!(reference.stdout, b"7:7:7\n");
    assert_eq!(reference.stderr, b"");
}

#[tokio::test]
async fn test_stderr_redirection_positional_expansion_and_command_sub_quotes() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        set --
        set -- "$@" "first arg"
        set -- "$@" "second arg"
        VAL="$(printf 'version = "1.2.3"\n' | cut -d '"' -f 2)"
        printf '%s\n' "$@" "$VAL"
        echo "diagnostic message" >&2
    "#;
    let status = engine
        .eval_str(script, &mut ctx)
        .await
        .expect("eval failed");
    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "first arg\nsecond arg\n1.2.3\n"
    );
    assert_eq!(ctx.get_captured_stderr().unwrap(), "diagnostic message\n");
}
