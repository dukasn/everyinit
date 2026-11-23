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

use crate::error::ParseError;
use crate::lexer::cursor::Lexer;
use crate::parser::ast::{ASTNode, Assignment, Redirection, Word, WordPart};
use crate::parser::pratt::Parser;
use crate::planner::dispatch::resolve_builtin;
use crate::planner::ir::{
    CommandTarget, ExecutionPlan, ExecutionStage, PlanAssignment, PlanNode, PlanRedirection,
    PlanWord, PlanWordPart,
};
use bumpalo::Bump;
use std::path::PathBuf;

pub fn compile_script(script: &str) -> Result<ExecutionPlan, ParseError> {
    let lexer = Lexer::new(script);
    let tokens = lexer.tokenize_all().map_err(|e| ParseError {
        message: e.message,
        span: e.span,
    })?;

    let arena = Bump::new();
    let mut parser = Parser::new(&arena, tokens);
    let ast_root = parser.parse_script()?;

    let stages = lower_ast_to_stages(ast_root)?;
    Ok(ExecutionPlan { stages })
}

fn lower_ast_to_stages(node: &ASTNode<'_>) -> Result<Vec<ExecutionStage>, ParseError> {
    match node {
        ASTNode::StatementList { statements, .. } => {
            let mut list = Vec::new();
            for stmt in statements {
                let mut sub_stages = lower_ast_to_stages(stmt)?;
                list.append(&mut sub_stages);
            }
            Ok(list)
        }
        _ => Ok(vec![lower_ast_node(node)?]),
    }
}

fn lower_ast_node(node: &ASTNode<'_>) -> Result<ExecutionStage, ParseError> {
    match node {
        ASTNode::Negation { inner, .. } => {
            let inner_stage = lower_ast_node(inner)?;
            Ok(ExecutionStage::Negation(Box::new(inner_stage)))
        }
        ASTNode::Command {
            name,
            args,
            assignments,
            redirections,
            span,
        } => {
            let plan_node = lower_command(name, args, assignments, redirections, *span)?;
            Ok(ExecutionStage::Pipeline(vec![plan_node]))
        }
        ASTNode::RedirectedCompound {
            inner,
            redirections,
            ..
        } => Ok(ExecutionStage::Redirected {
            inner: Box::new(lower_ast_node(inner)?),
            redirections: redirections.iter().map(lower_redirection).collect(),
        }),
        ASTNode::Pipeline { stages, .. } => {
            let mut plan_nodes = Vec::new();
            let mut lowered_stages = Vec::new();
            let mut contains_compound_stage = false;
            for stage in stages {
                match *stage {
                    ASTNode::Command {
                        name,
                        args,
                        assignments,
                        redirections,
                        span,
                    } => {
                        let node = lower_command(name, args, assignments, redirections, *span)?;
                        lowered_stages.push(ExecutionStage::Pipeline(vec![node.clone()]));
                        plan_nodes.push(node);
                    }
                    other => {
                        contains_compound_stage = true;
                        lowered_stages.push(lower_ast_node(other)?);
                    }
                }
            }
            if contains_compound_stage {
                Ok(ExecutionStage::PipelineStages(lowered_stages))
            } else {
                Ok(ExecutionStage::Pipeline(plan_nodes))
            }
        }
        ASTNode::ParallelPipeline { branches, .. } => {
            let mut branch_stages = Vec::new();
            for branch in branches {
                branch_stages.push(lower_ast_node(branch)?);
            }
            Ok(ExecutionStage::Parallel(branch_stages))
        }
        ASTNode::LogicalChain {
            left, op, right, ..
        } => {
            let left_stage = lower_ast_node(left)?;
            let right_stage = lower_ast_node(right)?;
            Ok(ExecutionStage::Sequential {
                left: Box::new(left_stage),
                require_success: matches!(op, crate::parser::ast::LogicalOp::And),
                right: Box::new(right_stage),
            })
        }
        ASTNode::FailFastBlock {
            try_stage,
            fallback,
            ..
        } => {
            let try_s = lower_ast_node(try_stage)?;
            let fb_s = match fallback {
                Some(f) => Some(Box::new(lower_ast_node(f)?)),
                None => None,
            };
            Ok(ExecutionStage::FailFastBlock {
                try_stage: Box::new(try_s),
                fallback: fb_s,
            })
        }
        ASTNode::Block { statements, .. } => {
            let mut list = Vec::new();
            for stmt in statements {
                list.push(lower_ast_node(stmt)?);
            }
            Ok(ExecutionStage::Block(list))
        }
        ASTNode::Subshell { statements, .. } => {
            let mut list = Vec::new();
            for stmt in statements {
                list.push(lower_ast_node(stmt)?);
            }
            Ok(ExecutionStage::Subshell(list))
        }
        ASTNode::FunctionDeclaration { name, body, .. } => {
            let body_stage = lower_ast_node(body)?;
            let plan = ExecutionPlan {
                stages: vec![body_stage],
            };
            Ok(ExecutionStage::FunctionDef {
                name: (*name).to_string(),
                plan,
            })
        }
        ASTNode::If {
            condition,
            then_branch,
            elif_branches,
            else_branch,
            ..
        } => {
            let cond_stage = lower_ast_node(condition)?;
            let then_stage = lower_ast_node(then_branch)?;
            let mut elif_stages = Vec::new();
            for (c, b) in elif_branches {
                elif_stages.push((lower_ast_node(c)?, lower_ast_node(b)?));
            }
            let else_stage = match else_branch {
                Some(eb) => Some(Box::new(lower_ast_node(eb)?)),
                None => None,
            };
            Ok(ExecutionStage::If {
                condition: Box::new(cond_stage),
                then_branch: Box::new(then_stage),
                elif_branches: elif_stages,
                else_branch: else_stage,
            })
        }
        ASTNode::For {
            var, words, body, ..
        } => {
            let body_stage = lower_ast_node(body)?;
            let plan_words = words
                .as_ref()
                .map(|w_list| w_list.iter().map(lower_word).collect());
            Ok(ExecutionStage::ForLoop {
                var: (*var).to_string(),
                words: plan_words,
                body: Box::new(body_stage),
            })
        }
        ASTNode::Select {
            var, words, body, ..
        } => {
            let body_stage = lower_ast_node(body)?;
            let plan_words = words
                .as_ref()
                .map(|w_list| w_list.iter().map(lower_word).collect());
            Ok(ExecutionStage::SelectLoop {
                var: (*var).to_string(),
                words: plan_words,
                body: Box::new(body_stage),
            })
        }
        ASTNode::While {
            condition,
            body,
            is_until,
            ..
        } => {
            let cond_stage = lower_ast_node(condition)?;
            let body_stage = lower_ast_node(body)?;
            Ok(ExecutionStage::WhileLoop {
                condition: Box::new(cond_stage),
                body: Box::new(body_stage),
                is_until: *is_until,
            })
        }
        ASTNode::Case { target, arms, .. } => {
            let plan_target = lower_word(target);
            let mut plan_arms = Vec::new();
            for arm in arms {
                let patterns = arm.patterns.iter().map(lower_case_pattern_word).collect();
                let body = match arm.body {
                    Some(b) => Some(Box::new(lower_ast_node(b)?)),
                    None => None,
                };
                plan_arms.push(crate::planner::ir::PlanCaseArm {
                    patterns,
                    body,
                    delimiter: arm.delimiter,
                });
            }
            Ok(ExecutionStage::CaseBranch {
                target: plan_target,
                arms: plan_arms,
            })
        }
        ASTNode::StatementList { statements, .. } => {
            let mut list = Vec::new();
            for stmt in statements {
                list.push(lower_ast_node(stmt)?);
            }
            Ok(ExecutionStage::Block(list))
        }
    }
}

fn lower_command(
    name: &Word<'_>,
    args: &[Word<'_>],
    assignments: &[Assignment<'_>],
    redirections: &[Redirection<'_>],
    span: crate::lexer::Span,
) -> Result<PlanNode, ParseError> {
    let raw_name = word_to_raw_string(name);
    let command_word = lower_word(name);

    let target = if command_word.parts.iter().any(|part| {
        matches!(
            part,
            PlanWordPart::HomeTilde
                | PlanWordPart::DoubleQuoted(_)
                | PlanWordPart::Variable(_)
                | PlanWordPart::CommandSub(_)
        )
    }) {
        CommandTarget::Dynamic(command_word)
    } else if let Some(builtin_fn) = resolve_builtin(&raw_name) {
        CommandTarget::Builtin(builtin_fn)
    } else {
        CommandTarget::External(PathBuf::from(&raw_name))
    };

    let plan_args = args.iter().map(lower_word).collect();
    let plan_assignments = assignments.iter().map(lower_assignment).collect();
    let plan_redirections = redirections.iter().map(lower_redirection).collect();

    Ok(PlanNode {
        target,
        raw_name,
        args: plan_args,
        assignments: plan_assignments,
        redirections: plan_redirections,
        span,
    })
}

fn lower_word(word: &Word<'_>) -> PlanWord {
    let parts = word
        .parts
        .iter()
        .enumerate()
        .flat_map(|(index, p)| match p {
            WordPart::Literal(s) if index == 0 && (*s == "~" || s.starts_with("~/")) => {
                let mut lowered = vec![PlanWordPart::HomeTilde];
                if s.len() > 1 {
                    lowered.push(PlanWordPart::Literal(unescape_unquoted(&s[1..])));
                }
                lowered
            }
            WordPart::Literal(s) => vec![PlanWordPart::Literal(unescape_unquoted(s))],
            WordPart::SingleQuoted(s) => vec![PlanWordPart::SingleQuoted((*s).to_string())],
            WordPart::DoubleQuoted(s) => vec![PlanWordPart::DoubleQuoted((*s).to_string())],
            WordPart::Variable(s) => vec![PlanWordPart::Variable((*s).to_string())],
            WordPart::CommandSub(s) => vec![PlanWordPart::CommandSub((*s).to_string())],
            WordPart::HereDocBody(body, quoted, strip_tabs) => {
                vec![PlanWordPart::HereDocBody(
                    (*body).to_string(),
                    *quoted,
                    *strip_tabs,
                )]
            }
        })
        .collect();

    PlanWord {
        parts,
        span: word.span,
    }
}

fn lower_case_pattern_word(word: &Word<'_>) -> PlanWord {
    let parts = word
        .parts
        .iter()
        .map(|part| match part {
            WordPart::Literal(source) => PlanWordPart::Literal(escape_pattern_quoting(source)),
            WordPart::SingleQuoted(value) => PlanWordPart::CaseQuoted((*value).to_string()),
            WordPart::DoubleQuoted(value) => PlanWordPart::CaseDoubleQuoted((*value).to_string()),
            WordPart::Variable(value) => PlanWordPart::Variable((*value).to_string()),
            WordPart::CommandSub(value) => PlanWordPart::CommandSub((*value).to_string()),
            WordPart::HereDocBody(body, quoted, strip_tabs) => {
                PlanWordPart::HereDocBody((*body).to_string(), *quoted, *strip_tabs)
            }
        })
        .collect();

    PlanWord {
        parts,
        span: word.span,
    }
}

fn escape_pattern_quoting(source: &str) -> String {
    let mut result = String::with_capacity(source.len());
    let mut chars = source.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            result.push(ch);
            continue;
        }
        match chars.next() {
            Some('\n') => {}
            Some('\r') if chars.clone().next() == Some('\n') => {
                chars.next();
            }
            Some(escaped) => {
                // Keep escapes that can change pattern meaning, including
                // hyphens and negators when they occur inside bracket lists.
                if matches!(escaped, '*' | '?' | '[' | ']' | '\\' | '-' | '!' | '^') {
                    result.push('\\');
                }
                result.push(escaped);
            }
            None => result.push_str("\\\\"),
        }
    }
    result
}

fn unescape_unquoted(source: &str) -> String {
    let mut result = String::with_capacity(source.len());
    let mut chars = source.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            result.push(ch);
            continue;
        }

        match chars.next() {
            Some('\n') => {}
            Some('\r') if chars.peek() == Some(&'\n') => {
                chars.next();
            }
            Some(escaped) => result.push(escaped),
            None => result.push('\\'),
        }
    }
    result
}

fn lower_assignment(asgn: &Assignment<'_>) -> PlanAssignment {
    PlanAssignment {
        name: asgn.name.to_string(),
        index: asgn.index.as_ref().map(lower_word),
        value: asgn.value.as_ref().map(lower_word),
        array_values: asgn
            .array_values
            .as_ref()
            .map(|arr| arr.iter().map(lower_word).collect()),
        span: asgn.span,
    }
}

fn lower_redirection(redir: &Redirection<'_>) -> PlanRedirection {
    PlanRedirection {
        op: redir.op,
        target: lower_word(&redir.target),
        span: redir.span,
    }
}

fn word_to_raw_string(word: &Word<'_>) -> String {
    let mut s = String::new();
    for part in &word.parts {
        match part {
            WordPart::Literal(p) => s.push_str(&unescape_unquoted(p)),
            WordPart::SingleQuoted(p)
            | WordPart::DoubleQuoted(p)
            | WordPart::Variable(p)
            | WordPart::CommandSub(p) => {
                s.push_str(p);
            }
            WordPart::HereDocBody(body, _, _) => s.push_str(body),
        }
    }
    s
}
