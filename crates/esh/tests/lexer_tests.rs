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

use esh::lexer::lut::{
    CHAR_DELIMITER, CHAR_OPERATOR, CHAR_QUOTE, CHAR_SIGIL, CHAR_TERMINATOR, CHAR_WHITESPACE,
    CHAR_WORD, is_char_kind,
};
use esh::lexer::{Lexer, TokenKind};

#[test]
fn test_ascii_lut_classification() {
    assert!(is_char_kind(b'a', CHAR_WORD));
    assert!(is_char_kind(b'Z', CHAR_WORD));
    assert!(is_char_kind(b'9', CHAR_WORD));
    assert!(is_char_kind(b'_', CHAR_WORD));
    assert!(is_char_kind(b'-', CHAR_WORD));
    assert!(is_char_kind(b'/', CHAR_WORD));

    assert!(is_char_kind(b' ', CHAR_WHITESPACE));
    assert!(is_char_kind(b'\t', CHAR_WHITESPACE));
    assert!(is_char_kind(b'\r', CHAR_WHITESPACE));

    assert!(is_char_kind(b'\n', CHAR_TERMINATOR));
    assert!(is_char_kind(b';', CHAR_TERMINATOR));

    assert!(is_char_kind(b'|', CHAR_OPERATOR));
    assert!(is_char_kind(b'&', CHAR_OPERATOR));
    assert!(is_char_kind(b'>', CHAR_OPERATOR));
    assert!(is_char_kind(b'<', CHAR_OPERATOR));

    assert!(is_char_kind(b'{', CHAR_DELIMITER));
    assert!(is_char_kind(b'}', CHAR_DELIMITER));
    assert!(is_char_kind(b'(', CHAR_DELIMITER));
    assert!(is_char_kind(b')', CHAR_DELIMITER));

    assert!(is_char_kind(b'$', CHAR_SIGIL));
    assert!(is_char_kind(b'"', CHAR_QUOTE));
    assert!(is_char_kind(b'\'', CHAR_QUOTE));
}

#[test]
fn test_lexer_simple_command() {
    let script = "mkdir -p ./dist";
    let tokens = Lexer::new(script)
        .tokenize_all()
        .expect("tokenization failed");

    assert_eq!(tokens.len(), 4); // [Word("mkdir"), Word("-p"), Word("./dist"), Eof]
    assert_eq!(tokens[0].kind, TokenKind::Word("mkdir"));
    assert_eq!(tokens[1].kind, TokenKind::Word("-p"));
    assert_eq!(tokens[2].kind, TokenKind::Word("./dist"));
    assert_eq!(tokens[3].kind, TokenKind::Eof);

    assert_eq!(tokens[0].span.start, 0);
    assert_eq!(tokens[0].span.end, 5);
    assert_eq!(tokens[0].span.line, 1);
    assert_eq!(tokens[0].span.column, 1);
}

#[test]
fn test_lexer_operators_and_delimiters() {
    let script = "cargo build &| npm test && echo done || exit 1;";
    let tokens = Lexer::new(script)
        .tokenize_all()
        .expect("tokenization failed");

    let kinds: Vec<TokenKind> = tokens.into_iter().map(|t| t.kind).collect();
    assert_eq!(
        kinds,
        vec![
            TokenKind::Word("cargo"),
            TokenKind::Word("build"),
            TokenKind::ParallelPipe,
            TokenKind::Word("npm"),
            TokenKind::Word("test"),
            TokenKind::AndIf,
            TokenKind::Word("echo"),
            TokenKind::Done,
            TokenKind::OrIf,
            TokenKind::Word("exit"),
            TokenKind::Word("1"),
            TokenKind::Semicolon,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn test_lexer_quotes_and_variables() {
    let script = "echo 'literal string' \"hello $USER\" $PATH ${CONFIG_DIR} $? $@ $1";
    let tokens = Lexer::new(script)
        .tokenize_all()
        .expect("tokenization failed");

    let kinds: Vec<TokenKind> = tokens.into_iter().map(|t| t.kind).collect();
    assert_eq!(
        kinds,
        vec![
            TokenKind::Word("echo"),
            TokenKind::SingleQuoted("literal string"),
            TokenKind::DoubleQuoted("hello $USER"),
            TokenKind::Variable("PATH"),
            TokenKind::Variable("CONFIG_DIR"),
            TokenKind::Variable("?"),
            TokenKind::Variable("@"),
            TokenKind::Variable("1"),
            TokenKind::Eof,
        ]
    );
}

#[test]
fn test_lexer_command_substitution() {
    let script = "BUILD_REV=$(git rev-parse HEAD)";
    let tokens = Lexer::new(script)
        .tokenize_all()
        .expect("tokenization failed");

    assert_eq!(tokens.len(), 3);
    assert_eq!(tokens[0].kind, TokenKind::Word("BUILD_REV="));
    assert_eq!(tokens[1].kind, TokenKind::CommandSub("git rev-parse HEAD"));
    assert_eq!(tokens[2].kind, TokenKind::Eof);

    let script2 = "echo $(git rev-parse HEAD)";
    let tokens2 = Lexer::new(script2)
        .tokenize_all()
        .expect("tokenization failed");
    assert_eq!(tokens2[0].kind, TokenKind::Word("echo"));
    assert_eq!(tokens2[1].kind, TokenKind::CommandSub("git rev-parse HEAD"));
}

#[test]
fn test_lexer_comments_and_line_continuations() {
    let script = "# Initial comment\nmkdir \\\n  -p ./build # trailing comment\n";
    let tokens = Lexer::new(script)
        .tokenize_all()
        .expect("tokenization failed");

    let kinds: Vec<TokenKind> = tokens.into_iter().map(|t| t.kind).collect();
    assert_eq!(
        kinds,
        vec![
            TokenKind::Newline,
            TokenKind::Word("mkdir"),
            TokenKind::Word("-p"),
            TokenKind::Word("./build"),
            TokenKind::Newline,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn test_lexer_redirections() {
    let script = "cmd > out.txt 2> err.txt >> append.log < input.txt &> all.log &>> all_app.log";
    let tokens = Lexer::new(script)
        .tokenize_all()
        .expect("tokenization failed");

    let kinds: Vec<TokenKind> = tokens.into_iter().map(|t| t.kind).collect();
    assert_eq!(
        kinds,
        vec![
            TokenKind::Word("cmd"),
            TokenKind::RedirectOut,
            TokenKind::Word("out.txt"),
            TokenKind::RedirectStderr,
            TokenKind::Word("err.txt"),
            TokenKind::RedirectAppend,
            TokenKind::Word("append.log"),
            TokenKind::RedirectIn,
            TokenKind::Word("input.txt"),
            TokenKind::RedirectAll,
            TokenKind::Word("all.log"),
            TokenKind::RedirectAllAppend,
            TokenKind::Word("all_app.log"),
            TokenKind::Eof,
        ]
    );
}

#[test]
fn test_lossless_roundtrip() {
    let src = "mkdir -p ./dist # create build dir\necho \"done\"\n";
    let mut lexer = Lexer::new_lossless(src);
    let mut reconstructed = String::new();
    while let Ok(tok) = lexer.next_token() {
        if tok.kind == TokenKind::Eof {
            break;
        }
        reconstructed.push_str(tok.slice(src));
    }
    assert_eq!(src, reconstructed);
}

#[test]
fn test_lossless_tokens() {
    let src = "# comment\n  cmd arg # inline\n";
    let mut lexer = Lexer::new_lossless(src);
    let mut tokens = Vec::new();
    while let Ok(tok) = lexer.next_token() {
        let is_eof = tok.kind == TokenKind::Eof;
        tokens.push(tok);
        if is_eof {
            break;
        }
    }

    assert_eq!(tokens[0].kind, TokenKind::Comment("# comment"));
    assert_eq!(tokens[1].kind, TokenKind::Newline);
    assert_eq!(tokens[2].kind, TokenKind::Whitespace("  "));
    assert_eq!(tokens[3].kind, TokenKind::Word("cmd"));
    assert_eq!(tokens[4].kind, TokenKind::Whitespace(" "));
    assert_eq!(tokens[5].kind, TokenKind::Word("arg"));
    assert_eq!(tokens[6].kind, TokenKind::Whitespace(" "));
    assert_eq!(tokens[7].kind, TokenKind::Comment("# inline"));
    assert_eq!(tokens[8].kind, TokenKind::Newline);
    assert_eq!(tokens[9].kind, TokenKind::Eof);
}
