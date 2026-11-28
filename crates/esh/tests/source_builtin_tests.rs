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

use esh::{Engine, ExecutionContext, ScopeManager};
use std::fs;
use tempfile::tempdir;

#[tokio::test]
async fn dot_runs_in_the_current_context_with_temporary_positionals() {
    let tmp = tempdir().unwrap();
    fs::write(
        tmp.path().join("setup.esh"),
        "SOURCE_VALUE=loaded\nSOURCE_ARGS=\"$#:$1:$2\"\nreturn 7\nSOURCE_AFTER=unreachable\n",
    )
    .unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();
    ctx.set_positional_args(vec!["main.esh".into(), "outer".into()]);

    let status = engine
        .eval_str(
            ". ./setup.esh first second; printf '%s|%s|%s|%s\\n' \"$SOURCE_VALUE\" \"$SOURCE_ARGS\" \"$1\" \"${SOURCE_AFTER-unset}\"\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.positional_args_with_zero(), ["main.esh", "outer"]);
    assert_eq!(ctx.get_var("SOURCE_VALUE").as_deref(), Some("loaded"));
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "loaded|2:first:second|outer|unset\n"
    );
}

#[tokio::test]
async fn dot_sees_prefix_assignments_and_restores_them_after_sourcing() {
    let tmp = tempdir().unwrap();
    fs::write(
        tmp.path().join("observe.esh"),
        "printf 'during=%s\\n' \"$SOURCE_PREFIX\"\nSOURCE_PREFIX=changed\n",
    )
    .unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();
    ctx.set_env("SOURCE_PREFIX", "original");

    let status = engine
        .eval_str(
            "SOURCE_PREFIX=temporary . ./observe.esh; printf 'after=%s\\n' \"$SOURCE_PREFIX\"\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_var("SOURCE_PREFIX").as_deref(), Some("original"));
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "during=temporary\nafter=original\n"
    );
}

#[tokio::test]
async fn sourced_file_can_declare_a_local_in_its_calling_function() {
    let tmp = tempdir().unwrap();
    fs::write(
        tmp.path().join("declare-local.esh"),
        "printf 'sourced-before=%s\\n' \"$ESH_SOURCED_LOCAL\"\nESH_SOURCED_LOCAL=updated\n",
    )
    .unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    let status = engine
        .eval_str(
            "run() { local ESH_SOURCED_LOCAL=initial; . ./declare-local.esh; printf 'function-after=%s\\n' \"$ESH_SOURCED_LOCAL\"; }; run; printf 'outside=%s\\n' \"${ESH_SOURCED_LOCAL-unset}\"\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_var("ESH_SOURCED_LOCAL"), None);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "sourced-before=initial\nfunction-after=updated\noutside=unset\n"
    );
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}

#[tokio::test]
async fn trap_registered_by_sourced_file_persists_in_caller_context() {
    let tmp = tempdir().unwrap();
    fs::write(
        tmp.path().join("register-trap.esh"),
        "trap 'echo sourced-cleanup' EXIT\n",
    )
    .unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    let status = engine
        .eval_str(". ./register-trap.esh\necho caller\n", &mut ctx)
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "caller\nsourced-cleanup\n"
    );
    assert!(
        ctx.get_trap(esh::runtime::context::TrapSignal::Exit)
            .is_some()
    );
}

#[tokio::test]
async fn return_from_sourced_file_does_not_return_from_enclosing_function() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("stop.esh"), "return 12\n").unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();
    let status = engine
        .eval_str(
            "caller() { . ./stop.esh; echo caller-continued; return 3; }; caller; printf 'status=%s\\n' \"$?\"\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "caller-continued\nstatus=3\n"
    );
}

#[tokio::test]
async fn nested_dot_restores_each_positional_frame_and_return_boundary() {
    let tmp = tempdir().unwrap();
    fs::write(
        tmp.path().join("outer.esh"),
        "OUTER_BEFORE=\"$#:$1:$2\"\n. ./inner.esh inner-arg\nOUTER_AFTER=\"$#:$1:$2\"\nreturn 7\nOUTER_TAIL=unreachable\n",
    )
    .unwrap();
    fs::write(
        tmp.path().join("inner.esh"),
        "INNER_ARGS=\"$#:$1\"\nreturn 9\nINNER_TAIL=unreachable\n",
    )
    .unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    let script = "set -- caller-arg\n. ./outer.esh outer-arg outer-second\nprintf '%s|%s|%s|%s|%s:%s|%s|%s\\n' \"$OUTER_BEFORE\" \"$INNER_ARGS\" \"$OUTER_AFTER\" \"$?\" \"$#\" \"$1\" \"${OUTER_TAIL-unset}\" \"${INNER_TAIL-unset}\"\n";
    let status = engine.eval_str(script, &mut ctx).await.unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.positional_args_with_zero(), ["esh", "caller-arg"]);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "2:outer-arg:outer-second|1:inner-arg|2:outer-arg:outer-second|7|1:caller-arg|unset|unset\n"
    );

    #[cfg(unix)]
    if common::should_compare_against_bash()
        && let Ok(reference) = std::process::Command::new("bash")
            .args(["--posix", "-c", script])
            .current_dir(tmp.path())
            .output()
    {
        assert!(reference.status.success());
        assert_eq!(
            reference.stdout,
            b"2:outer-arg:outer-second|1:inner-arg|2:outer-arg:outer-second|7|1:caller-arg|unset|unset\n"
        );
    }
}

#[tokio::test]
async fn nonzero_return_from_sourced_file_triggers_errexit_and_exit_trap() {
    let tmp = tempdir().unwrap();
    fs::write(
        tmp.path().join("fail.esh"),
        "printf 'source-before-return\\n'\nreturn 7\nprintf 'wrong-source-tail\\n'\n",
    )
    .unwrap();

    let script = r#"trap 'printf "exit:%s\n" "$?"' EXIT
set -e
. ./fail.esh
printf 'wrong-caller-tail\n'
"#;
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    let status = engine.eval_str(script, &mut ctx).await.unwrap();

    assert_eq!(status, 7);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "source-before-return\nexit:7\n"
    );

    if common::should_compare_against_bash()
        && let Ok(reference) = std::process::Command::new("bash")
            .args(["--posix", "-c", script])
            .current_dir(tmp.path())
            .output()
    {
        assert_eq!(reference.status.code(), Some(status));
        assert_eq!(
            reference.stdout,
            ctx.get_captured_stdout().unwrap().as_bytes()
        );
        assert_eq!(reference.stderr, b"");
    }
}

#[tokio::test]
async fn dot_searches_shell_path_for_bare_file_names_even_when_unexported() {
    let tmp = tempdir().unwrap();
    let working_dir = tmp.path().join("work");
    let path_dir = tmp.path().join("source-path");
    fs::create_dir_all(&working_dir).unwrap();
    fs::create_dir_all(&path_dir).unwrap();
    fs::write(path_dir.join("env.esh"), "PATH_SOURCE=found\n").unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(&working_dir).unwrap();
    ctx.unset_var("PATH");
    let status = engine
        .eval_str(
            "PATH=../source-path\n. env.esh; printf '%s\\n' \"$PATH_SOURCE\"\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "found\n");

    if common::should_compare_against_bash()
        && let Ok(reference) = std::process::Command::new("bash")
            .args(["--posix", "-c"])
            .arg("unset PATH\nPATH=../source-path\n. env.esh\nprintf '%s\\n' \"$PATH_SOURCE\"\n")
            .current_dir(&working_dir)
            .output()
    {
        assert!(reference.status.success());
        assert_eq!(reference.stdout, b"found\n");
        assert_eq!(reference.stderr, b"");
    }
}

#[tokio::test]
async fn dot_falls_back_to_the_working_directory_after_path_misses() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("local.esh"), "LOCAL_SOURCE=found\n").unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(tmp.path()).unwrap();
    ctx.set_env("PATH", tmp.path().join("empty-path").to_string_lossy());

    let status = engine.eval_str(". local.esh\n", &mut ctx).await.unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_var("LOCAL_SOURCE").as_deref(), Some("found"));
}

#[tokio::test]
async fn dot_path_with_slash_bypasses_path_search() {
    let tmp = tempdir().unwrap();
    let working_dir = tmp.path().join("work");
    let nested_dir = working_dir.join("nested");
    let path_dir = tmp.path().join("source-path");
    fs::create_dir_all(&nested_dir).unwrap();
    fs::create_dir_all(&path_dir).unwrap();
    fs::write(nested_dir.join("same.esh"), "SOURCE_ORIGIN=explicit-path\n").unwrap();
    fs::write(path_dir.join("same.esh"), "SOURCE_ORIGIN=path-search\n").unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(&working_dir).unwrap();
    let status = engine
        .eval_str(
            "PATH=../source-path\n. ./nested/same.esh\nprintf '%s\\n' \"$SOURCE_ORIGIN\"\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "explicit-path\n");

    if common::should_compare_against_bash()
        && let Ok(reference) = std::process::Command::new("bash")
            .args(["--posix", "-c"])
            .arg("PATH=../source-path\n. ./nested/same.esh\nprintf '%s\\n' \"$SOURCE_ORIGIN\"\n")
            .current_dir(&working_dir)
            .output()
    {
        assert!(reference.status.success());
        assert_eq!(reference.stdout, b"explicit-path\n");
        assert_eq!(reference.stderr, b"");
    }
}

#[tokio::test]
async fn dot_reports_missing_operand_and_unreadable_source() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    assert_eq!(engine.eval_str(".\n", &mut ctx).await.unwrap(), 2);
    assert_eq!(
        engine.eval_str(". missing.esh\n", &mut ctx).await.unwrap(),
        1
    );
    let stderr = ctx.get_captured_stderr().unwrap();
    assert!(stderr.contains("esh: .: expected a file operand"));
    assert!(stderr.contains("esh: .: cannot read"));
}
