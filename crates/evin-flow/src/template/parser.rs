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

use regex::Regex;

/// A parsed token from a template string.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Token {
    /// Literal text content outside of braces.
    Literal(String),
    /// The inner body of a brace variable (e.g., `body` inside `${body}`).
    Variable(String),
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct VariableMatch {
    pub body: String,
    pub offset: usize,
    pub length: usize,
}

/// A generic parser for brace-based template formats.
pub struct TemplateParser {
    pub open: &'static str,
    pub close: &'static str,
    regex: Regex,
}

impl TemplateParser {
    /// Creates a new `TemplateParser`.
    /// `open` is the starting brace sequence (e.g. `"${"`).
    /// `close` is the ending brace sequence (e.g. `"}"`).
    /// `invalid_chars` is a regex character class for characters not allowed inside the body (e.g. `"^{}"`).
    pub fn new(open: &'static str, close: &'static str, invalid_chars: &'static str) -> Self {
        let pattern = format!(
            r"{}(?P<body>[{}]*){}",
            regex::escape(open),
            invalid_chars,
            regex::escape(close)
        );
        Self {
            open,
            close,
            regex: Regex::new(&pattern).expect("valid regex pattern"),
        }
    }

    /// Parses a string into a list of literal and variable tokens.
    pub fn parse(&self, text: &str) -> Vec<Token> {
        let mut tokens = Vec::new();
        let mut last_end = 0;

        for captures in self.regex.captures_iter(text) {
            let full = captures.get(0).expect("full template match");
            if full.start() > last_end {
                tokens.push(Token::Literal(text[last_end..full.start()].to_string()));
            }
            let body = captures.name("body").expect("template body").as_str();
            tokens.push(Token::Variable(body.to_string()));
            last_end = full.end();
        }

        if last_end < text.len() {
            tokens.push(Token::Literal(text[last_end..].to_string()));
        }

        tokens
    }

    /// Return variable bodies together with their byte ranges in `text`.
    pub fn variables(&self, text: &str) -> Vec<VariableMatch> {
        self.regex
            .captures_iter(text)
            .map(|captures| {
                let full = captures.get(0).expect("template capture has full range");
                let body = captures.name("body").expect("template capture has body");
                VariableMatch {
                    body: body.as_str().to_owned(),
                    offset: full.start(),
                    length: full.end() - full.start(),
                }
            })
            .collect()
    }

    /// Checks if a string contains malformed usage of the opening brace.
    /// Returns `Err(fragment)` pointing to the malformed whitespace-separated word.
    pub fn check_malformed(&self, text: &str) -> Result<(), String> {
        let without_valid = self.regex.replace_all(text, "");
        if without_valid.contains(self.open) {
            let fragment = text
                .split_whitespace()
                .find(|part| part.contains(self.open))
                .unwrap_or(text);
            return Err(fragment.to_string());
        }
        Ok(())
    }
}
