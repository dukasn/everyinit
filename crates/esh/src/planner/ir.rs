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

use crate::error::ExecError;
use crate::lexer::Span;
use crate::parser::ast::RedirectionOp;
use crate::runtime::context::ExecutionContext;
use crate::runtime::traits::BoxFuture;
use std::fmt;
use std::path::PathBuf;

pub type BuiltinFn = for<'a> fn(
    args: &'a [String],
    ctx: &'a mut ExecutionContext,
) -> BoxFuture<'a, Result<i32, ExecError>>;

#[derive(Clone)]
pub enum CommandTarget {
    Builtin(BuiltinFn),
    External(PathBuf),
    UserFunction { name: String, plan: ExecutionPlan },
    Dynamic(PlanWord),
}

impl fmt::Debug for CommandTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CommandTarget::Builtin(_) => write!(f, "CommandTarget::Builtin(...)"),
            CommandTarget::External(path) => write!(f, "CommandTarget::External({:?})", path),
            CommandTarget::UserFunction { name, .. } => {
                write!(f, "CommandTarget::UserFunction({})", name)
            }
            CommandTarget::Dynamic(word) => write!(f, "CommandTarget::Dynamic({word:?})"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanWordPart {
    Literal(String),
    HomeTilde,
    SingleQuoted(String),
    DoubleQuoted(String),
    CaseQuoted(String),
    CaseDoubleQuoted(String),
    Variable(String),
    CommandSub(String),
    HereDocBody(String, bool, bool),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanWord {
    pub parts: Vec<PlanWordPart>,
    pub span: Span,
}

impl PlanWord {
    pub fn new_literal(s: impl Into<String>, span: Span) -> Self {
        Self {
            parts: vec![PlanWordPart::Literal(s.into())],
            span,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanRedirection {
    pub op: RedirectionOp,
    pub target: PlanWord,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanAssignment {
    pub name: String,
    pub index: Option<PlanWord>,
    pub value: Option<PlanWord>,
    pub array_values: Option<Vec<PlanWord>>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct PlanNode {
    pub target: CommandTarget,
    pub raw_name: String,
    pub args: Vec<PlanWord>,
    pub assignments: Vec<PlanAssignment>,
    pub redirections: Vec<PlanRedirection>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum ExecutionStage {
    /// Atomic command or stream pipe: cmd1 | cmd2
    Pipeline(Vec<PlanNode>),

    /// Pipeline that includes a compound-command stage.
    PipelineStages(Vec<ExecutionStage>),

    /// Parallel join: task1 &| task2
    Parallel(Vec<ExecutionStage>),

    /// Sequential chain: task1 && task2 or task1; task2
    Sequential {
        left: Box<ExecutionStage>,
        require_success: bool,
        right: Box<ExecutionStage>,
    },

    /// Transactional block: { try } || { catch }
    FailFastBlock {
        try_stage: Box<ExecutionStage>,
        fallback: Option<Box<ExecutionStage>>,
    },

    /// Lexical block: { stmt1; stmt2 }
    Block(Vec<ExecutionStage>),

    /// Subshell: ( stmt1; stmt2 )
    Subshell(Vec<ExecutionStage>),

    /// Negation: ! pipeline
    Negation(Box<ExecutionStage>),

    /// Compound command with command-level redirections.
    Redirected {
        inner: Box<ExecutionStage>,
        redirections: Vec<PlanRedirection>,
    },

    /// User function declaration: my_func() { ... }
    FunctionDef { name: String, plan: ExecutionPlan },

    /// Conditional branch: if cond; then ...; elif ...; else ...; fi
    If {
        condition: Box<ExecutionStage>,
        then_branch: Box<ExecutionStage>,
        elif_branches: Vec<(ExecutionStage, ExecutionStage)>,
        else_branch: Option<Box<ExecutionStage>>,
    },

    /// For loop: for var in words...; do ...; done
    ForLoop {
        var: String,
        words: Option<Vec<PlanWord>>,
        body: Box<ExecutionStage>,
    },

    SelectLoop {
        var: String,
        words: Option<Vec<PlanWord>>,
        body: Box<ExecutionStage>,
    },

    /// While / Until loop: while/until cond; do ...; done
    WhileLoop {
        condition: Box<ExecutionStage>,
        body: Box<ExecutionStage>,
        is_until: bool,
    },

    /// Case branch: case target in ... esac
    CaseBranch {
        target: PlanWord,
        arms: Vec<PlanCaseArm>,
    },
}

#[derive(Debug, Clone)]
pub struct PlanCaseArm {
    pub patterns: Vec<PlanWord>,
    pub body: Option<Box<ExecutionStage>>,
    pub delimiter: crate::parser::ast::CaseDelimiter,
}

#[derive(Debug, Clone)]
pub struct ExecutionPlan {
    pub stages: Vec<ExecutionStage>,
}
