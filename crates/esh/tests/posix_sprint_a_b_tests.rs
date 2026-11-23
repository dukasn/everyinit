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

use esh::runtime::traits::ScopeManager;
use esh::{Engine, ExecutionContext};

#[tokio::test]
async fn test_pipeline_negation() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        ! [ -f "/non/existent/file.txt" ] && echo "negation passed"
        ! false && echo "false was negated"
    "#;

    let code = engine.eval_str(script, &mut ctx).await.unwrap();
    assert_eq!(code, 0);
    let output = ctx.get_captured_stdout().unwrap();
    assert!(output.contains("negation passed"));
    assert!(output.contains("false was negated"));
}

#[tokio::test]
async fn test_local_scoping_and_functions() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        GLOBAL_VAR="original"
        
        my_func() {
            local LOCAL_VAR="secret"
            GLOBAL_VAR="mutated"
            echo "Inside func: local=$LOCAL_VAR, global=$GLOBAL_VAR, arg1=$1"
        }
        
        my_func "first_arg"
        echo "Outside func: local=$LOCAL_VAR, global=$GLOBAL_VAR"
    "#;

    let code = engine.eval_str(script, &mut ctx).await.unwrap();
    assert_eq!(code, 0);
    let output = ctx.get_captured_stdout().unwrap();
    assert!(output.contains("Inside func: local=secret, global=mutated, arg1=first_arg"));
    assert!(output.contains("Outside func: local=, global=mutated"));
}

#[tokio::test]
async fn nested_functions_resolve_and_update_the_nearest_dynamic_local() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        VALUE=root
        inner() {
            echo "inner-before=$VALUE"
            VALUE=changed
        }
        outer() {
            local VALUE=caller-local
            inner
            echo "outer-after=$VALUE"
        }
        outer
        echo "root-after=$VALUE"
    "#;

    let status = engine.eval_str(script, &mut ctx).await.unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "inner-before=caller-local\nouter-after=changed\nroot-after=root\n"
    );

    #[cfg(unix)]
    {
        let reference = std::process::Command::new("bash")
            .args(["--posix", "-c"])
            .arg(script)
            .output()
            .expect("Bash should be available for the dynamic-local comparison");
        assert!(reference.status.success());
        assert_eq!(
            reference.stdout,
            b"inner-before=caller-local\nouter-after=changed\nroot-after=root\n"
        );
    }
}

#[tokio::test]
async fn recursive_local_bindings_restore_each_dynamic_frame() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        VALUE=root
        walk() {
            local VALUE=$1
            if [ "$1" -gt 0 ]; then
                walk "$(( $1 - 1 ))" nested
            fi
            printf 'frame=%s arg1=%s arg2=%s count=%s\n' "$VALUE" "$1" "$2" "$#"
        }
        walk 3 original
        printf 'root=%s\n' "$VALUE"
    "#;
    let status = engine.eval_str(script, &mut ctx).await.unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "frame=0 arg1=0 arg2=nested count=2\nframe=1 arg1=1 arg2=nested count=2\nframe=2 arg1=2 arg2=nested count=2\nframe=3 arg1=3 arg2=original count=2\nroot=root\n"
    );

    #[cfg(unix)]
    {
        let reference = std::process::Command::new("bash")
            .args(["--posix", "-c"])
            .arg(script)
            .output()
            .expect("Bash should be available for the recursive-local comparison");
        assert!(reference.status.success());
        assert_eq!(
            reference.stdout,
            b"frame=0 arg1=0 arg2=nested count=2\nframe=1 arg1=1 arg2=nested count=2\nframe=2 arg1=2 arg2=nested count=2\nframe=3 arg1=3 arg2=original count=2\nroot=root\n"
        );
    }
}

#[tokio::test]
async fn function_keyword_extension_and_compound_body_scope_match_the_contract() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                VALUE=root
                function keyword_body { VALUE=keyword; }
                keyword_body
                printf 'keyword=%s\n' "$VALUE"

                function parenthesized_body() { VALUE=parenthesized; }
                parenthesized_body
                printf 'parenthesized=%s\n' "$VALUE"

                brace_body() { VALUE=brace; }
                brace_body
                printf 'brace=%s\n' "$VALUE"

                subshell_body() ( VALUE=subshell; printf 'inside=%s\n' "$VALUE" )
                subshell_body
                printf 'after-subshell=%s\n' "$VALUE"

                conditional_body() if true; then VALUE=conditional; fi
                conditional_body
                printf 'conditional=%s\n' "$VALUE"

                case_body() case selected in selected) VALUE=case;; esac
                case_body
                printf 'case=%s\n' "$VALUE"

                loop_body() for ITEM in first last; do VALUE=$ITEM; done
                loop_body
                printf 'loop=%s\n' "$VALUE"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "keyword=keyword\nparenthesized=parenthesized\nbrace=brace\ninside=subshell\nafter-subshell=brace\nconditional=conditional\ncase=case\nloop=last\n"
    );
}

#[tokio::test]
async fn function_keyword_extension_accepts_reserved_word_identifiers() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let keywords = [
        "if", "then", "elif", "else", "fi", "for", "in", "while", "until", "do", "done", "case",
        "esac",
    ];
    let mut script = String::new();
    let mut expected = String::new();
    for keyword in keywords {
        script.push_str(&format!(
            "function {keyword} {{ printf '{keyword}\\n'; }}\n\\{keyword}\n"
        ));
        expected.push_str(&format!("{keyword}\n"));
    }

    let status = engine.eval_str(&script, &mut ctx).await.unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), expected);

    #[cfg(unix)]
    if let Ok(reference) = std::process::Command::new("bash")
        .args(["--posix", "-c"])
        .arg(&script)
        .output()
    {
        assert!(reference.status.success());
        assert_eq!(reference.stdout, expected.as_bytes());
    }
}

#[tokio::test]
async fn test_advanced_parameter_expansions() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        FILE="/var/log/app/service.prod.log"
        BASENAME="${FILE##*/}"
        DIRNAME="${FILE%/*}"
        NO_EXT="${BASENAME%.*}"
        
        COMMIT="a1b2c3d4e5f6g7h8"
        SHORT_COMMIT="${COMMIT:0:7}"
        
        TEXT="apple-banana-apple"
        REPLACED_FIRST="${TEXT/apple/orange}"
        REPLACED_ALL="${TEXT//apple/orange}"
        
        echo "basename=$BASENAME"
        echo "dirname=$DIRNAME"
        echo "no_ext=$NO_EXT"
        echo "short_commit=$SHORT_COMMIT"
        echo "replaced_first=$REPLACED_FIRST"
        echo "replaced_all=$REPLACED_ALL"
    "#;

    let code = engine.eval_str(script, &mut ctx).await.unwrap();
    assert_eq!(code, 0);
    let output = ctx.get_captured_stdout().unwrap();
    assert!(output.contains("basename=service.prod.log"));
    assert!(output.contains("dirname=/var/log/app"));
    assert!(output.contains("no_ext=service.prod"));
    assert!(output.contains("short_commit=a1b2c3d"));
    assert!(output.contains("replaced_first=orange-banana-apple"));
    assert!(output.contains("replaced_all=orange-banana-orange"));
}

#[tokio::test]
async fn test_read_builtin_and_herestrings() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        read -r FIRST SECOND REST <<< "alpha beta gamma"
        echo "FIRST=$FIRST, SECOND=$SECOND, REST=$REST"
        
        read -r SINGLE <<< "single_line_payload"
        echo "SINGLE=$SINGLE"

        read -r <<< "standalone_line"
        echo "REPLY=$REPLY"
    "#;

    let code = engine.eval_str(script, &mut ctx).await.unwrap();
    assert_eq!(code, 0);
    let output = ctx.get_captured_stdout().unwrap();
    assert!(output.contains("FIRST=alpha, SECOND=beta, REST=gamma"));
    assert!(output.contains("SINGLE=single_line_payload"));
    assert!(output.contains("REPLY=standalone_line"));
}

#[tokio::test]
async fn scalar_expansion_preserves_one_argument_without_splitting_or_globbing() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("alpha.txt"), "").unwrap();
    std::fs::write(tmp.path().join("beta.txt"), "").unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    let script = r#"
        capture_args() {
            echo "$#:<$1>"
        }
        VALUE="two words *.txt"
        capture_args $VALUE
        capture_args $(printf 'two words *.txt')
        capture_args $(printf 'two words *.txt\n\n')
        capture_args $(printf '')
        ARGS=("two words" "*.txt")
        capture_args "${ARGS[@]}"
        for FILE in $(glob "*.txt"); do echo "glob=<$FILE>"; done
    "#;

    let status = engine.eval_str(script, &mut ctx).await.unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "1:<two words *.txt>\n1:<two words *.txt>\n1:<two words *.txt>\n1:<>\n2:<two words>\nglob=<alpha.txt>\nglob=<beta.txt>\n"
    );
}

#[tokio::test]
async fn tilde_expansion_respects_user_prefixes_quoting_and_unset_home() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                HOME='/esh home'
                printf '<%s>|<%s>|<%s>|<%s>|<%s>|<%s>|<%s>\n' ~ ~/release "~" \~/literal prefix~/literal ~esh_audit_missing_user ~esh_audit_missing_user/path
                printf 'other-prefixes=<%s>|<%s>\n' ~+ ~-
                unset HOME
                printf 'unset=<%s>|<%s>\n' ~ ~/release
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "</esh home>|</esh home/release>|<~>|<~/literal>|<prefix~/literal>|<~esh_audit_missing_user>|<~esh_audit_missing_user/path>\nother-prefixes=<~+>|<~->\nunset=<~>|<~/release>\n"
    );
}

#[tokio::test]
async fn command_substitution_preserves_nul_bytes_in_shell_values() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"VALUE=$(printf 'left\000right\n\n')
LEGACY=`printf 'back\000tick\n\n'`
printf '<%s>|<%s>\n' "$VALUE" "$LEGACY"
"#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout_bytes().unwrap(),
        b"<left\0right>|<back\0tick>\n"
    );
}

#[tokio::test]
async fn backslash_quoting_preserves_escaped_characters_and_removes_line_continuations() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = "echo one\\ two; echo literal\\*; echo \"double\\\\q\"; echo left\\\nright";
    let status = engine.eval_str(script, &mut ctx).await.unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "one two\nliteral*\ndouble\\q\nleftright\n"
    );
}

#[tokio::test]
async fn quote_contexts_preserve_literals_and_join_adjacent_word_segments() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                VALUE='literal $HOME * `tick` \\ end'
                capture() { printf 'argc=%s:<%s>\n' "$#" "$1"; }
                capture '$HOME * `tick` \\ end'
                capture "$VALUE"
                capture pre" two "'three '"$VALUE"post
                capture ''
                capture ""
                capture pre""post
                EMPTY=
                capture "$EMPTY"
                capture pre"$EMPTY"post
                capture "dollar=\$HOME slash=\\ quote=\" tick=\` other=\q"
                capture "$(printf 'first\nsecond\n\n')"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "argc=1:<$HOME * `tick` \\\\ end>\nargc=1:<literal $HOME * `tick` \\\\ end>\nargc=1:<pre two three literal $HOME * `tick` \\\\ endpost>\nargc=1:<>\nargc=1:<>\nargc=1:<prepost>\nargc=1:<>\nargc=1:<prepost>\nargc=1:<dollar=$HOME slash=\\ quote=\" tick=` other=\\q>\nargc=1:<first\nsecond>\n"
    );
}

#[tokio::test]
async fn positional_at_expands_to_arguments_but_iterates_separately_in_for() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_positional_args(vec![
        "script.esh".to_string(),
        "first value".to_string(),
        "second".to_string(),
    ]);

    let script = r#"
        capture_args() { echo "$#:<$1>"; }
        capture_args "$@"
        capture_args "$*"
        IFS=:
        capture_args "$*"
        capture_args $*
        RESULT=""
        for ARG in "$@"; do
            RESULT="${RESULT}[${ARG}]"
        done
        echo "$RESULT"
    "#;

    let status = engine.eval_str(script, &mut ctx).await.unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "2:<first value>\n1:<first value second>\n1:<first value second>\n1:<first value second>\n[first value][second]\n"
    );
}

#[tokio::test]
async fn embedded_positional_at_remains_one_space_joined_scalar() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_positional_args(vec![
        "script.esh".to_string(),
        "first value".to_string(),
        "second".to_string(),
    ]);

    let status = engine
        .eval_str(
            "capture_args() { echo \"$#:<$1>\"; }\ncapture_args \"pre$@post\"\ncapture_args prefix$@suffix\nIFS=:\nset -- '' ''\ncapture_args \"pre$@post\"\ncapture_args prefix$@suffix\nset -- ''\ncapture_args \"pre$@post\"\nset -- '' value ''\ncapture_args \"pre$@post\"\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "1:<prefirst value secondpost>\n1:<prefixfirst value secondsuffix>\n1:<pre post>\n1:<prefix suffix>\n1:<prepost>\n1:<pre value post>\n"
    );
}

#[tokio::test]
async fn positional_at_uses_one_scalar_in_assignments_and_redirections() {
    let temp = tempfile::tempdir().unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(temp.path()).unwrap();

    let status = engine
        .eval_str(
            "set -- 'first value' second\nIFS=:\nJOINED=\"$@\"\nprintf '<%s>\\n' \"$JOINED\"\nprintf payload >\"$@\"\ncat 'first value second'\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "<first value second>\npayload"
    );
    assert_eq!(
        std::fs::read_to_string(temp.path().join("first value second")).unwrap(),
        "payload"
    );
}

#[tokio::test]
async fn standalone_unquoted_at_expands_to_unsplit_positional_arguments() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_positional_args(vec![
        "script.esh".to_string(),
        "first value".to_string(),
        "second".to_string(),
    ]);

    let status = engine
        .eval_str(
            "capture_args() { echo \"$#:<$1>:<$2>\"; }\ncapture_args $@\ncapture_args ${@:-fallback}\nset --\ncapture_args $@\ncapture_args ${@:-fallback}\ncapture_args ${@+fallback}\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "2:<first value>:<second>\n2:<first value>:<second>\n0:<>:<>\n1:<fallback>:<>\n0:<>:<>\n"
    );
}

#[tokio::test]
async fn special_parameter_lengths_distinguish_positional_count_from_count_length() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_positional_args(
        std::iter::once("script.esh".to_string())
            .chain((1..=10).map(|index| format!("arg{index}")))
            .collect(),
    );

    assert_eq!(
        engine
            .eval_str("echo ${#@}:${#*}:${##}\n", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "10:10:2\n");
}

#[tokio::test]
async fn braced_positional_parameters_support_indices_above_nine() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    let mut positional = vec!["script.esh".to_string()];
    positional.extend((1..=10).map(|index| format!("arg{index}")));
    positional[1] = "first".to_string();
    positional[10] = "tenth".to_string();
    ctx.set_positional_args(positional);

    let status = engine
        .eval_str(
            r#"
                echo "$10:${10}:${10:-fallback}"
                inspect_tenth() { echo "${10}"; }
                inspect_tenth "$@"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "first0:tenth:tenth\ntenth\n"
    );
}

#[tokio::test]
async fn command_prefix_assignments_are_visible_during_command_then_restored() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_env("ESH_PREFIX_TEST", "original");

    let script = r#"
        ESH_PREFIX_TEST=builtin echo "$ESH_PREFIX_TEST"
        echo "$ESH_PREFIX_TEST"
        ESH_PREFIX_TEST=external printenv ESH_PREFIX_TEST
        echo "$ESH_PREFIX_TEST"
        ESH_ASSIGN_ONLY=persist
        echo "$ESH_ASSIGN_ONLY"
    "#;

    let status = engine.eval_str(script, &mut ctx).await.unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "builtin\noriginal\nexternal\noriginal\npersist\n"
    );
    assert_eq!(ctx.get_var("ESH_PREFIX_TEST").as_deref(), Some("original"));
    assert_eq!(ctx.get_var("ESH_ASSIGN_ONLY").as_deref(), Some("persist"));
}

#[tokio::test]
async fn prefix_assignments_are_restored_after_loop_and_function_control_flow() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                TEMP=original
                for ITEM in one two; do
                    TEMP=break break
                    echo unreachable-break
                done
                for ITEM in one two; do
                    TEMP=continue continue
                    echo unreachable-continue
                done
                leave() {
                    TEMP=return return 7
                    echo unreachable-return
                }
                leave
                printf 'return=%s value=%s\n' "$?" "$TEMP"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "return=7 value=original\n"
    );
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}

#[tokio::test]
async fn prefix_assignment_scope_cleans_up_after_functions_and_expansion_errors() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_env("ESH_PREFIX_SCOPE", "original");

    let script = r#"
        mutate() {
            ESH_PREFIX_SCOPE=inside
            echo "$ESH_PREFIX_SCOPE"
        }
        ESH_PREFIX_SCOPE=call mutate
        echo "$ESH_PREFIX_SCOPE"
        set -u
        ESH_PREFIX_SCOPE=temporary echo "$ESH_MISSING"
    "#;

    let result = engine.eval_str(script, &mut ctx).await;

    assert!(result.is_err());
    assert_eq!(ctx.get_var("ESH_PREFIX_SCOPE").as_deref(), Some("original"));
    assert_eq!(
        ctx.env_vars().get("ESH_PREFIX_SCOPE").map(String::as_str),
        Some("original")
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "inside\noriginal\n");
}

#[tokio::test]
async fn prefix_assignments_restore_only_their_environment_bindings_after_export() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_env("ESH_PREFIX_EXPORT", "original");

    let status = engine
        .eval_str(
            "ESH_PREFIX_EXPORT=temporary export ESH_PREFIX_EXPORT ESH_NEW_EXPORT=kept\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_var("ESH_PREFIX_EXPORT").as_deref(),
        Some("original")
    );
    assert_eq!(
        ctx.env_vars().get("ESH_PREFIX_EXPORT").map(String::as_str),
        Some("original")
    );
    assert_eq!(ctx.get_var("ESH_NEW_EXPORT").as_deref(), Some("kept"));
    assert_eq!(
        ctx.env_vars().get("ESH_NEW_EXPORT").map(String::as_str),
        Some("kept")
    );

    let status = engine
        .eval_str(
            "ESH_PREFIX_EXPORT=temporary export ESH_REDIRECT_EXPORT=kept > /dev/null\n",
            &mut ctx,
        )
        .await
        .unwrap();
    assert_eq!(status, 0);
    assert_eq!(ctx.get_var("ESH_REDIRECT_EXPORT").as_deref(), Some("kept"));
    assert_eq!(
        ctx.env_vars()
            .get("ESH_REDIRECT_EXPORT")
            .map(String::as_str),
        Some("kept")
    );

    let status = engine
        .eval_str(
            "ESH_TEMP_EXPORT=temporary export ESH_TEMP_EXPORT\n",
            &mut ctx,
        )
        .await
        .unwrap();
    assert_eq!(status, 0);
    assert_eq!(ctx.get_var("ESH_TEMP_EXPORT"), None);
    assert!(!ctx.env_vars().contains_key("ESH_TEMP_EXPORT"));
}

#[tokio::test]
async fn later_prefix_assignments_expand_with_earlier_prefix_values() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_env("ESH_PREFIX_ORDER", "original");

    let status = engine
        .eval_str(
            r#"ESH_PREFIX_ORDER=temporary ESH_PREFIX_COPY=$ESH_PREFIX_ORDER printf '<%s><%s>\n' "$ESH_PREFIX_ORDER" "$ESH_PREFIX_COPY"
printf 'after=<%s><%s>\n' "$ESH_PREFIX_ORDER" "${ESH_PREFIX_COPY-unset}"
"#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "<temporary><temporary>\nafter=<original><unset>\n"
    );
    assert_eq!(ctx.get_var("ESH_PREFIX_ORDER").as_deref(), Some("original"));
    assert_eq!(ctx.get_var("ESH_PREFIX_COPY"), None);
}

#[tokio::test]
async fn array_prefix_assignments_are_scoped_across_builtins_and_redirections() {
    let tmp = tempfile::tempdir().unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();
    ctx.set_env("ESH_PREFIX_EXPORTED_ARRAY", "environment value");
    std::fs::write(tmp.path().join("input.txt"), "input value\n").unwrap();

    let script = r#"
        ESH_PREFIX_ARRAY=(original values)
        ESH_PREFIX_ARRAY=(temporary command) echo "${ESH_PREFIX_ARRAY[@]}" > output.txt
        ESH_PREFIX_EXPORTED_ARRAY=(temporary array) echo "${ESH_PREFIX_EXPORTED_ARRAY[@]}"
        printenv ESH_PREFIX_EXPORTED_ARRAY
        ESH_PREFIX_PATH=from-prefix.txt echo output > "$ESH_PREFIX_PATH"
        ESH_INPUT_PATH=input.txt cat < "$ESH_INPUT_PATH" > input-result.txt
        ESH_ASSIGN_REDIRECT=kept > assignment-only.txt
        echo "${ESH_PREFIX_ARRAY[@]}"
    "#;
    let status = engine.eval_str(script, &mut ctx).await.unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "temporary array\nenvironment value\noriginal values\n"
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("output.txt")).unwrap(),
        "temporary command\n"
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("from-prefix.txt")).unwrap(),
        "output\n"
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("input-result.txt")).unwrap(),
        "input value\n"
    );
    assert_eq!(ctx.get_var("ESH_ASSIGN_REDIRECT").as_deref(), Some("kept"));
    assert_eq!(
        std::fs::read(tmp.path().join("assignment-only.txt")).unwrap(),
        b""
    );
    assert_eq!(
        ctx.get_shell_val("ESH_PREFIX_ARRAY"),
        Some(&esh::runtime::context::ShellValue::Array(
            [(0, "original".to_string()), (1, "values".to_string()),].into(),
        ))
    );
    assert!(!ctx.env_vars().contains_key("ESH_PREFIX_ARRAY"));
}

#[tokio::test]
async fn subshell_state_is_isolated_and_pipeline_adopts_last_stage_state() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        VALUE=outer
        ( echo child; VALUE=subshell )
        echo "$VALUE"
        VALUE=first | cat
        echo "$VALUE"
        echo pipeline | VALUE=last
        echo "$VALUE"
    "#;

    let status = engine.eval_str(script, &mut ctx).await.unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "child\nouter\nouter\nlast\n"
    );
}
