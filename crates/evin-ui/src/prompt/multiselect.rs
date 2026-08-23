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

use super::{PromptResult, PromptSession, TtySession, format_prompt_header, format_prompt_success};
use crate::render::{
    SYMBOL_CHECKBOX_CHECKED, SYMBOL_CHECKBOX_UNCHECKED, SYMBOL_FRAME_BAR, SYMBOL_SELECT_POINTER,
    Theme,
};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use std::collections::HashSet;
use std::io;
use std::time::Duration;

pub struct MultiSelectPrompt<'a> {
    prompt: &'a str,
    items: Vec<(&'a str, &'a str)>, // (value/key, label)
    default_keys: HashSet<&'a str>,
    timeout: Option<Duration>,
}

pub fn multiselect(prompt: &str) -> MultiSelectPrompt<'_> {
    MultiSelectPrompt {
        prompt,
        items: Vec::new(),
        default_keys: HashSet::new(),
        timeout: None,
    }
}

impl<'a> MultiSelectPrompt<'a> {
    pub fn item(mut self, value: &'a str, label: &'a str) -> Self {
        self.items.push((value, label));
        self
    }

    pub fn items(mut self, items: &[(&'a str, &'a str)]) -> Self {
        self.items.extend_from_slice(items);
        self
    }

    pub fn defaults(mut self, keys: &[&'a str]) -> Self {
        for k in keys {
            self.default_keys.insert(*k);
        }
        self
    }

    pub fn timeout(mut self, duration: Duration) -> Self {
        self.timeout = Some(duration);
        self
    }

    pub fn interact(self) -> io::Result<PromptResult<Vec<String>>> {
        let mut session = TtySession::try_new()?;
        self.interact_with(&mut session)
    }

    pub fn interact_with<S: ?Sized + PromptSession>(
        self,
        session: &mut S,
    ) -> io::Result<PromptResult<Vec<String>>> {
        if self.items.is_empty() {
            return Ok(PromptResult::Error(
                "No items provided for multi-select menu".to_string(),
            ));
        }

        let theme = Theme::default();
        let mut selected: Vec<bool> = self
            .items
            .iter()
            .map(|(k, _)| self.default_keys.contains(k))
            .collect();
        let mut cursor_idx = 0;
        let item_count = self.items.len();
        let mut rendered_once = false;

        loop {
            if rendered_once {
                for _ in 0..=item_count {
                    session.write_str("\x1B[1A\x1B[2K\r")?;
                }
            }

            // Draw Header
            let header = format_prompt_header(&theme, self.prompt, None);
            session.write_str("\x1B[2K\r")?;
            session.write_str(&header)?;
            session.write_str("\n\r")?;

            // Draw Items
            for (i, (_, label)) in self.items.iter().enumerate() {
                let is_current = i == cursor_idx;
                let is_selected = selected[i];

                let frame = theme
                    .frame_color
                    .apply_to(format!("{}  ", SYMBOL_FRAME_BAR))
                    .to_string();
                session.write_str(&frame)?;

                if is_current {
                    let pointer = theme.info_color.apply_to(SYMBOL_SELECT_POINTER).to_string();
                    session.write_str(&pointer)?;
                } else {
                    session.write_str("  ")?;
                }

                if is_selected {
                    let chk = theme
                        .success_color
                        .apply_to(SYMBOL_CHECKBOX_CHECKED)
                        .to_string();
                    session.write_str(&chk)?;
                } else {
                    let chk = theme
                        .frame_color
                        .apply_to(SYMBOL_CHECKBOX_UNCHECKED)
                        .to_string();
                    session.write_str(&chk)?;
                }

                if is_current {
                    let lbl = theme.info_color.apply_to(*label).to_string();
                    session.write_str(&format!("{}\n\r", lbl))?;
                } else {
                    session.write_str(&format!("{}\n\r", label))?;
                }
            }

            session.flush()?;
            rendered_once = true;

            match session.poll_event(self.timeout)? {
                Some(Event::Key(key)) if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        for _ in 0..=item_count {
                            session.write_str("\x1B[1A\x1B[2K\r")?;
                        }
                        session.flush()?;
                        return Ok(PromptResult::Cancelled);
                    }
                    KeyCode::Esc => {
                        for _ in 0..=item_count {
                            session.write_str("\x1B[1A\x1B[2K\r")?;
                        }
                        session.flush()?;
                        return Ok(PromptResult::Cancelled);
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        if cursor_idx > 0 {
                            cursor_idx -= 1;
                        } else {
                            cursor_idx = item_count - 1;
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if cursor_idx + 1 < item_count {
                            cursor_idx += 1;
                        } else {
                            cursor_idx = 0;
                        }
                    }
                    KeyCode::Char(' ') => {
                        selected[cursor_idx] = !selected[cursor_idx];
                    }
                    KeyCode::Char('a') => {
                        let all_checked = selected.iter().all(|&s| s);
                        selected.fill(!all_checked);
                    }
                    KeyCode::Enter => {
                        break;
                    }
                    _ => {}
                },
                None if self.timeout.is_some() => {
                    for _ in 0..=item_count {
                        session.write_str("\x1B[1A\x1B[2K\r")?;
                    }
                    session.flush()?;
                    return Ok(PromptResult::TimedOut);
                }
                _ => {}
            }
        }

        // Clear menu lines
        for _ in 0..=item_count {
            session.write_str("\x1B[1A\x1B[2K\r")?;
        }

        let mut picked_vals = Vec::new();
        let mut picked_labels = Vec::new();
        for (i, (val, label)) in self.items.iter().enumerate() {
            if selected[i] {
                picked_vals.push(val.to_string());
                picked_labels.push(label.to_string());
            }
        }

        let summary = if picked_labels.is_empty() {
            "none".to_string()
        } else {
            picked_labels.join(", ")
        };

        let success = format_prompt_success(&theme, self.prompt, &summary);
        session.write_str(&success)?;
        session.flush()?;

        Ok(PromptResult::Selected(picked_vals))
    }
}
