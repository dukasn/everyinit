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
use evin_ui::prompt::{
    MockPromptSession, PromptResult, confirm, input, multiselect, secret, select,
};

fn make_key_event(code: KeyCode) -> Event {
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

#[test]
fn test_input_prompt_typing_and_default() {
    // 1. Typed input
    let mut session1 = MockPromptSession::from_chars("hello world\n");
    let res1 = input("Greeting").interact_with(&mut session1).unwrap();
    assert_eq!(res1, PromptResult::Selected("hello world".to_string()));

    // 2. Default fallback on empty enter
    let mut session2 = MockPromptSession::from_chars("\n");
    let res2 = input("Target")
        .default_input("production")
        .interact_with(&mut session2)
        .unwrap();
    assert_eq!(res2, PromptResult::Selected("production".to_string()));

    // 3. Cancel with Esc
    let mut session3 = MockPromptSession::new(vec![make_key_event(KeyCode::Esc)]);
    let res3 = input("Cancelled").interact_with(&mut session3).unwrap();
    assert_eq!(res3, PromptResult::Cancelled);
}

#[test]
fn test_secret_prompt_masking() {
    let mut session = MockPromptSession::from_chars("my-secret-token\n");
    let res = secret("API Key")
        .mask('*')
        .interact_with(&mut session)
        .unwrap();
    assert_eq!(res, PromptResult::Selected("my-secret-token".to_string()));
}

#[test]
fn test_confirm_prompt_choices() {
    // 'y' -> true
    let mut session1 = MockPromptSession::from_chars("y");
    let res1 = confirm("Deploy?").interact_with(&mut session1).unwrap();
    assert_eq!(res1, PromptResult::Selected(true));

    // 'n' -> false
    let mut session2 = MockPromptSession::from_chars("n");
    let res2 = confirm("Delete database?")
        .interact_with(&mut session2)
        .unwrap();
    assert_eq!(res2, PromptResult::Selected(false));

    // Enter with default true -> true
    let mut session3 = MockPromptSession::from_chars("\n");
    let res3 = confirm("Continue?")
        .default_confirm(true)
        .interact_with(&mut session3)
        .unwrap();
    assert_eq!(res3, PromptResult::Selected(true));

    // Ctrl-C -> Cancelled
    let mut session4 = MockPromptSession::new(vec![make_ctrl_c()]);
    let res4 = confirm("Dangerous operation?")
        .interact_with(&mut session4)
        .unwrap();
    assert_eq!(res4, PromptResult::Cancelled);
}

#[test]
fn test_single_select_prompt() {
    let items = vec![
        ("us", "United States"),
        ("eu", "Europe"),
        ("ap", "Asia Pacific"),
    ];

    // Down arrow -> select Europe (index 1)
    let events = vec![
        make_key_event(KeyCode::Down),
        make_key_event(KeyCode::Enter),
    ];
    let mut session = MockPromptSession::new(events);
    let res = select("Select region")
        .items(&items)
        .interact_with(&mut session)
        .unwrap();
    assert_eq!(res, PromptResult::Selected("eu".to_string()));
}

#[test]
fn test_multi_select_prompt() {
    let items = vec![
        ("auth", "Authentication service"),
        ("db", "Database cluster"),
        ("cache", "Redis cache"),
    ];

    // Space (selects auth), Down, Space (selects db), Enter
    let events = vec![
        make_key_event(KeyCode::Char(' ')),
        make_key_event(KeyCode::Down),
        make_key_event(KeyCode::Char(' ')),
        make_key_event(KeyCode::Enter),
    ];
    let mut session = MockPromptSession::new(events);
    let res = multiselect("Select components")
        .items(&items)
        .interact_with(&mut session)
        .unwrap();
    assert_eq!(
        res,
        PromptResult::Selected(vec!["auth".to_string(), "db".to_string()])
    );
}
