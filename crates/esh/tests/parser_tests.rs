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

use bumpalo::Bump;
use esh::lexer::Lexer;
use esh::parser::Parser;
use esh::parser::ast::{ASTNode, LogicalOp, RedirectionOp};

#[test]
fn test_parser_simple_command() {
    let script = "mkdir -p ./dist";
    let tokens = Lexer::new(script).tokenize_all().unwrap();
    let arena = Bump::new();
    let mut parser = Parser::new(&arena, tokens);
    let ast = parser.parse_script().unwrap();

    match ast {
        ASTNode::Command { name, args, .. } => {
            assert_eq!(name.parts.len(), 1);
            assert_eq!(args.len(), 2);
        }
        _ => panic!("Expected simple Command, got {:?}", ast),
    }
}

#[test]
fn test_parser_pipeline() {
    let script = "cat file.txt | grep 'pattern' | wc -l";
    let tokens = Lexer::new(script).tokenize_all().unwrap();
    let arena = Bump::new();
    let mut parser = Parser::new(&arena, tokens);
    let ast = parser.parse_script().unwrap();

    match ast {
        ASTNode::Pipeline { stages, .. } => {
            assert_eq!(stages.len(), 3);
        }
        _ => panic!("Expected Pipeline, got {:?}", ast),
    }
}

#[test]
fn test_parser_parallel_pipeline() {
    let script = "cargo build --bin api &| cargo build --bin worker &| npm run build";
    let tokens = Lexer::new(script).tokenize_all().unwrap();
    let arena = Bump::new();
    let mut parser = Parser::new(&arena, tokens);
    let ast = parser.parse_script().unwrap();

    match ast {
        ASTNode::ParallelPipeline { branches, .. } => {
            assert_eq!(branches.len(), 3);
        }
        _ => panic!("Expected ParallelPipeline, got {:?}", ast),
    }
}

#[test]
fn test_parser_logical_chain() {
    let script = "test -f config.json && cat config.json || echo 'missing'";
    let tokens = Lexer::new(script).tokenize_all().unwrap();
    let arena = Bump::new();
    let mut parser = Parser::new(&arena, tokens);
    let ast = parser.parse_script().unwrap();

    match ast {
        ASTNode::LogicalChain {
            left, op, right, ..
        } => {
            assert_eq!(*op, LogicalOp::Or);
            match left {
                ASTNode::LogicalChain { op: left_op, .. } => {
                    assert_eq!(*left_op, LogicalOp::And);
                }
                _ => panic!("Expected left to be LogicalChain (And)"),
            }
            match right {
                ASTNode::Command { .. } => {}
                _ => panic!("Expected right to be Command"),
            }
        }
        _ => panic!("Expected LogicalChain, got {:?}", ast),
    }
}

#[test]
fn test_parser_fail_fast_block() {
    let script = "{ mkdir -p ./dist; cargo build; } || { echo failed; rm -rf ./dist; }";
    let tokens = Lexer::new(script).tokenize_all().unwrap();
    let arena = Bump::new();
    let mut parser = Parser::new(&arena, tokens);
    let ast = parser.parse_script().unwrap();

    match ast {
        ASTNode::FailFastBlock {
            try_stage,
            fallback,
            ..
        } => {
            match try_stage {
                ASTNode::Block { statements, .. } => {
                    assert_eq!(statements.len(), 2);
                }
                _ => panic!("Expected Block for try_stage"),
            }
            assert!(fallback.is_some());
        }
        _ => panic!("Expected FailFastBlock, got {:?}", ast),
    }
}

#[test]
fn test_parser_subshell_and_functions() {
    let script = "deploy() { ( cd ./dist; npm publish ); }";
    let tokens = Lexer::new(script).tokenize_all().unwrap();
    let arena = Bump::new();
    let mut parser = Parser::new(&arena, tokens);
    let ast = parser.parse_script().unwrap();

    match ast {
        ASTNode::FunctionDeclaration { name, body, .. } => {
            assert_eq!(*name, "deploy");
            match body {
                ASTNode::Block { statements, .. } => {
                    assert_eq!(statements.len(), 1);
                    match statements[0] {
                        ASTNode::Subshell {
                            statements: sub_stmts,
                            ..
                        } => {
                            assert_eq!(sub_stmts.len(), 2);
                        }
                        _ => panic!("Expected Subshell"),
                    }
                }
                _ => panic!("Expected Block body"),
            }
        }
        _ => panic!("Expected FunctionDeclaration, got {:?}", ast),
    }
}

#[test]
fn test_parser_redirections() {
    let script = "echo hello > out.txt 2> err.txt";
    let tokens = Lexer::new(script).tokenize_all().unwrap();
    let arena = Bump::new();
    let mut parser = Parser::new(&arena, tokens);
    let ast = parser.parse_script().unwrap();

    match ast {
        ASTNode::Command { redirections, .. } => {
            assert_eq!(redirections.len(), 2);
            assert_eq!(redirections[0].op, RedirectionOp::RedirectOut);
            assert_eq!(redirections[1].op, RedirectionOp::RedirectStderr);
        }
        _ => panic!("Expected Command with redirections"),
    }
}
