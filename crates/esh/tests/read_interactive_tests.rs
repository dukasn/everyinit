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

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use esh::planner::compiler::compile_script;
use esh::runtime::context::{ExecutionContext, ShellValue};
use esh::runtime::traits::{Executable, ScopeManager};

fn make_key(code: KeyCode) -> Event {
    Event::Key(KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    })
}

fn make_ctrl_c() -> Event {
    Event::Key(KeyEvent {
        code: KeyCode::Char('c'),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    })
}

#[tokio::test]
async fn test_read_prompt_writes_stderr_only() {
    let script = r#"
        read -p "Enter your name: " NAME <<< "Alice"
    "#;

    let plan = compile_script(script).expect("compile");
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    plan.execute(&mut ctx).await.expect("execute");

    assert_eq!(ctx.get_var("NAME").as_deref(), Some("Alice"));
    let stdout = ctx.get_captured_stdout().unwrap_or_default();
    let stderr = ctx.get_captured_stderr().unwrap_or_default();

    assert!(stdout.is_empty(), "Prompt must not write to stdout");
    assert!(
        stderr.contains("Enter your name: "),
        "Prompt must write to stderr"
    );
}

#[tokio::test]
async fn test_read_default_and_silent_rejection() {
    // Normal read with default fallback on empty line
    let script1 = r#"
        read --default "production" ENV <<< ""
    "#;
    let plan1 = compile_script(script1).expect("compile");
    let mut ctx1 = ExecutionContext::new();
    plan1.execute(&mut ctx1).await.expect("execute");
    assert_eq!(ctx1.get_var("ENV").as_deref(), Some("production"));

    // Rejection of --default with --silent
    let script2 = r#"
        read -s --default "secret123" SECRET
    "#;
    let plan2 = compile_script(script2).expect("compile");
    let mut ctx2 = ExecutionContext::new();
    let res = plan2.execute(&mut ctx2).await.unwrap();
    assert_eq!(res, 2, "--default with --silent must return status 2");
}

#[tokio::test]
async fn read_default_is_used_when_stdin_is_already_at_eof() {
    let plan = compile_script("read --default fallback VALUE").expect("compile");
    let mut ctx = ExecutionContext::new();
    ctx.set_stdin(Vec::new());

    let status = plan.execute(&mut ctx).await.unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_var("VALUE").as_deref(), Some("fallback"));
}

#[tokio::test]
async fn test_read_secret_input() {
    let script = r#"
        read -s -m "*" API_KEY
    "#;
    let plan = compile_script(script).expect("compile");
    let mut ctx = ExecutionContext::new();
    ctx.set_interactive_chars("super-secret-pass\n");

    let status = plan.execute(&mut ctx).await.unwrap();
    assert_eq!(status, 0);
    assert_eq!(ctx.get_var("API_KEY").as_deref(), Some("super-secret-pass"));
}

#[tokio::test]
async fn test_read_confirm_mode() {
    // 1. Confirm with 'y' -> status 0, value "yes"
    let script1 = r#"
        read -c CONFIRMED
    "#;
    let plan1 = compile_script(script1).expect("compile");
    let mut ctx1 = ExecutionContext::new();
    ctx1.set_interactive_chars("y");
    let status1 = plan1.execute(&mut ctx1).await.unwrap();
    assert_eq!(status1, 0);
    assert_eq!(ctx1.get_var("CONFIRMED").as_deref(), Some("yes"));

    // 2. Confirm with 'n' -> status 1, value "no"
    let mut ctx2 = ExecutionContext::new();
    ctx2.set_interactive_chars("n");
    let status2 = plan1.execute(&mut ctx2).await.unwrap();
    assert_eq!(status2, 1);
    assert_eq!(ctx2.get_var("CONFIRMED").as_deref(), Some("no"));

    // 3. Confirm with default yes
    let script3 = r#"
        read -c --default yes PROCEED
    "#;
    let plan3 = compile_script(script3).expect("compile");
    let mut ctx3 = ExecutionContext::new();
    ctx3.set_interactive_chars("\n");
    let status3 = plan3.execute(&mut ctx3).await.unwrap();
    assert_eq!(status3, 0);
    assert_eq!(ctx3.get_var("PROCEED").as_deref(), Some("yes"));
}

#[tokio::test]
async fn read_confirm_cancellation_returns_130_without_assigning_target() {
    let plan = compile_script("TARGET=original\nread -c TARGET\n").expect("compile");
    let mut ctx = ExecutionContext::new();
    ctx.set_interactive_events(vec![make_ctrl_c()]);

    let status = plan.execute(&mut ctx).await.unwrap();

    assert_eq!(status, 130);
    assert_eq!(ctx.get_var("TARGET").as_deref(), Some("original"));
}

#[tokio::test]
async fn test_read_single_menu_inline_and_map() {
    // 1. Inline menu
    let script1 = r#"
        read --menu 'us="United States" eu="Europe" ap="Asia Pacific"' REGION
    "#;
    let plan1 = compile_script(script1).expect("compile");
    let mut ctx1 = ExecutionContext::new();
    // Down arrow then Enter -> selects 'eu'
    ctx1.set_interactive_events(vec![make_key(KeyCode::Down), make_key(KeyCode::Enter)]);
    let status1 = plan1.execute(&mut ctx1).await.unwrap();
    assert_eq!(status1, 0);
    assert_eq!(ctx1.get_var("REGION").as_deref(), Some("eu"));

    // 2. Map variable menu (sorted by key)
    let script2 = r#"
        declare -A REGIONS
        REGIONS[b_eu]="Europe"
        REGIONS[a_us]="United States"
        REGIONS[c_ap]="Asia Pacific"
        read --menu REGIONS CHOSEN
    "#;
    let plan2 = compile_script(script2).expect("compile");
    let mut ctx2 = ExecutionContext::new();
    // Sorted keys are [a_us, b_eu, c_ap]. Down -> b_eu
    ctx2.set_interactive_events(vec![make_key(KeyCode::Down), make_key(KeyCode::Enter)]);
    let status2 = plan2.execute(&mut ctx2).await.unwrap();
    assert_eq!(status2, 0);
    assert_eq!(ctx2.get_var("CHOSEN").as_deref(), Some("b_eu"));
}

#[tokio::test]
async fn test_read_multi_menu_and_array_target() {
    let script = r#"
        declare -a SERVICES
        SERVICES[0]="auth"
        SERVICES[1]="database"
        SERVICES[2]="cache"
        read -a SELECTED_SERVICES --menu SERVICES
    "#;

    let plan = compile_script(script).expect("compile");
    let mut ctx = ExecutionContext::new();
    // Space (toggle auth), Down, Down, Space (toggle cache), Enter
    ctx.set_interactive_events(vec![
        make_key(KeyCode::Char(' ')),
        make_key(KeyCode::Down),
        make_key(KeyCode::Down),
        make_key(KeyCode::Char(' ')),
        make_key(KeyCode::Enter),
    ]);

    let status = plan.execute(&mut ctx).await.unwrap();
    assert_eq!(status, 0);

    let shell_val = ctx.get_shell_val("SELECTED_SERVICES").expect("array set");
    match shell_val {
        ShellValue::Array(arr) => {
            assert_eq!(
                arr.values().map(String::as_str).collect::<Vec<_>>(),
                ["auth", "cache"]
            );
        }
        _ => panic!("Expected array shell value"),
    }
}

#[tokio::test]
async fn test_read_menu_piped_lines() {
    let script = r#"
        printf "staging\nproduction\nqa\n" | read --menu TARGET_ENV
    "#;

    let plan = compile_script(script).expect("compile");
    let mut ctx = ExecutionContext::new();
    // Down -> selects "production"
    ctx.set_interactive_events(vec![make_key(KeyCode::Down), make_key(KeyCode::Enter)]);
    let status = plan.execute(&mut ctx).await.unwrap();
    assert_eq!(status, 0);
    assert_eq!(ctx.get_var("TARGET_ENV").as_deref(), Some("production"));
}

#[tokio::test]
async fn test_read_menu_defaults_validation() {
    // 1. Single menu with valid default key
    let script1 = r#"
        DEF="eu"
        read --menu 'us="United States" eu="Europe"' --defaults DEF CHOSEN
    "#;
    let plan1 = compile_script(script1).expect("compile");
    let mut ctx1 = ExecutionContext::new();
    // Enter immediately accepts default key 'eu'
    ctx1.set_interactive_events(vec![make_key(KeyCode::Enter)]);
    let status1 = plan1.execute(&mut ctx1).await.unwrap();
    assert_eq!(status1, 0);
    assert_eq!(ctx1.get_var("CHOSEN").as_deref(), Some("eu"));

    // 2. Invalid default key not in menu -> status 2
    let script2 = r#"
        DEF="invalid_key"
        read --menu 'us="United States" eu="Europe"' --defaults DEF CHOSEN
    "#;
    let plan2 = compile_script(script2).expect("compile");
    let mut ctx2 = ExecutionContext::new();
    let status2 = plan2.execute(&mut ctx2).await.unwrap();
    assert_eq!(status2, 2);
}

#[tokio::test]
async fn test_read_menu_cancellation() {
    let script = r#"
        TARGET="original_value"
        read --menu 'opt1="Option 1" opt2="Option 2"' TARGET
    "#;

    let plan = compile_script(script).expect("compile");
    let mut ctx = ExecutionContext::new();
    // Cancel with Ctrl-C
    ctx.set_interactive_events(vec![make_ctrl_c()]);
    let status = plan.execute(&mut ctx).await.unwrap();

    assert_eq!(status, 130, "Cancellation must return 130");
    assert_eq!(
        ctx.get_var("TARGET").as_deref(),
        Some("original_value"),
        "Target variable must remain unchanged on cancellation"
    );
}

#[tokio::test]
async fn test_read_non_raw_escapes_and_continuation() {
    let mut ctx = ExecutionContext::new();
    ctx.set_stdin(b"first line \\\ncontinued line with \\escaped\n".to_vec());

    let script = "read LINE";
    let plan = compile_script(script).expect("compile");
    plan.execute(&mut ctx).await.expect("execute");

    assert_eq!(
        ctx.get_var("LINE").as_deref(),
        Some("first line continued line with escaped")
    );
}
