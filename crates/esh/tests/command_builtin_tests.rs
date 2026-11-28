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

mod common;

use esh::{Engine, ExecutionContext};

#[tokio::test]
async fn command_runs_builtin_without_invoking_shadowing_function() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "echo() { printf 'function:%s\\n' \"$1\"; }\ncommand echo builtin\necho ordinary\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "builtin\nfunction:ordinary\n"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn true_and_false_builtins_take_precedence_over_same_named_path_executables() {
    use std::os::unix::fs::PermissionsExt;

    let engine = Engine::new();
    let temp = tempfile::tempdir().unwrap();
    for (name, marker) in [("true", "external-true"), ("false", "external-false")] {
        let executable = temp.path().join(name);
        std::fs::write(
            &executable,
            format!("#!/bin/sh\nprintf '%s\\n' '{marker}'\nexit 19\n"),
        )
        .unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let mut ctx = ExecutionContext::new();
    ctx.set_env("PATH", temp.path().to_string_lossy().into_owned());
    ctx.enable_capture();
    let status = engine
        .eval_str(
            "true ignored operands\nTRUE_STATUS=$?\nfalse ignored operands\nFALSE_STATUS=$?\ntype true false\nprintf 'statuses=%s:%s\\n' \"$TRUE_STATUS\" \"$FALSE_STATUS\"\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "true is an ESH builtin\nfalse is an ESH builtin\nstatuses=0:1\n"
    );
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}

#[tokio::test]
async fn alias_and_unalias_do_not_rewrite_later_command_words() {
    let engine = Engine::new();
    let temp = tempfile::tempdir().unwrap();
    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(temp.path()).unwrap();
    ctx.set_env("PATH", temp.path().to_string_lossy().into_owned());
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "alias ll='printf alias-output'\nll\nunalias ll\nll\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 127);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "");
    assert_eq!(
        ctx.get_captured_stderr().unwrap(),
        concat!(
            "esh: alias: command not found\n",
            "esh: ll: command not found\n",
            "esh: unalias: command not found\n",
            "esh: ll: command not found\n",
        )
    );

    #[cfg(unix)]
    {
        let reference = std::process::Command::new("bash")
            .args(["--posix", "-c"])
            .arg("shopt -s expand_aliases\nalias ll='printf alias-output'\nll\n")
            .output()
            .expect("Bash should be available for the alias comparison");
        assert!(reference.status.success(), "Bash did not expand the alias");
        assert_eq!(reference.stdout, b"alias-output");
        assert_eq!(reference.stderr, b"");
    }
}

#[tokio::test]
async fn command_v_and_big_v_report_builtin_resolution() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str("command -v printf\ncommand -V printf\n", &mut ctx)
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "printf\nprintf is an ESH builtin\n"
    );
}

#[tokio::test]
async fn command_queries_recognize_reserved_words_like_posix_shells() {
    let script = "command -v if\ncommand -V if\ncommand -v '!'\ncommand -V '!'\n";
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    assert_eq!(engine.eval_str(script, &mut ctx).await.unwrap(), 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "if\nif is a reserved word\n!\n! is a reserved word\n"
    );

    if common::should_compare_against_bash()
        && let Ok(reference) = std::process::Command::new("bash")
            .args(["--posix", "-c", "command -v if; command -v '!'"])
            .output()
    {
        assert!(reference.status.success());
        assert_eq!(reference.stdout, b"if\n!\n");
        assert_eq!(reference.stderr, b"");
    }
}

#[tokio::test]
async fn command_and_type_without_operands_succeed_without_output() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "command\nCOMMAND_STATUS=$?\ncommand -v\nBRIEF_STATUS=$?\ncommand -V\nVERBOSE_STATUS=$?\ntype\nTYPE_STATUS=$?\nprintf '%s:%s:%s:%s\\n' \"$COMMAND_STATUS\" \"$BRIEF_STATUS\" \"$VERBOSE_STATUS\" \"$TYPE_STATUS\"\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "0:0:0:0\n");
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}

#[tokio::test]
async fn type_describes_keywords_functions_builtins_and_external_commands() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str("echo() { :; }\ntype if echo printf '!' '{' '}'\n", &mut ctx)
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "if is a reserved word\necho is a shell function\nprintf is an ESH builtin\n! is a reserved word\n{ is a reserved word\n} is a reserved word\n"
    );
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}

#[cfg(unix)]
#[tokio::test]
async fn type_uses_shell_path_and_reports_unknown_names() {
    use std::os::unix::fs::PermissionsExt;

    let engine = Engine::new();
    let temp = tempfile::tempdir().unwrap();
    let tool = temp.path().join("external-tool");
    std::fs::write(&tool, "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();

    let mut ctx = ExecutionContext::new();
    ctx.set_env("PATH", temp.path().to_string_lossy().into_owned());
    ctx.enable_capture();

    assert_eq!(
        engine
            .eval_str("type external-tool missing-tool", &mut ctx)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        format!("external-tool is {}\n", tool.display())
    );
    assert_eq!(
        ctx.get_captured_stderr().unwrap(),
        "esh: type: missing-tool: not found\n"
    );
}

#[tokio::test]
async fn command_returns_failure_for_unknown_names_and_conflicting_queries() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let query_status = engine
        .eval_str("command -v esh_missing_command_12345", &mut ctx)
        .await
        .unwrap();
    assert_eq!(query_status, 1);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "");

    let option_status = engine
        .eval_str("command -v -V echo", &mut ctx)
        .await
        .unwrap();
    assert_eq!(option_status, 2);
    assert_eq!(
        ctx.get_captured_stderr().unwrap(),
        "esh: command: -v and -V cannot be combined\n"
    );
}

#[tokio::test]
async fn command_reports_not_found_as_127() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str("command esh_missing_command_12345", &mut ctx)
        .await
        .unwrap();

    assert_eq!(status, 127);
    assert_eq!(
        ctx.get_captured_stderr().unwrap(),
        "esh: command: esh_missing_command_12345: not found\n"
    );
}

#[tokio::test]
async fn missing_commands_return_127_for_fallbacks_and_errexit() {
    let engine = Engine::new();
    let mut fallback_ctx = ExecutionContext::new();
    fallback_ctx.enable_capture();

    let fallback_status = engine
        .eval_str(
            "esh_missing_command_12345 || printf 'fallback\\n'\n",
            &mut fallback_ctx,
        )
        .await
        .unwrap();

    assert_eq!(fallback_status, 0);
    assert_eq!(fallback_ctx.get_captured_stdout().unwrap(), "fallback\n");
    assert_eq!(
        fallback_ctx.get_captured_stderr().unwrap(),
        "esh: esh_missing_command_12345: command not found\n"
    );

    let mut errexit_ctx = ExecutionContext::new();
    errexit_ctx.enable_capture();
    let errexit_status = engine
        .eval_str(
            "set -e\nesh_missing_command_12345\nprintf 'unreachable\\n'\n",
            &mut errexit_ctx,
        )
        .await
        .unwrap();

    assert_eq!(errexit_status, 127);
    assert!(errexit_ctx.is_exiting());
    assert_eq!(errexit_ctx.get_captured_stdout().unwrap(), "");
    assert_eq!(
        errexit_ctx.get_captured_stderr().unwrap(),
        "esh: esh_missing_command_12345: command not found\n"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn command_distinguishes_non_executable_files_from_missing_commands() {
    use std::os::unix::fs::PermissionsExt;

    let engine = Engine::new();
    let temp = tempfile::tempdir().unwrap();
    let blocked = temp.path().join("blocked-command");
    std::fs::write(&blocked, "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(&blocked, std::fs::Permissions::from_mode(0o644)).unwrap();

    let mut ctx = ExecutionContext::new();
    ctx.set_env("PATH", temp.path().to_string_lossy().into_owned());
    ctx.enable_capture();

    let lookup_status = engine
        .eval_str("command -v blocked-command", &mut ctx)
        .await
        .unwrap();
    assert_eq!(lookup_status, 1);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "");

    let execute_status = engine
        .eval_str("command blocked-command", &mut ctx)
        .await
        .unwrap();
    assert_eq!(execute_status, 126);
    assert_eq!(
        ctx.get_captured_stderr().unwrap(),
        "esh: command: blocked-command: permission denied\n"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn command_resolves_relative_paths_from_the_esh_working_directory() {
    use std::os::unix::fs::PermissionsExt;

    let engine = Engine::new();
    let temp = tempfile::tempdir().unwrap();
    let bin = temp.path().join("bin");
    std::fs::create_dir(&bin).unwrap();

    let explicit = temp.path().join("explicit-tool");
    std::fs::write(&explicit, "#!/bin/sh\nprintf explicit\nprintf '\\n'\n").unwrap();
    std::fs::set_permissions(&explicit, std::fs::Permissions::from_mode(0o755)).unwrap();

    let path_tool = bin.join("path-tool");
    std::fs::write(&path_tool, "#!/bin/sh\nprintf path\nprintf '\\n'\n").unwrap();
    std::fs::set_permissions(&path_tool, std::fs::Permissions::from_mode(0o755)).unwrap();

    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(temp.path()).unwrap();
    ctx.set_env("PATH", "bin");
    ctx.enable_capture();

    let status = engine
        .eval_str("command ./explicit-tool\ncommand path-tool\n", &mut ctx)
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "explicit\npath\n");
}

#[cfg(unix)]
#[tokio::test]
async fn command_search_uses_unexported_shell_path_assignments() {
    use std::os::unix::fs::PermissionsExt;

    let engine = Engine::new();
    let temp = tempfile::tempdir().unwrap();
    let bin = temp.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    let tool = bin.join("shell-path-tool");
    std::fs::write(&tool, "#!/bin/sh\nprintf 'ran\\n'\n").unwrap();
    std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
    let canonical_tool = std::fs::canonicalize(&tool).unwrap();

    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(temp.path()).unwrap();
    ctx.unset_var("PATH");
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "PATH=bin\ncommand -v shell-path-tool\nshell-path-tool\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        format!("{}\nran\n", canonical_tool.display())
    );
    assert_eq!(ctx.get_captured_stderr().unwrap_or_default(), "");

    let reference = std::process::Command::new("bash")
        .args(["--posix", "-c"])
        .arg("unset PATH\nPATH=bin\ncommand -v shell-path-tool >/dev/null\nshell-path-tool\n")
        .current_dir(temp.path())
        .output();
    if let Ok(reference) = reference {
        assert!(reference.status.success());
        assert_eq!(reference.stdout, b"ran\n");
        assert_eq!(reference.stderr, b"");
    }
}

#[cfg(unix)]
#[tokio::test]
async fn hash_does_not_cache_command_locations_after_path_changes() {
    use std::os::unix::fs::PermissionsExt;

    let engine = Engine::new();
    let temp = tempfile::tempdir().unwrap();
    let first_bin = temp.path().join("first");
    let second_bin = temp.path().join("second");
    std::fs::create_dir(&first_bin).unwrap();
    std::fs::create_dir(&second_bin).unwrap();

    for (directory, marker) in [(&first_bin, "first-path"), (&second_bin, "second-path")] {
        let tool = directory.join("path-tool");
        std::fs::write(&tool, format!("#!/bin/sh\nprintf '%s\\n' '{marker}'\n")).unwrap();
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    let first_path = first_bin.display();
    let second_path = second_bin.display();
    let script = format!("PATH='{first_path}'\nhash path-tool\nPATH='{second_path}'\npath-tool\n");
    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(temp.path()).unwrap();
    ctx.set_env("PATH", first_bin.to_string_lossy().into_owned());
    ctx.enable_capture();

    assert_eq!(engine.eval_str(&script, &mut ctx).await.unwrap(), 0);
    let expected = b"second-path\n";
    assert_eq!(ctx.get_captured_stdout().unwrap().as_bytes(), expected);
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");

    for (shell, args) in [("bash", vec!["--posix", "-c"]), ("dash", vec!["-c"])] {
        if let Ok(reference) = std::process::Command::new(shell)
            .args(args)
            .arg(&script)
            .current_dir(temp.path())
            .output()
        {
            assert!(
                reference.status.success(),
                "{shell} failed to resolve path-tool"
            );
            assert_eq!(reference.stdout, expected, "{shell} command cache result");
        }
    }
}

#[cfg(unix)]
#[tokio::test]
async fn command_can_invoke_an_external_command_without_a_shadowing_function() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "sh() { printf 'function\\n'; }\ncommand -p sh -c 'printf external'\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "external");
}
