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

use crate::lexer::Span;
use bumpalo::collections::Vec as BumpVec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogicalOp {
    And, // &&
    Or,  // ||
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedirectionOp {
    RedirectOut,            // >
    RedirectClobber,        // >|
    RedirectAppend,         // >>
    RedirectIn,             // <
    RedirectHereDoc,        // <<
    RedirectHereDocStrip,   // <<-
    RedirectHereString,     // <<<
    RedirectStderr,         // 2>
    RedirectStderrAppend,   // 2>>
    RedirectStderrToStdout, // 2>&1
    RedirectStdoutToStderr, // 1>&2
    RedirectAll,            // &>
    RedirectAllAppend,      // &>>
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WordPart<'a> {
    Literal(&'a str),
    SingleQuoted(&'a str),
    DoubleQuoted(&'a str),
    Variable(&'a str),
    CommandSub(&'a str),
    HereDocBody(&'a str, bool, bool),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word<'a> {
    pub parts: BumpVec<'a, WordPart<'a>>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Redirection<'a> {
    pub op: RedirectionOp,
    pub target: Word<'a>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assignment<'a> {
    pub name: &'a str,
    pub index: Option<Word<'a>>,
    pub value: Option<Word<'a>>,
    pub array_values: Option<BumpVec<'a, Word<'a>>>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseDelimiter {
    Break,       // ;;
    Fallthrough, // ;&
    ResumeNext,  // ;;&
}

#[derive(Debug)]
pub struct CaseArm<'a> {
    pub patterns: BumpVec<'a, Word<'a>>,
    pub body: Option<&'a ASTNode<'a>>,
    pub delimiter: CaseDelimiter,
    pub span: Span,
}

#[derive(Debug)]
pub enum ASTNode<'a> {
    Command {
        name: Word<'a>,
        args: BumpVec<'a, Word<'a>>,
        assignments: BumpVec<'a, Assignment<'a>>,
        redirections: BumpVec<'a, Redirection<'a>>,
        span: Span,
    },
    RedirectedCompound {
        inner: &'a ASTNode<'a>,
        redirections: BumpVec<'a, Redirection<'a>>,
        span: Span,
    },
    Negation {
        inner: &'a ASTNode<'a>,
        span: Span,
    },
    FunctionDeclaration {
        name: &'a str,
        body: &'a ASTNode<'a>,
        span: Span,
    },
    Block {
        statements: BumpVec<'a, &'a ASTNode<'a>>,
        span: Span,
    },
    Subshell {
        statements: BumpVec<'a, &'a ASTNode<'a>>,
        span: Span,
    },
    Pipeline {
        stages: BumpVec<'a, &'a ASTNode<'a>>,
        span: Span,
    },
    ParallelPipeline {
        branches: BumpVec<'a, &'a ASTNode<'a>>,
        span: Span,
    },
    LogicalChain {
        left: &'a ASTNode<'a>,
        op: LogicalOp,
        right: &'a ASTNode<'a>,
        span: Span,
    },
    FailFastBlock {
        try_stage: &'a ASTNode<'a>,
        fallback: Option<&'a ASTNode<'a>>,
        span: Span,
    },
    If {
        condition: &'a ASTNode<'a>,
        then_branch: &'a ASTNode<'a>,
        elif_branches: BumpVec<'a, (&'a ASTNode<'a>, &'a ASTNode<'a>)>,
        else_branch: Option<&'a ASTNode<'a>>,
        span: Span,
    },
    For {
        var: &'a str,
        words: Option<BumpVec<'a, Word<'a>>>,
        body: &'a ASTNode<'a>,
        span: Span,
    },
    While {
        condition: &'a ASTNode<'a>,
        body: &'a ASTNode<'a>,
        is_until: bool,
        span: Span,
    },
    Case {
        target: Word<'a>,
        arms: BumpVec<'a, CaseArm<'a>>,
        span: Span,
    },
    StatementList {
        statements: BumpVec<'a, &'a ASTNode<'a>>,
        span: Span,
    },
}

impl<'a> ASTNode<'a> {
    pub fn span(&self) -> Span {
        match self {
            ASTNode::Command { span, .. } => *span,
            ASTNode::RedirectedCompound { span, .. } => *span,
            ASTNode::Negation { span, .. } => *span,
            ASTNode::FunctionDeclaration { span, .. } => *span,
            ASTNode::Block { span, .. } => *span,
            ASTNode::Subshell { span, .. } => *span,
            ASTNode::Pipeline { span, .. } => *span,
            ASTNode::ParallelPipeline { span, .. } => *span,
            ASTNode::LogicalChain { span, .. } => *span,
            ASTNode::FailFastBlock { span, .. } => *span,
            ASTNode::If { span, .. } => *span,
            ASTNode::For { span, .. } => *span,
            ASTNode::While { span, .. } => *span,
            ASTNode::Case { span, .. } => *span,
            ASTNode::StatementList { span, .. } => *span,
        }
    }
}
