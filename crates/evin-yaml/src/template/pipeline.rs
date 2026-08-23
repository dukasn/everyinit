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

use super::parser::{TemplateParser, Token};
use std::collections::HashMap;
use thiserror::Error;

pub trait TemplateContext {
    fn task_output(&self, task: &str, key: &str) -> Option<String>;
    fn value(&self, key: &str) -> Option<String>;
    fn env(&self, key: &str) -> Option<String>;
    fn is_unverified(&self, task: &str) -> bool;
    fn mark_unverified(&self, message: String);
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TemplateError {
    #[error("{}", evin_locales::t!("malformed-syntax", text = .0))]
    MalformedSyntax(String),
    #[error("{}", evin_locales::t!("missing-task", task = .0))]
    MissingTask(String),
    #[error("{}", evin_locales::t!("missing-output", task = task, key = key))]
    MissingOutput { task: String, key: String },
    #[error("missing value '{0}'")]
    MissingValue(String),
    #[error("missing environment value '{0}'")]
    MissingEnv(String),
    #[error("missing action-local value '{0}'")]
    MissingSelf(String),
    #[error("{}", evin_locales::t!("blank-resolution", text = .0))]
    BlankResolution(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Reference {
    Task { task: String, key: String },
    Env(String),
    Local(String),
}

fn get_parser() -> TemplateParser {
    // Pipeline syntax uses ${var}, meaning { and } are invalid inside the body.
    TemplateParser::new("${", "}", "^\\{\\}")
}

pub fn validate_template_syntax(text: &str) -> Result<(), TemplateError> {
    validate_template_syntax_with_self(text, &[])
}

pub fn validate_template_syntax_with_self(
    text: &str,
    allowed_self: &[&str],
) -> Result<(), TemplateError> {
    if text.contains("{{") || text.contains("}}") {
        return Err(TemplateError::MalformedSyntax(
            "old syntax {{...}} is not supported, use ${...}".to_string(),
        ));
    }
    parse_references(text, allowed_self).map(|_| ())
}

pub fn resolve_templates<C: TemplateContext>(text: &str, ctx: &C) -> Result<String, TemplateError> {
    resolve_templates_with_self(text, ctx, &HashMap::new())
}

pub fn resolve_templates_with_self<C: TemplateContext>(
    text: &str,
    ctx: &C,
    self_values: &HashMap<String, String>,
) -> Result<String, TemplateError> {
    let parser = get_parser();
    if let Err(fragment) = parser.check_malformed(text) {
        return Err(TemplateError::MalformedSyntax(fragment));
    }

    let tokens = parser.parse(text);
    let mut result = String::new();

    for token in tokens {
        match token {
            Token::Literal(lit) => result.push_str(&lit),
            Token::Variable(body) => {
                let reference = parse_reference(
                    &body,
                    &self_values.keys().map(|k| k.as_str()).collect::<Vec<_>>(),
                )
                .map_err(|_| TemplateError::MalformedSyntax(format!("${{{}}}", body)))?;

                let resolved = match reference {
                    Reference::Task { task, key } => match ctx.task_output(&task, &key) {
                        Some(value) => value,
                        None if ctx.is_unverified(&task) => {
                            ctx.mark_unverified(format!(
                                "Output '{}.{}' is unresolved because '{}' is unverified",
                                task, key, task
                            ));
                            format!("UNRESOLVED_{}_{}", task.replace(':', "_"), key)
                        }
                        None => return Err(TemplateError::MissingOutput { task, key }),
                    },
                    Reference::Env(key) => ctx.env(&key).ok_or(TemplateError::MissingEnv(key))?,
                    Reference::Local(key) => self_values
                        .get(&key)
                        .cloned()
                        .or_else(|| ctx.value(&key))
                        .ok_or(TemplateError::MissingSelf(key))?,
                };
                result.push_str(&resolved);
            }
        }
    }

    if result.trim().is_empty() {
        return Err(TemplateError::BlankResolution(text.to_string()));
    }

    Ok(result)
}

fn parse_references(text: &str, allowed_self: &[&str]) -> Result<(), TemplateError> {
    let parser = get_parser();
    if let Err(fragment) = parser.check_malformed(text) {
        return Err(TemplateError::MalformedSyntax(fragment));
    }

    let tokens = parser.parse(text);
    for token in tokens {
        if let Token::Variable(body) = token {
            parse_reference(&body, allowed_self)
                .map_err(|_| TemplateError::MalformedSyntax(format!("${{{}}}", body)))?;
        }
    }
    Ok(())
}

fn parse_reference(body: &str, allowed_self: &[&str]) -> Result<Reference, ()> {
    if let Some(key) = body.strip_prefix("env.") {
        validate_name(key, true)?;
        return Ok(Reference::Env(key.to_string()));
    }

    if let Some((task, key)) = body.rsplit_once('.') {
        validate_task_id(task)?;
        validate_name(key, false)?;
        return Ok(Reference::Task {
            task: task.to_string(),
            key: key.to_string(),
        });
    }

    validate_name(body, false)?;
    if !allowed_self.is_empty() && !allowed_self.contains(&body) {
        return Err(());
    }
    Ok(Reference::Local(body.to_string()))
}

fn validate_task_id(value: &str) -> Result<(), ()> {
    let regex = regex::Regex::new(r"^[a-zA-Z0-9_:-]+$").unwrap();
    if value.is_empty() || !regex.is_match(value) {
        return Err(());
    }
    Ok(())
}

fn validate_name(value: &str, env: bool) -> Result<(), ()> {
    let regex = if env {
        regex::Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]*$").unwrap()
    } else {
        regex::Regex::new(r"^[a-zA-Z0-9_-]+$").unwrap()
    };
    if value.is_empty() || !regex.is_match(value) {
        return Err(());
    }
    Ok(())
}
