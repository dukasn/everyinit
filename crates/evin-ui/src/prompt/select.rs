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
use crate::render::{SYMBOL_FRAME_BAR, SYMBOL_SELECT_POINTER, Theme};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use std::io;
use std::time::Duration;

pub struct SelectPrompt<'a> {
    prompt: &'a str,
    items: Vec<(&'a str, &'a str)>, // (value/key, label)
    default_key: Option<&'a str>,
    timeout: Option<Duration>,
}

pub fn select(prompt: &str) -> SelectPrompt<'_> {
    SelectPrompt {
        prompt,
        items: Vec::new(),
        default_key: None,
        timeout: None,
    }
}

impl<'a> SelectPrompt<'a> {
    pub fn item(mut self, value: &'a str, label: &'a str) -> Self {
        self.items.push((value, label));
        self
    }

    pub fn items(mut self, items: &[(&'a str, &'a str)]) -> Self {
        self.items.extend_from_slice(items);
        self
    }

    pub fn default_key(mut self, key: &'a str) -> Self {
        self.default_key = Some(key);
        self
    }

    pub fn timeout(mut self, duration: Duration) -> Self {
        self.timeout = Some(duration);
        self
    }

    pub fn interact(self) -> io::Result<PromptResult<String>> {
        let mut session = TtySession::try_new()?;
        self.interact_with(&mut session)
    }

    pub fn interact_with<S: ?Sized + PromptSession>(
        self,
        session: &mut S,
    ) -> io::Result<PromptResult<String>> {
        if self.items.is_empty() {
            return Ok(PromptResult::Error(
                "No items provided for select menu".to_string(),
            ));
        }

        let theme = Theme::default();
        let mut cursor_idx = if let Some(def_k) = self.default_key {
            self.items
                .iter()
                .position(|(k, _)| *k == def_k)
                .unwrap_or(0)
        } else {
            0
        };

        let item_count = self.items.len();
        let mut rendered_once = false;

        loop {
            if rendered_once {
                // Clear previous menu lines (header line + items)
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
                let frame = theme
                    .frame_color
                    .apply_to(format!("{}  ", SYMBOL_FRAME_BAR))
                    .to_string();
                session.write_str(&frame)?;

                if is_current {
                    let pointer = theme.info_color.apply_to(SYMBOL_SELECT_POINTER).to_string();
                    let lbl = theme.info_color.apply_to(*label).to_string();
                    session.write_str(&format!("{}{}\n\r", pointer, lbl))?;
                } else {
                    session.write_str(&format!("  {}\n\r", label))?;
                }
            }

            session.flush()?;
            rendered_once = true;

            match session.poll_event(self.timeout)? {
                Some(Event::Key(key)) if key.kind == KeyEventKind::Press => {
                    match key.code {
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            // Clear menu lines
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
                        KeyCode::Home => {
                            cursor_idx = 0;
                        }
                        KeyCode::End => {
                            cursor_idx = item_count - 1;
                        }
                        KeyCode::Enter => {
                            break;
                        }
                        _ => {}
                    }
                }
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

        let (chosen_val, chosen_label) = self.items[cursor_idx];
        let success = format_prompt_success(&theme, self.prompt, chosen_label);
        session.write_str(&success)?;
        session.flush()?;

        Ok(PromptResult::Selected(chosen_val.to_string()))
    }
}
