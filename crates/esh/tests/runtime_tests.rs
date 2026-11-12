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
use esh::{Engine, ExecutionContext};
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
sh -c 'printf "payload\n"; exit 7' | { read RESULT; }
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
async fn cd_dash_toggles_exported_pwd_and_prints_the_destination() {
    let engine = Engine::new();
    let temp = tempdir().unwrap();
    let child = temp.path().join("child");
    std::fs::create_dir(&child).unwrap();

    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(temp.path()).unwrap();
    ctx.enable_capture();

    let script = "cd child\nprintf '<%s>\\n' \"$PWD\"\ncd -\nprintf '<%s>\\n' \"$OLDPWD\"\ncd -\n";
    let status = engine.eval_str(script, &mut ctx).await.unwrap();

    let temp_path = temp.path().to_string_lossy();
    let child_path = child.to_string_lossy();
    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        format!("<{child_path}>\n{temp_path}\n<{child_path}>\n{child_path}\n")
    );
    assert_eq!(ctx.cwd(), child.as_path());
    assert_eq!(ctx.get_var("PWD").as_deref(), Some(child_path.as_ref()));
    assert_eq!(ctx.get_var("OLDPWD").as_deref(), Some(temp_path.as_ref()));
    assert!(ctx.env_vars().contains_key("PWD"));
    assert!(ctx.env_vars().contains_key("OLDPWD"));
}

#[tokio::test]
async fn prefixed_pwd_assignments_do_not_stale_cd_state() {
    let engine = Engine::new();
    let temp = tempdir().unwrap();
    let child = temp.path().join("child");
    std::fs::create_dir(&child).unwrap();

    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(temp.path()).unwrap();
    ctx.set_var("PWD", temp.path().to_string_lossy().as_ref());
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
    assert_eq!(ctx.get_var("PWD").as_deref(), child_path.to_str());
    assert_eq!(ctx.get_var("OLDPWD").as_deref(), temp.path().to_str());
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        format!("{}:{}\n", child_path.display(), temp.path().display())
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
    assert_eq!(ctx.get_var("OLDPWD").as_deref(), physical.to_str());
    assert_eq!(ctx.get_var("PWD").as_deref(), child_physical.to_str());
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        format!("{}:{}\n", physical.display(), child_physical.display())
    );
}

#[tokio::test]
async fn cd_without_operand_uses_the_shell_home_variable() {
    let engine = Engine::new();
    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    std::fs::create_dir(&home).unwrap();

    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(temp.path()).unwrap();
    ctx.set_var("HOME", home.to_string_lossy().as_ref());

    let status = engine.eval_str("cd", &mut ctx).await.unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.cwd(), home.as_path());
}

#[tokio::test]
async fn cd_without_home_variables_stays_in_the_execution_context_directory() {
    let engine = Engine::new();
    let temp = tempdir().unwrap();
    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(temp.path()).unwrap();
    ctx.unset_var("HOME");
    ctx.unset_var("USERPROFILE");

    let status = engine.eval_str("cd\ncd --\n", &mut ctx).await.unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.cwd(), temp.path());
    assert_eq!(ctx.get_var("PWD").as_deref(), temp.path().to_str());
    assert_eq!(ctx.get_var("OLDPWD").as_deref(), temp.path().to_str());
}

#[tokio::test]
async fn cd_with_empty_home_falls_back_to_nonempty_userprofile_or_current_directory() {
    let engine = Engine::new();
    let temp = tempdir().unwrap();
    let profile = temp.path().join("profile");
    std::fs::create_dir(&profile).unwrap();

    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(temp.path()).unwrap();
    ctx.set_var("HOME", "");
    ctx.set_var("USERPROFILE", profile.to_string_lossy().as_ref());

    assert_eq!(engine.eval_str("cd\n", &mut ctx).await.unwrap(), 0);
    assert_eq!(ctx.cwd(), profile.as_path());

    ctx.set_cwd(temp.path()).unwrap();
    ctx.set_var("HOME", "");
    ctx.unset_var("USERPROFILE");
    assert_eq!(engine.eval_str("cd --\n", &mut ctx).await.unwrap(), 0);
    assert_eq!(ctx.cwd(), temp.path());
}

#[tokio::test]
async fn cd_double_dash_ends_options_and_alone_selects_home() {
    let engine = Engine::new();
    let temp = tempdir().unwrap();
    let home = temp.path().join("home");
    let dash_directory = temp.path().join("-dash");
    let literal_dash_directory = dash_directory.join("-");
    std::fs::create_dir(&home).unwrap();
    std::fs::create_dir(&dash_directory).unwrap();
    std::fs::create_dir(&literal_dash_directory).unwrap();

    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(temp.path()).unwrap();
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
            dash_directory.display(),
            literal_dash_directory.display(),
            home.display()
        )
    );
    assert_eq!(
        ctx.get_var("OLDPWD").as_deref(),
        Some(literal_dash_directory.to_string_lossy().as_ref())
    );
}

#[tokio::test]
async fn cd_rejects_an_empty_directory_operand_without_changing_directory() {
    let engine = Engine::new();
    let temp = tempdir().unwrap();
    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(temp.path()).unwrap();
    ctx.enable_capture();

    let status = engine.eval_str("cd ''\n", &mut ctx).await.unwrap();

    assert_ne!(status, 0);
    assert_eq!(ctx.cwd(), temp.path());
    assert!(!ctx.get_captured_stderr().unwrap().is_empty());
}

#[tokio::test]
async fn cd_searches_cdpath_in_order_and_prints_selected_directory() {
    let engine = Engine::new();
    let temp = tempdir().unwrap();
    let first = temp.path().join("first");
    let second = temp.path().join("second");
    std::fs::create_dir_all(first.join("shared")).unwrap();
    std::fs::create_dir_all(second.join("shared")).unwrap();

    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(temp.path()).unwrap();
    ctx.set_var("CDPATH", "first:second");
    ctx.enable_capture();

    let status = engine.eval_str("cd shared\n", &mut ctx).await.unwrap();

    let first_shared = first.join("shared");
    assert_eq!(status, 0);
    assert_eq!(ctx.cwd(), first_shared.as_path());
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        format!("{}\n", first_shared.display())
    );
    assert_eq!(
        ctx.get_var("OLDPWD").as_deref(),
        Some(temp.path().to_str().unwrap())
    );
}

#[tokio::test]
async fn cd_uses_current_directory_fallback_and_skips_cdpath_for_explicit_paths() {
    let engine = Engine::new();
    let temp = tempdir().unwrap();
    let local = temp.path().join("local");
    let local_child = local.join("child");
    let search = temp.path().join("search");
    std::fs::create_dir_all(&local_child).unwrap();
    std::fs::create_dir_all(search.join("child")).unwrap();

    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(temp.path()).unwrap();
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
    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(temp.path()).unwrap();
    ctx.unset_var("OLDPWD");
    ctx.enable_capture();

    let missing_oldpwd = engine.eval_str("cd -", &mut ctx).await.unwrap();
    assert_eq!(missing_oldpwd, 1);
    assert_eq!(ctx.cwd(), temp.path());

    let extra_operands = engine.eval_str("cd one two", &mut ctx).await.unwrap();
    assert_eq!(extra_operands, 2);
    assert_eq!(ctx.cwd(), temp.path());
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
    let physical = temp.path().join("real");
    let logical = temp.path().join("alias");
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
            physical.display(),
            physical.display(),
            physical.display(),
            physical.display()
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
    assert!(output.contains("branch1"));
    assert!(output.contains("branch2"));
    assert!(output.contains("branch3"));
}

#[tokio::test]
async fn test_parallel_join_orders_output_and_failure_by_source_order() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        slow_branch() { sleep 30ms; printf 'slow-out\n'; printf 'slow-err\n' >&2; exit 7; }
        fast_branch() { printf 'fast-out\n'; printf 'fast-err\n' >&2; false; }
        last_branch() { printf 'last-out\n'; printf 'last-err\n' >&2; exit 9; }
        slow_branch &| fast_branch &| last_branch
    "#;
    let status = engine.eval_str(script, &mut ctx).await.unwrap();

    assert_eq!(status, 7);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "slow-out\nfast-out\nlast-out\n"
    );
    assert_eq!(
        ctx.get_captured_stderr().unwrap(),
        "slow-err\nfast-err\nlast-err\n"
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
