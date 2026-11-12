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
use crate::lexer::token::{Token, TokenKind};
use crate::parser::ast::{
    ASTNode, Assignment, CaseArm, CaseDelimiter, LogicalOp, Redirection, RedirectionOp, Word,
    WordPart,
};
use bumpalo::Bump;
use bumpalo::collections::Vec as BumpVec;

pub struct Parser<'a> {
    arena: &'a Bump,
    tokens: Vec<Token<'a>>,
    cursor: usize,
}

impl<'a> Parser<'a> {
    pub fn new(arena: &'a Bump, tokens: Vec<Token<'a>>) -> Self {
        Self {
            arena,
            tokens,
            cursor: 0,
        }
    }

    #[inline(always)]
    fn current(&self) -> Token<'a> {
        if self.cursor < self.tokens.len() {
            self.tokens[self.cursor]
        } else {
            let last_span = self.tokens.last().map(|t| t.span).unwrap_or_default();
            Token::new(TokenKind::Eof, last_span)
        }
    }

    #[inline(always)]
    fn peek_kind(&self) -> TokenKind<'a> {
        self.current().kind
    }

    #[inline(always)]
    fn peek_kind_at(&self, delta: usize) -> TokenKind<'a> {
        let pos = self.cursor + delta;
        if pos < self.tokens.len() {
            self.tokens[pos].kind
        } else {
            TokenKind::Eof
        }
    }

    fn advance(&mut self) -> Token<'a> {
        let token = self.current();
        if self.cursor < self.tokens.len() {
            self.cursor += 1;
        }
        token
    }

    fn skip_newlines(&mut self) {
        while matches!(self.peek_kind(), TokenKind::Newline) {
            self.advance();
        }
    }

    fn skip_terminators(&mut self) {
        while matches!(self.peek_kind(), TokenKind::Newline | TokenKind::Semicolon) {
            self.advance();
        }
    }

    fn expect(&mut self, expected: TokenKind<'a>) -> Result<Token<'a>, ParseError> {
        let tok = self.advance();
        if std::mem::discriminant(&tok.kind) == std::mem::discriminant(&expected) {
            Ok(tok)
        } else {
            Err(ParseError {
                message: format!("Expected {:?}, found {:?}", expected, tok.kind),
                span: tok.span,
            })
        }
    }

    pub fn parse_script(&mut self) -> Result<&'a ASTNode<'a>, ParseError> {
        self.skip_terminators();
        let mut stmts = BumpVec::new_in(self.arena);
        let start_span = self.current().span;

        while self.peek_kind() != TokenKind::Eof {
            let stmt = self.parse_statement()?;
            stmts.push(stmt);
            if self.peek_kind() == TokenKind::Eof {
                break;
            }
            if !matches!(self.peek_kind(), TokenKind::Newline | TokenKind::Semicolon) {
                return Err(ParseError {
                    message: format!("Expected statement separator, found {:?}", self.peek_kind()),
                    span: self.current().span,
                });
            }
            self.skip_terminators();
        }

        let end_span = self.current().span;
        let span = start_span.merge(end_span);

        if stmts.len() == 1 {
            Ok(stmts[0])
        } else {
            Ok(self.arena.alloc(ASTNode::StatementList {
                statements: stmts,
                span,
            }))
        }
    }

    pub fn parse_statement(&mut self) -> Result<&'a ASTNode<'a>, ParseError> {
        self.parse_logical_chain()
    }

    fn parse_logical_chain(&mut self) -> Result<&'a ASTNode<'a>, ParseError> {
        let mut left = self.parse_parallel_chain()?;

        while matches!(self.peek_kind(), TokenKind::AndIf | TokenKind::OrIf) {
            let op_tok = self.advance();
            self.skip_newlines();
            let right = self.parse_parallel_chain()?;
            let span = left.span().merge(right.span());

            let op = match op_tok.kind {
                TokenKind::AndIf => LogicalOp::And,
                TokenKind::OrIf => LogicalOp::Or,
                _ => unreachable!(),
            };

            // Detect FailFastBlock syntax: { try } || { catch }
            let left_is_fail_fast_block = matches!(left, ASTNode::Block { .. })
                || matches!(
                    left,
                    ASTNode::RedirectedCompound {
                        inner,
                        ..
                    } if matches!(inner, ASTNode::Block { .. })
                );
            if op == LogicalOp::Or && left_is_fail_fast_block {
                left = self.arena.alloc(ASTNode::FailFastBlock {
                    try_stage: left,
                    fallback: Some(right),
                    span,
                });
            } else {
                left = self.arena.alloc(ASTNode::LogicalChain {
                    left,
                    op,
                    right,
                    span,
                });
            }
        }

        Ok(left)
    }

    fn parse_parallel_chain(&mut self) -> Result<&'a ASTNode<'a>, ParseError> {
        let first = self.parse_pipeline()?;

        if self.peek_kind() == TokenKind::ParallelPipe {
            let mut branches = BumpVec::new_in(self.arena);
            branches.push(first);
            let start_span = first.span();

            while self.peek_kind() == TokenKind::ParallelPipe {
                self.advance(); // consume '&|'
                self.skip_newlines();
                let next = self.parse_pipeline()?;
                branches.push(next);
            }

            let last_span = branches.last().map(|b| b.span()).unwrap_or(start_span);
            let span = start_span.merge(last_span);

            Ok(self
                .arena
                .alloc(ASTNode::ParallelPipeline { branches, span }))
        } else {
            Ok(first)
        }
    }

    fn parse_pipeline(&mut self) -> Result<&'a ASTNode<'a>, ParseError> {
        let is_negated = if self.peek_kind() == TokenKind::Bang {
            self.advance();
            self.skip_newlines();
            true
        } else {
            false
        };

        let first = self.parse_command_or_block()?;

        let node = if self.peek_kind() == TokenKind::Pipe {
            let mut stages = BumpVec::new_in(self.arena);
            stages.push(first);
            let start_span = first.span();

            while self.peek_kind() == TokenKind::Pipe {
                self.advance(); // consume '|'
                self.skip_newlines();
                let next = self.parse_command_or_block()?;
                stages.push(next);
            }

            let last_span = stages.last().map(|s| s.span()).unwrap_or(start_span);
            let span = start_span.merge(last_span);

            self.arena.alloc(ASTNode::Pipeline { stages, span })
        } else {
            first
        };

        if is_negated {
            let span = node.span();
            Ok(self.arena.alloc(ASTNode::Negation { inner: node, span }))
        } else {
            Ok(node)
        }
    }

    fn parse_command_or_block(&mut self) -> Result<&'a ASTNode<'a>, ParseError> {
        let inner = match self.peek_kind() {
            TokenKind::If => self.parse_if()?,
            TokenKind::For => self.parse_for()?,
            TokenKind::While => self.parse_while_or_until(false)?,
            TokenKind::Until => self.parse_while_or_until(true)?,
            TokenKind::Case => self.parse_case()?,
            TokenKind::LBrace => self.parse_block()?,
            TokenKind::LParen => self.parse_subshell()?,
            TokenKind::Then
            | TokenKind::Elif
            | TokenKind::Else
            | TokenKind::Fi
            | TokenKind::In
            | TokenKind::Do
            | TokenKind::Done
            | TokenKind::Esac => {
                return Err(ParseError {
                    message: format!("Unexpected reserved word {:?}", self.peek_kind()),
                    span: self.current().span,
                });
            }
            _ => return self.parse_simple_command_or_func(),
        };

        if !self.is_redirection_token(self.peek_kind()) {
            return Ok(inner);
        }

        let mut redirections = BumpVec::new_in(self.arena);
        while self.is_redirection_token(self.peek_kind()) {
            redirections.push(self.parse_redirection()?);
        }
        let span = inner
            .span()
            .merge(redirections.last().expect("one redirection parsed").span);
        Ok(self.arena.alloc(ASTNode::RedirectedCompound {
            inner,
            redirections,
            span,
        }))
    }

    fn parse_if(&mut self) -> Result<&'a ASTNode<'a>, ParseError> {
        let if_tok = self.expect(TokenKind::If)?;
        self.skip_terminators();

        let condition = self.parse_statement_list_until(&[TokenKind::Then])?;
        self.expect(TokenKind::Then)?;
        self.skip_terminators();

        let then_branch =
            self.parse_statement_list_until(&[TokenKind::Elif, TokenKind::Else, TokenKind::Fi])?;

        let mut elif_branches = BumpVec::new_in(self.arena);
        while self.peek_kind() == TokenKind::Elif {
            self.advance(); // consume `elif`
            self.skip_terminators();
            let elif_cond = self.parse_statement_list_until(&[TokenKind::Then])?;
            self.expect(TokenKind::Then)?;
            self.skip_terminators();
            let elif_body = self.parse_statement_list_until(&[
                TokenKind::Elif,
                TokenKind::Else,
                TokenKind::Fi,
            ])?;
            elif_branches.push((elif_cond, elif_body));
        }

        let else_branch = if self.peek_kind() == TokenKind::Else {
            self.advance(); // consume `else`
            self.skip_terminators();
            let else_body = self.parse_statement_list_until(&[TokenKind::Fi])?;
            Some(else_body)
        } else {
            None
        };

        let fi_tok = self.expect(TokenKind::Fi)?;
        let span = if_tok.span.merge(fi_tok.span);

        Ok(self.arena.alloc(ASTNode::If {
            condition,
            then_branch,
            elif_branches,
            else_branch,
            span,
        }))
    }

    fn parse_for(&mut self) -> Result<&'a ASTNode<'a>, ParseError> {
        let for_tok = self.expect(TokenKind::For)?;

        let var_tok = self.advance();
        let var_name = match var_tok.kind {
            TokenKind::Word(w) => w,
            other => {
                return Err(ParseError {
                    message: format!("Expected variable name after 'for', found {:?}", other),
                    span: var_tok.span,
                });
            }
        };

        self.skip_newlines();
        let words = if self.peek_kind() == TokenKind::In {
            self.advance(); // consume `in`
            let mut word_list = BumpVec::new_in(self.arena);
            while !matches!(
                self.peek_kind(),
                TokenKind::Newline | TokenKind::Semicolon | TokenKind::Do | TokenKind::Eof
            ) {
                let w = self.parse_word()?;
                word_list.push(w);
            }
            if matches!(self.peek_kind(), TokenKind::Newline | TokenKind::Semicolon) {
                self.advance();
            }
            Some(word_list)
        } else if matches!(self.peek_kind(), TokenKind::Newline | TokenKind::Semicolon) {
            self.advance();
            None
        } else {
            None
        };

        self.skip_terminators();
        self.expect(TokenKind::Do)?;
        self.skip_terminators();

        let body = self.parse_statement_list_until(&[TokenKind::Done])?;
        let done_tok = self.expect(TokenKind::Done)?;
        let span = for_tok.span.merge(done_tok.span);

        Ok(self.arena.alloc(ASTNode::For {
            var: var_name,
            words,
            body,
            span,
        }))
    }

    fn parse_while_or_until(&mut self, is_until: bool) -> Result<&'a ASTNode<'a>, ParseError> {
        let start_tok = if is_until {
            self.expect(TokenKind::Until)?
        } else {
            self.expect(TokenKind::While)?
        };
        self.skip_terminators();

        let condition = self.parse_statement_list_until(&[TokenKind::Do])?;
        self.expect(TokenKind::Do)?;
        self.skip_terminators();

        let body = self.parse_statement_list_until(&[TokenKind::Done])?;
        let done_tok = self.expect(TokenKind::Done)?;
        let span = start_tok.span.merge(done_tok.span);

        Ok(self.arena.alloc(ASTNode::While {
            condition,
            body,
            is_until,
            span,
        }))
    }

    fn parse_case(&mut self) -> Result<&'a ASTNode<'a>, ParseError> {
        let case_tok = self.expect(TokenKind::Case)?;
        self.skip_terminators();

        let target = self.parse_word()?;
        self.skip_terminators();

        self.expect(TokenKind::In)?;
        self.skip_terminators();

        let mut arms = BumpVec::new_in(self.arena);
        while self.peek_kind() != TokenKind::Esac && self.peek_kind() != TokenKind::Eof {
            // Optional leading '('
            if self.peek_kind() == TokenKind::LParen {
                self.advance();
            }

            let start_arm_span = self.current().span;
            let mut patterns = BumpVec::new_in(self.arena);
            let first_pat = self.parse_word()?;
            patterns.push(first_pat);

            while self.peek_kind() == TokenKind::Pipe {
                self.advance(); // consume '|'
                let next_pat = self.parse_word()?;
                patterns.push(next_pat);
            }

            self.expect(TokenKind::RParen)?;
            self.skip_terminators();

            let body = if matches!(
                self.peek_kind(),
                TokenKind::DoubleSemi
                    | TokenKind::SemiAnd
                    | TokenKind::DoubleSemiAnd
                    | TokenKind::Esac
            ) {
                None
            } else {
                let stmt = self.parse_statement_list_until(&[
                    TokenKind::DoubleSemi,
                    TokenKind::SemiAnd,
                    TokenKind::DoubleSemiAnd,
                    TokenKind::Esac,
                ])?;
                Some(stmt)
            };

            let delimiter = match self.peek_kind() {
                TokenKind::DoubleSemi => {
                    self.advance();
                    CaseDelimiter::Break
                }
                TokenKind::SemiAnd => {
                    self.advance();
                    CaseDelimiter::Fallthrough
                }
                TokenKind::DoubleSemiAnd => {
                    self.advance();
                    CaseDelimiter::ResumeNext
                }
                _ => CaseDelimiter::Break,
            };

            let end_arm_span = self.current().span;
            let span = start_arm_span.merge(end_arm_span);

            arms.push(CaseArm {
                patterns,
                body,
                delimiter,
                span,
            });

            self.skip_terminators();
        }

        let esac_tok = self.expect(TokenKind::Esac)?;
        let span = case_tok.span.merge(esac_tok.span);

        Ok(self.arena.alloc(ASTNode::Case { target, arms, span }))
    }

    fn parse_statement_list_until(
        &mut self,
        terminators: &[TokenKind<'a>],
    ) -> Result<&'a ASTNode<'a>, ParseError> {
        self.skip_terminators();
        let mut stmts = BumpVec::new_in(self.arena);
        let start_span = self.current().span;

        while !terminators.contains(&self.peek_kind()) && self.peek_kind() != TokenKind::Eof {
            let stmt = self.parse_statement()?;
            stmts.push(stmt);
            if terminators.contains(&self.peek_kind()) {
                break;
            }
            if !matches!(self.peek_kind(), TokenKind::Newline | TokenKind::Semicolon) {
                return Err(ParseError {
                    message: format!(
                        "Expected statement separator before {:?}, found {:?}",
                        terminators,
                        self.peek_kind()
                    ),
                    span: self.current().span,
                });
            }
            self.skip_terminators();
        }

        let end_span = self.current().span;
        let span = start_span.merge(end_span);

        if stmts.len() == 1 {
            Ok(stmts[0])
        } else {
            Ok(self.arena.alloc(ASTNode::StatementList {
                statements: stmts,
                span,
            }))
        }
    }

    fn parse_block(&mut self) -> Result<&'a ASTNode<'a>, ParseError> {
        let lbrace = self.expect(TokenKind::LBrace)?;
        self.skip_terminators();

        let mut stmts = BumpVec::new_in(self.arena);
        while self.peek_kind() != TokenKind::RBrace && self.peek_kind() != TokenKind::Eof {
            let stmt = self.parse_statement()?;
            stmts.push(stmt);
            if self.peek_kind() == TokenKind::RBrace {
                break;
            }
            if !matches!(self.peek_kind(), TokenKind::Newline | TokenKind::Semicolon) {
                return Err(ParseError {
                    message: format!(
                        "Expected statement separator in block, found {:?}",
                        self.peek_kind()
                    ),
                    span: self.current().span,
                });
            }
            self.skip_terminators();
        }

        let rbrace = self.expect(TokenKind::RBrace)?;
        let span = lbrace.span.merge(rbrace.span);

        Ok(self.arena.alloc(ASTNode::Block {
            statements: stmts,
            span,
        }))
    }

    fn parse_subshell(&mut self) -> Result<&'a ASTNode<'a>, ParseError> {
        let lparen = self.expect(TokenKind::LParen)?;
        self.skip_terminators();

        let mut stmts = BumpVec::new_in(self.arena);
        while self.peek_kind() != TokenKind::RParen && self.peek_kind() != TokenKind::Eof {
            let stmt = self.parse_statement()?;
            stmts.push(stmt);
            if self.peek_kind() == TokenKind::RParen {
                break;
            }
            if !matches!(self.peek_kind(), TokenKind::Newline | TokenKind::Semicolon) {
                return Err(ParseError {
                    message: format!(
                        "Expected statement separator in subshell, found {:?}",
                        self.peek_kind()
                    ),
                    span: self.current().span,
                });
            }
            self.skip_terminators();
        }

        let rparen = self.expect(TokenKind::RParen)?;
        let span = lparen.span.merge(rparen.span);

        Ok(self.arena.alloc(ASTNode::Subshell {
            statements: stmts,
            span,
        }))
    }

    fn parse_function_body(&mut self) -> Result<&'a ASTNode<'a>, ParseError> {
        if matches!(
            self.peek_kind(),
            TokenKind::If
                | TokenKind::For
                | TokenKind::While
                | TokenKind::Until
                | TokenKind::Case
                | TokenKind::LBrace
                | TokenKind::LParen
        ) {
            self.parse_command_or_block()
        } else {
            Err(ParseError {
                message: format!(
                    "Expected compound command as function body, found {:?}",
                    self.peek_kind()
                ),
                span: self.current().span,
            })
        }
    }

    fn parse_simple_command_or_func(&mut self) -> Result<&'a ASTNode<'a>, ParseError> {
        // Check for function declaration: `func_name() { ... }` or `function func_name { ... }` or `function func_name() { ... }`
        if let TokenKind::Word(word) = self.peek_kind() {
            if word == "function" {
                let func_kw_tok = self.advance(); // consume 'function'
                if let TokenKind::Word(name) = self.peek_kind() {
                    let _name_tok = self.advance();
                    // optional '()'
                    if self.peek_kind() == TokenKind::LParen
                        && self.peek_kind_at(1) == TokenKind::RParen
                    {
                        self.advance();
                        self.advance();
                    }
                    self.skip_terminators();
                    let body = self.parse_block()?;
                    let span = func_kw_tok.span.merge(body.span());
                    return Ok(self
                        .arena
                        .alloc(ASTNode::FunctionDeclaration { name, body, span }));
                }
            } else if self.peek_kind_at(1) == TokenKind::LParen
                && self.peek_kind_at(2) == TokenKind::RParen
            {
                let name = word;
                let name_tok = self.advance();
                self.advance(); // consume '('
                self.advance(); // consume ')'
                self.skip_terminators();
                let body = self.parse_function_body()?;
                let span = name_tok.span.merge(body.span());
                return Ok(self
                    .arena
                    .alloc(ASTNode::FunctionDeclaration { name, body, span }));
            }
        }

        let mut assignments = BumpVec::new_in(self.arena);
        let mut args = BumpVec::new_in(self.arena);
        let mut redirections = BumpVec::new_in(self.arena);
        let mut cmd_name: Option<Word<'a>> = None;

        let start_span = self.current().span;

        while !self.is_at_command_end() {
            if self.is_redirection_token(self.peek_kind()) {
                let redir = self.parse_redirection()?;
                redirections.push(redir);
                continue;
            }

            // Check if this is an assignment before command name
            if cmd_name.is_none()
                && let TokenKind::Word(w) = self.peek_kind()
                && let Some(eq_idx) = w.find('=')
            {
                let lhs = &w[..eq_idx];
                let val_part = &w[eq_idx + 1..];
                let tok = self.advance();

                let (var_name, index) = if let Some(sub_start) = lhs.find('[') {
                    if lhs.ends_with(']') {
                        let name = &lhs[..sub_start];
                        let sub = &lhs[sub_start + 1..lhs.len() - 1];
                        let mut sub_parts = BumpVec::new_in(self.arena);
                        sub_parts.push(WordPart::Literal(sub));
                        (
                            name,
                            Some(Word {
                                parts: sub_parts,
                                span: tok.span,
                            }),
                        )
                    } else {
                        (lhs, None)
                    }
                } else {
                    (lhs, None)
                };

                if val_part.is_empty() && self.peek_kind() == TokenKind::LParen {
                    self.advance(); // consume '('
                    self.skip_terminators();
                    let mut array_vals = BumpVec::new_in(self.arena);
                    while self.peek_kind() != TokenKind::RParen
                        && self.peek_kind() != TokenKind::Eof
                    {
                        let item = self.parse_word()?;
                        array_vals.push(item);
                        self.skip_terminators();
                    }
                    self.expect(TokenKind::RParen)?;

                    assignments.push(Assignment {
                        name: var_name,
                        index: None,
                        value: None,
                        array_values: Some(array_vals),
                        span: tok.span,
                    });
                    continue;
                }

                let mut parts = BumpVec::new_in(self.arena);
                if !val_part.is_empty() {
                    parts.push(WordPart::Literal(val_part));
                } else if matches!(
                    self.peek_kind(),
                    TokenKind::SingleQuoted(_)
                        | TokenKind::DoubleQuoted(_)
                        | TokenKind::Word(_)
                        | TokenKind::Variable(_)
                        | TokenKind::CommandSub(_)
                ) {
                    let next_word = self.parse_word()?;
                    for p in next_word.parts {
                        parts.push(p);
                    }
                }

                assignments.push(Assignment {
                    name: var_name,
                    index,
                    value: Some(Word {
                        parts,
                        span: tok.span,
                    }),
                    array_values: None,
                    span: tok.span,
                });
                continue;
            }

            let word = self.parse_word()?;
            if cmd_name.is_none() {
                cmd_name = Some(word);
            } else {
                args.push(word);
            }
        }

        let end_span = self.current().span;
        let span = start_span.merge(end_span);

        if let Some(name) = cmd_name {
            Ok(self.arena.alloc(ASTNode::Command {
                name,
                args,
                assignments,
                redirections,
                span,
            }))
        } else if !assignments.is_empty() {
            // Standalone assignment (e.g. `VAR=123`)
            let first_asgn = &assignments[0];
            let name_parts = {
                let mut p = BumpVec::new_in(self.arena);
                p.push(WordPart::Literal("="));
                p
            };
            Ok(self.arena.alloc(ASTNode::Command {
                name: Word {
                    parts: name_parts,
                    span: first_asgn.span,
                },
                args,
                assignments,
                redirections,
                span,
            }))
        } else if !redirections.is_empty() {
            // A redirection-only simple command applies its redirects and
            // succeeds without dispatching a command.
            let mut name_parts = BumpVec::new_in(self.arena);
            name_parts.push(WordPart::Literal("="));
            Ok(self.arena.alloc(ASTNode::Command {
                name: Word {
                    parts: name_parts,
                    span: start_span,
                },
                args,
                assignments,
                redirections,
                span,
            }))
        } else {
            Err(ParseError {
                message: format!(
                    "Expected command or expression, found {:?}",
                    self.peek_kind()
                ),
                span: self.current().span,
            })
        }
    }

    fn is_redirection_token(&self, kind: TokenKind<'a>) -> bool {
        matches!(
            kind,
            TokenKind::RedirectOut
                | TokenKind::RedirectClobber
                | TokenKind::RedirectAppend
                | TokenKind::RedirectIn
                | TokenKind::RedirectHereDoc
                | TokenKind::RedirectHereDocStrip
                | TokenKind::RedirectHereString
                | TokenKind::RedirectStderr
                | TokenKind::RedirectStderrAppend
                | TokenKind::RedirectStderrToStdout
                | TokenKind::RedirectStdoutToStderr
                | TokenKind::RedirectAll
                | TokenKind::RedirectAllAppend
        )
    }

    fn parse_redirection(&mut self) -> Result<Redirection<'a>, ParseError> {
        let op_tok = self.advance();
        let op = match op_tok.kind {
            TokenKind::RedirectOut => RedirectionOp::RedirectOut,
            TokenKind::RedirectClobber => RedirectionOp::RedirectClobber,
            TokenKind::RedirectAppend => RedirectionOp::RedirectAppend,
            TokenKind::RedirectIn => RedirectionOp::RedirectIn,
            TokenKind::RedirectHereDoc => RedirectionOp::RedirectHereDoc,
            TokenKind::RedirectHereDocStrip => RedirectionOp::RedirectHereDocStrip,
            TokenKind::RedirectHereString => RedirectionOp::RedirectHereString,
            TokenKind::RedirectStderr => RedirectionOp::RedirectStderr,
            TokenKind::RedirectStderrAppend => RedirectionOp::RedirectStderrAppend,
            TokenKind::RedirectStderrToStdout => RedirectionOp::RedirectStderrToStdout,
            TokenKind::RedirectStdoutToStderr => RedirectionOp::RedirectStdoutToStderr,
            TokenKind::RedirectAll => RedirectionOp::RedirectAll,
            TokenKind::RedirectAllAppend => RedirectionOp::RedirectAllAppend,
            _ => unreachable!(),
        };

        let target = if self.is_at_command_end()
            || self.is_redirection_token(self.peek_kind())
            || matches!(
                self.peek_kind(),
                TokenKind::RBrace | TokenKind::RParen | TokenKind::Semicolon | TokenKind::Newline
            ) {
            let mut parts = BumpVec::new_in(self.arena);
            parts.push(WordPart::Literal(""));
            Word {
                parts,
                span: op_tok.span,
            }
        } else {
            self.parse_word()?
        };
        let span = op_tok.span.merge(target.span);

        Ok(Redirection { op, target, span })
    }

    fn parse_word(&mut self) -> Result<Word<'a>, ParseError> {
        let mut parts = BumpVec::new_in(self.arena);
        let start_span = self.current().span;
        let mut last_span = start_span;

        while !self.is_at_command_end() {
            let curr_span = self.current().span;
            if !parts.is_empty() && last_span.end != curr_span.start {
                break;
            }

            let reserved_word = match self.peek_kind() {
                TokenKind::If => Some("if"),
                TokenKind::Then => Some("then"),
                TokenKind::Elif => Some("elif"),
                TokenKind::Else => Some("else"),
                TokenKind::Fi => Some("fi"),
                TokenKind::For => Some("for"),
                TokenKind::In => Some("in"),
                TokenKind::While => Some("while"),
                TokenKind::Until => Some("until"),
                TokenKind::Do => Some("do"),
                TokenKind::Done => Some("done"),
                TokenKind::Case => Some("case"),
                TokenKind::Esac => Some("esac"),
                _ => None,
            };
            if let Some(word) = reserved_word {
                let tok = self.advance();
                parts.push(WordPart::Literal(word));
                last_span = tok.span;
                continue;
            }

            match self.peek_kind() {
                TokenKind::Word(s) => {
                    let tok = self.advance();
                    parts.push(WordPart::Literal(s));
                    last_span = tok.span;
                }
                TokenKind::SingleQuoted(s) => {
                    let tok = self.advance();
                    parts.push(WordPart::SingleQuoted(s));
                    last_span = tok.span;
                }
                TokenKind::DoubleQuoted(s) => {
                    let tok = self.advance();
                    parts.push(WordPart::DoubleQuoted(s));
                    last_span = tok.span;
                }
                TokenKind::Variable(s) => {
                    let tok = self.advance();
                    parts.push(WordPart::Variable(s));
                    last_span = tok.span;
                }
                TokenKind::CommandSub(s) => {
                    let tok = self.advance();
                    parts.push(WordPart::CommandSub(s));
                    last_span = tok.span;
                }
                TokenKind::Bang => {
                    let tok = self.advance();
                    parts.push(WordPart::Literal("!"));
                    last_span = tok.span;
                }
                TokenKind::HereDocDelimiter {
                    body,
                    quoted,
                    strip_tabs,
                } => {
                    let tok = self.advance();
                    parts.push(WordPart::HereDocBody(body, quoted, strip_tabs));
                    last_span = tok.span;
                }
                _ => break,
            }
        }

        if parts.is_empty() {
            return Err(ParseError {
                message: format!("Expected word or argument, found {:?}", self.peek_kind()),
                span: start_span,
            });
        }

        let span = start_span.merge(last_span);
        Ok(Word { parts, span })
    }

    fn is_at_command_end(&self) -> bool {
        matches!(
            self.peek_kind(),
            TokenKind::Newline
                | TokenKind::Semicolon
                | TokenKind::Pipe
                | TokenKind::ParallelPipe
                | TokenKind::AndIf
                | TokenKind::OrIf
                | TokenKind::DoubleSemi
                | TokenKind::SemiAnd
                | TokenKind::DoubleSemiAnd
                | TokenKind::RBrace
                | TokenKind::RParen
                | TokenKind::Eof
        )
    }
}
