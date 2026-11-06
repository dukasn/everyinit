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

/// Reserved variable name that compiles to a literal `!` optional capture group
/// rather than a generic `.+?` matcher.
const BREAKING_VAR: &str = "breaking";

/// A token emitted by the template lexer.
#[derive(Debug)]
enum Token {
    /// A literal string segment (already regex-escaped).
    Literal(String),
    /// A required named capture group: `(?P<name>.+?)`.
    Required(String),
    /// An optional named capture group, potentially with surrounding delimiters.
    /// `delimiters` holds the (open, close) literal chars if they were detected.
    Optional {
        name: String,
        delimiters: Option<(String, String)>,
    },
}

/// Compiles a template string (e.g. `{type}({scope}?){breaking?}: {description}`)
/// into a fully-anchored `^...$` `Regex`.
///
/// ### Variable Syntax
/// | Template | Compiled regex | Notes |
/// |---|---|---|
/// | `{type}` | `(?P<type>.+?)` | Required capture |
/// | `{scope?}` or `{scope}?` | `(?P<scope>.+?)?` | Optional capture |
/// | `({scope}?)` | `(?:\((?P<scope>.+?)\))?` | **Delimiter collapsing** — the `()` are also made optional |
/// | `{breaking?}` | `(?P<breaking>!)?` | **Reserved** — compiles to literal `!`, not generic `.+?` |
pub fn compile_template(template: &str) -> Result<Regex, regex::Error> {
    let tokens = lex(template);
    let mut pattern = String::from("^");
    for token in tokens {
        match token {
            Token::Literal(s) => pattern.push_str(&s),
            Token::Required(name) => {
                pattern.push_str(&format!("(?P<{name}>.+?)"));
            }
            Token::Optional { name, delimiters } => {
                let inner = if name == BREAKING_VAR {
                    // Reserved: the breaking marker is always the literal character `!`
                    format!("(?P<{name}>!)")
                } else {
                    format!("(?P<{name}>.+?)")
                };
                if let Some((open, close)) = delimiters {
                    // Wrap delimiters + capture as a single atomic optional group
                    pattern.push_str(&format!("(?:{open}{inner}{close})?"));
                } else {
                    pattern.push_str(&format!("(?:{inner})?"));
                }
            }
        }
    }
    pattern.push('$');
    Regex::new(&pattern)
}

/// Lex the template string into a flat list of [`Token`]s.
///
/// This is a two-phase lexer:
/// 1. Parse all tokens in order, marking variables as required or optional.
/// 2. After parsing, perform a backward pass to detect delimiter collapsing:
///    if a `Literal` token immediately before an `Optional` variable ends with
///    a single delimiter character, and the next `Literal` token starts with
///    its counterpart, fold those delimiter chars into the `Optional` token.
fn lex(template: &str) -> Vec<Token> {
    use super::parser::{TemplateParser, Token as ParserToken};
    let parser = TemplateParser::new("{", "}", "^\\{\\}");
    let parser_tokens = parser.parse(template);

    let mut raw: Vec<Token> = Vec::new();

    for pt in parser_tokens {
        match pt {
            ParserToken::Literal(lit) => raw.push(Token::Literal(regex::escape(&lit))),
            ParserToken::Variable(mut var_name) => {
                let is_optional = var_name.ends_with('?');
                if is_optional {
                    var_name.pop();
                }

                if is_optional {
                    raw.push(Token::Optional {
                        name: var_name,
                        delimiters: None,
                    });
                } else {
                    raw.push(Token::Required(var_name));
                }
            }
        }
    }

    // We need to support `{scope}?` which trailing `?` after closing brace.
    // So we post-process `raw` to check if a Literal starts with `?` immediately after a Required token.
    for i in 0..raw.len() {
        let is_qmark = if let Token::Literal(lit) = &raw[i] {
            let escaped_qmark = regex::escape("?");
            if lit.starts_with(&escaped_qmark) {
                Some((lit.clone(), escaped_qmark))
            } else {
                None
            }
        } else {
            None
        };

        if let Some((lit, escaped_qmark)) = is_qmark {
            if i == 0 {
                continue;
            }
            if let Token::Required(name) = &raw[i - 1] {
                let name = name.clone();
                // Turn previous into Optional
                raw[i - 1] = Token::Optional {
                    name,
                    delimiters: None,
                };
                // Strip '\?' from literal
                let new_lit = lit[escaped_qmark.len()..].to_string();
                raw[i] = Token::Literal(new_lit);
            }
        }
    }
    //
    // Canonical delimiter pairs we recognise.
    const PAIRS: &[(char, char)] = &[('(', ')'), ('[', ']'), ('<', '>')];

    let mut result: Vec<Token> = Vec::new();
    let mut i = 0;

    while i < raw.len() {
        match &raw[i] {
            Token::Optional { .. } => {
                // Check if the previous result token ends with an opening delimiter
                // and the next raw token starts with the matching closing delimiter.
                let prev_lit = result.last().and_then(|t| {
                    if let Token::Literal(s) = t {
                        Some(s.clone())
                    } else {
                        None
                    }
                });

                let next_lit = raw.get(i + 1).and_then(|t| {
                    if let Token::Literal(s) = t {
                        Some(s.clone())
                    } else {
                        None
                    }
                });

                // regex::escape turns `(` into `\(` — we check for that escaped form
                let collapsed =
                    prev_lit
                        .as_deref()
                        .zip(next_lit.as_deref())
                        .and_then(|(prev, next)| {
                            for &(open, close) in PAIRS {
                                let esc_open = regex::escape(&open.to_string());
                                let esc_close = regex::escape(&close.to_string());
                                if prev.ends_with(&esc_open) && next.starts_with(&esc_close) {
                                    return Some((esc_open, esc_close));
                                }
                            }
                            None
                        });

                if let Some((esc_open, esc_close)) = collapsed {
                    // Trim the opening delimiter from the previous literal token
                    if let Some(Token::Literal(s)) = result.last_mut() {
                        let trimmed = s[..s.len() - esc_open.len()].to_string();
                        *s = trimmed;
                    }

                    // Extract the variable name
                    let name = if let Token::Optional { name, .. } = &raw[i] {
                        name.clone()
                    } else {
                        unreachable!()
                    };

                    // Push the collapsed optional token
                    result.push(Token::Optional {
                        name,
                        delimiters: Some((esc_open, esc_close.clone())),
                    });

                    // Consume the next literal, trimming the closing delimiter from its start
                    if let Some(Token::Literal(next_s)) = raw.get(i + 1) {
                        let remainder = next_s[esc_close.len()..].to_string();
                        if !remainder.is_empty() {
                            result.push(Token::Literal(remainder));
                        }
                        i += 2; // Skip both the Optional and the next Literal
                        continue;
                    }
                } else {
                    // No collapsing — push as-is
                    let name = if let Token::Optional { name, .. } = &raw[i] {
                        name.clone()
                    } else {
                        unreachable!()
                    };
                    result.push(Token::Optional {
                        name,
                        delimiters: None,
                    });
                }
            }
            Token::Literal(s) => result.push(Token::Literal(s.clone())),
            Token::Required(name) => result.push(Token::Required(name.clone())),
        }
        i += 1;
    }

    result
}
