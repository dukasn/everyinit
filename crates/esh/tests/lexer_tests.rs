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
fn unterminated_here_document_is_a_lexical_error() {
    let error = Lexer::new("cat <<EOF\npartial body\n")
        .tokenize_all()
        .expect_err("a missing here-document delimiter must be rejected");

    assert!(error.to_string().contains("Unterminated here-document"));
    assert!(error.to_string().contains("expected 'EOF'"));
}

#[test]
fn here_document_delimiter_at_eof_without_line_ending_is_accepted() {
    let tokens = Lexer::new("cat <<EOF\nbody\nEOF")
        .tokenize_all()
        .expect("ESH accepts a closing delimiter at EOF without a line ending");

    assert!(tokens.iter().any(|token| matches!(
        token.kind,
        TokenKind::HereDocDelimiter {
            body: "body\n",
            quoted: false,
            strip_tabs: false,
        }
    )));
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
fn hash_inside_word_is_literal_but_word_initial_hash_starts_comment() {
    let tokens = Lexer::new("echo value#suffix # trailing comment\n")
        .tokenize_all()
        .expect("tokenization failed");

    let kinds: Vec<TokenKind> = tokens.into_iter().map(|token| token.kind).collect();
    assert_eq!(
        kinds,
        vec![
            TokenKind::Word("echo"),
            TokenKind::Word("value#suffix"),
            TokenKind::Newline,
            TokenKind::Eof,
        ]
    );
}

#[test]
fn test_lexer_rejects_unstructured_background_operator() {
    let error = Lexer::new("sleep 1 &")
        .tokenize_all()
        .expect_err("standalone '&' is outside the ESH command language");

    assert!(error.message.contains("Unexpected single '&' operator"));
}

#[test]
fn test_lexer_rejects_unsupported_descriptor_redirections() {
    for script in [
        "echo value 3>output",
        "echo value 10>>output",
        "echo value 2>&-",
        "echo value 1>&-",
        "echo value 0>output",
        "cat <> file",
    ] {
        let error = Lexer::new(script)
            .tokenize_all()
            .expect_err("unsupported descriptor redirection should fail lexing");
        assert!(error.message.contains("redirection"), "{error}");
    }
}

#[test]
fn test_lexer_redirections() {
    let script = "cmd > out.txt 2> err.txt >&2 2>&1 1>&2 >> append.log < input.txt 0< stdin.txt 1> stdout2.txt 1>> append2.log 1>| force.txt &> all.log &>> all_app.log";
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
            TokenKind::RedirectStdoutToStderr,
            TokenKind::RedirectStderrToStdout,
            TokenKind::RedirectStdoutToStderr,
            TokenKind::RedirectAppend,
            TokenKind::Word("append.log"),
            TokenKind::RedirectIn,
            TokenKind::Word("input.txt"),
            TokenKind::RedirectIn,
            TokenKind::Word("stdin.txt"),
            TokenKind::RedirectOut,
            TokenKind::Word("stdout2.txt"),
            TokenKind::RedirectAppend,
            TokenKind::Word("append2.log"),
            TokenKind::RedirectClobber,
            TokenKind::Word("force.txt"),
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
