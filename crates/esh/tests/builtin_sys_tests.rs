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

use esh::{Engine, ExecutionContext};
#[cfg(unix)]
use std::process::Command;

#[tokio::test]
async fn test_os_and_uname() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        os -n
        os -m
        uname -s
    "#;

    let code = engine.eval_str(script, &mut ctx).await.unwrap();
    assert_eq!(code, 0);
    let output = ctx.get_captured_stdout().unwrap();
    assert!(output.contains(std::env::consts::OS));
    assert!(output.contains(std::env::consts::ARCH));
}

#[cfg(unix)]
#[tokio::test]
async fn uname_matches_host_utility_for_posix_fields_and_option_combinations() {
    let engine = Engine::new();
    for flags in [
        vec![],
        vec!["-s"],
        vec!["-n"],
        vec!["-r"],
        vec!["-v"],
        vec!["-m"],
        vec!["-snrv"],
        vec!["-a"],
    ] {
        let expected_flags = if flags == ["-a"] {
            vec!["-snrvm"]
        } else {
            flags.clone()
        };
        let expected = Command::new("uname")
            .args(&expected_flags)
            .output()
            .unwrap();
        assert!(expected.status.success());

        let script = if flags.is_empty() {
            "uname".to_owned()
        } else {
            format!("uname {}", flags.join(" "))
        };
        let mut ctx = ExecutionContext::new();
        ctx.enable_capture();
        assert_eq!(
            engine.eval_str(&script, &mut ctx).await.unwrap(),
            0,
            "{script}"
        );
        assert_eq!(
            ctx.get_captured_stdout_bytes().unwrap(),
            expected.stdout,
            "{script}"
        );
        assert_eq!(ctx.get_captured_stderr().unwrap(), "", "{script}");
    }

    for script in ["uname -x", "uname extra"] {
        let mut ctx = ExecutionContext::new();
        ctx.enable_capture();
        assert_eq!(
            engine.eval_str(script, &mut ctx).await.unwrap(),
            2,
            "{script}"
        );
        assert_eq!(ctx.get_captured_stdout().unwrap(), "", "{script}");
        assert_ne!(ctx.get_captured_stderr().unwrap(), "", "{script}");
    }
}

#[tokio::test]
async fn test_env_and_which() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_env("MY_TEST_KEY", "test_value_123");

    let script = r#"
        env | grep "MY_TEST_KEY"
    "#;

    let code = engine.eval_str(script, &mut ctx).await.unwrap();
    assert_eq!(code, 0);
    let output = ctx.get_captured_stdout().unwrap();
    assert!(output.contains("MY_TEST_KEY=test_value_123"));
}

#[tokio::test]
async fn test_env_options_and_literal_command_arguments() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_env("ESH_ENV_TEST", "original");

    let code = engine
        .eval_str(
            "env -u ESH_ENV_TEST ESH_ENV_TEST=changed printf '%s\\n' 'literal; exit'\nenv -u ESH_ENV_TEST env -i\nenv -i\nprintf() { echo function-shadow; }\nenv printf '%s\\n' 'builtin bypasses function'",
            &mut ctx,
        )
        .await
        .unwrap();
    assert_eq!(code, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "literal; exit\nbuiltin bypasses function\n"
    );
    assert_eq!(
        ctx.env_vars().get("ESH_ENV_TEST").map(String::as_str),
        Some("original")
    );
}

#[cfg(unix)]
#[tokio::test]
async fn env_external_utility_operand_bypasses_shell_function_lookup() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "sh() { echo function-shadow; }\nenv sh -c 'printf \"external\\\\n\"'\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "external\n");
}

#[tokio::test]
async fn test_env_reports_modified_environment_and_usage_errors() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_env("ESH_ENV_TEST", "present");

    let code = engine
        .eval_str("env -u ESH_ENV_TEST ESH_NEW=value", &mut ctx)
        .await
        .unwrap();
    assert_eq!(code, 0);
    let output = ctx.get_captured_stdout().unwrap();
    assert!(output.contains("ESH_NEW=value\n"));
    assert!(!output.contains("ESH_ENV_TEST="));

    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    assert_eq!(engine.eval_str("env -u", &mut ctx).await.unwrap(), 2);
    assert_eq!(engine.eval_str("env --unknown", &mut ctx).await.unwrap(), 2);
}
