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
use esh::planner::compiler::compile_script;

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
fn loop_and_function_names_follow_shell_identifier_syntax() {
    for (script, invalid_name, expected_message) in [
        (
            "for 1ITEM in value; do :; done",
            "1ITEM",
            "Invalid variable name after 'for'",
        ),
        (
            "for ITEM-NAME in value; do :; done",
            "ITEM-NAME",
            "Invalid variable name after 'for'",
        ),
        ("1deploy() { :; }", "1deploy", "Invalid function name"),
        ("deploy-now() { :; }", "deploy-now", "Invalid function name"),
        (
            "function 1deploy { :; }",
            "1deploy",
            "Invalid function name",
        ),
    ] {
        let error = compile_script(script).expect_err("invalid identifier should not compile");
        assert!(error.message.contains(expected_message), "{error}");
        let expected_start = script.find(invalid_name).unwrap() as u32;
        assert_eq!(error.span.start, expected_start, "{error}");
        assert_eq!(
            error.span.end - error.span.start,
            invalid_name.len() as u32,
            "diagnostic should cover the invalid name: {error}"
        );
    }

    for script in [
        "for _ITEM2 in value; do :; done",
        "deploy_2() { :; }",
        "function rollback_2 { :; }",
        "function deploy_2() { :; }",
    ] {
        compile_script(script).unwrap_or_else(|error| {
            panic!("valid shell identifier should compile: {script}: {error}")
        });
    }
}

#[test]
fn compound_commands_require_nonempty_lists_and_separators() {
    for (script, expected_message) in [
        ("if then :; fi", "Expected command before Then"),
        ("if true; then fi", "Expected command before Fi"),
        ("if true; then :; else fi", "Expected command before Fi"),
        ("if true then :; fi", "Unexpected reserved word Fi"),
        ("if true; then : fi", "Expected Fi, found Eof"),
        ("while do :; done", "Expected command before Do"),
        ("while true; do done", "Expected command before Done"),
        ("for item in value; do done", "Expected command before Done"),
        (
            "for item in value do :; done",
            "Expected statement separator before Do",
        ),
        (
            "for item in do :; done",
            "Expected statement separator before Do",
        ),
        (
            "for item\n; do :; done",
            "Unexpected semicolon after line break in for header",
        ),
        ("case value in value) : esac", "Expected Esac, found Eof"),
        ("if true; then :;", "Expected Fi, found Eof"),
        ("()", "Expected command inside subshell"),
        ("{ :}", "Expected statement separator before '}'"),
    ] {
        let error = compile_script(script).expect_err("malformed command should not compile");
        assert!(
            error.message.contains(expected_message),
            "{script}: {error}"
        );
    }

    for script in [
        "if true; then :; fi",
        "if true; then :; else :; fi",
        "while false; do :; done",
        "for item in; do :; done",
        "for item; do :; done",
        "case value in esac",
        "case value in value) ;; esac",
        "case value in value) :; esac",
        "{ }",
    ] {
        compile_script(script).unwrap_or_else(|error| {
            panic!("valid compound command should compile: {script}: {error}")
        });
    }
}

#[test]
fn command_list_continuations_require_a_command_after_the_operator() {
    for script in ["true &&\n", "false ||\n", "printf value |\n", "!\n"] {
        assert!(
            compile_script(script).is_err(),
            "an incomplete continuation must not compile: {script:?}"
        );
    }
}

#[test]
fn semicolons_do_not_create_empty_commands() {
    for script in [
        "; :",
        ":; ; :",
        "if; true; then :; fi",
        "if true; ; then :; fi",
        "while true; ; do break; done",
        "for item in value; ; do :; done",
        "for item in value; do; :; done",
        "{ ; :; }",
        "(; :)",
    ] {
        assert!(
            compile_script(script).is_err(),
            "accepted an empty command separator in {script:?}"
        );
    }

    for script in [
        "\n\n:; :",
        "if\n\ntrue; then\n\n:; fi",
        "while\n\ntrue; do\n\nbreak; done",
        "for item in value;\n\ndo :; done",
        "{\n\n:;\n}",
        "(\n\n:;\n)",
    ] {
        compile_script(script)
            .unwrap_or_else(|error| panic!("rejected valid blank lines in {script:?}: {error}"));
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

#[test]
fn redirections_require_targets_except_for_descriptor_duplication() {
    for script in [
        "echo >",
        "echo >\nresult.txt",
        "echo <;",
        "echo > > result.txt",
        "cat <<",
    ] {
        assert!(
            compile_script(script).is_err(),
            "accepted a redirection without a target: {script:?}"
        );
    }

    for script in [
        "echo >&2",
        "echo 2>&1",
        "echo > result.txt",
        "cat <<EOF\nbody\nEOF",
    ] {
        compile_script(script)
            .unwrap_or_else(|error| panic!("rejected valid redirection {script:?}: {error}"));
    }
}
