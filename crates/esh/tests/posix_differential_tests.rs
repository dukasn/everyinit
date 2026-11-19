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

#[cfg(unix)]
use esh::{Engine, ExecutionContext};
#[cfg(unix)]
use std::process::Command;
#[cfg(unix)]
use tempfile::tempdir;

#[cfg(unix)]
struct Case {
    name: &'static str,
    script: &'static str,
    expected_status: i32,
    expected_stdout: &'static str,
    expected_file: Option<&'static str>,
}

#[cfg(unix)]
#[tokio::test]
async fn adopted_shell_subset_matches_available_posix_shells() {
    let cases = [
        Case {
            name: "posix_echo_appends_newline_after_operand_newline",
            script: "echo 'line\n'\n",
            expected_status: 0,
            expected_stdout: "line\n\n",
            expected_file: None,
        },
        Case {
            name: "posix_comment_ends_at_physical_newline_after_backslash",
            script: r"# comment \
printf 'after-comment\n'
printf 'done\n'
",
            expected_status: 0,
            expected_stdout: "after-comment\ndone\n",
            expected_file: None,
        },
        Case {
            name: "posix_read_duplicate_variable_operands_use_last_assignment",
            script: "read VALUE OTHER VALUE <<EOF\nfirst second third\nEOF\nprintf '<%s><%s>\\n' \"$VALUE\" \"$OTHER\"\nVALUE=initial\nOTHER=initial\nread VALUE OTHER VALUE <<EOF\nonly\nEOF\nprintf '<%s><%s>\\n' \"$VALUE\" \"$OTHER\"\n",
            expected_status: 0,
            expected_stdout: "<third><second>\n<><>\n",
            expected_file: None,
        },
        Case {
            name: "if_case_and_for",
            script: r#"
                VALUE=base
                if [ "$VALUE" = base ]; then VALUE=matched; fi
                for ITEM in one two; do printf '%s ' "$ITEM"; done
                case "$VALUE" in
                    match*)
                        echo result=case
                        ;;
                    *)
                        echo result=other
                        ;;
                esac
            "#,
            expected_status: 0,
            expected_stdout: "one two result=case\n",
            expected_file: None,
        },
        Case {
            name: "posix_reserved_words_are_literals_in_word_positions",
            script: "printf '<%s>\\n' if then elif else fi for in while until do done case esac\nprintf payload >if\ncat if >result.txt\ncat if\n",
            expected_status: 0,
            expected_stdout: "<if>\n<then>\n<elif>\n<else>\n<fi>\n<for>\n<in>\n<while>\n<until>\n<do>\n<done>\n<case>\n<esac>\npayload",
            expected_file: Some("payload"),
        },
        Case {
            name: "posix_if_elif_else_branch_selection",
            script: r#"VALUE=beta
if test "$VALUE" = alpha; then
    printf 'alpha\n'
elif test "$VALUE" = beta; then
    printf 'beta\n'
else
    printf 'other\n'
fi
"#,
            expected_status: 0,
            expected_stdout: "beta\n",
            expected_file: None,
        },
        Case {
            name: "posix_if_status_uses_selected_branch_or_zero_without_branch",
            script: r#"if false; then :; fi
NO_BRANCH=$?
if true; then false; fi
FAILED_BRANCH=$?
if false; then :; else true; fi
ELSE_BRANCH=$?
printf '%s:%s:%s\n' "$NO_BRANCH" "$FAILED_BRANCH" "$ELSE_BRANCH"
"#,
            expected_status: 0,
            expected_stdout: "0:1:0\n",
            expected_file: None,
        },
        Case {
            name: "posix_for_without_in_uses_script_positionals_and_handles_empty_list",
            script: r#"set -- first second
for ITEM; do printf '<%s>' "$ITEM"; done
printf '\n'
set --
for ITEM; do printf 'unexpected=<%s>\n' "$ITEM"; done
printf '<done>\n'
"#,
            expected_status: 0,
            expected_stdout: "<first><second>\n<done>\n",
            expected_file: None,
        },
        Case {
            name: "posix_for_with_empty_in_list_does_not_use_positionals",
            script: r#"set -- positional
for ITEM in; do printf 'unexpected=<%s>\n' "$ITEM"; done
printf '<empty-in-done>\n'
for ITEM; do printf '<%s>' "$ITEM"; done
printf '\n'
"#,
            expected_status: 0,
            expected_stdout: "<empty-in-done>\n<positional>\n",
            expected_file: None,
        },
        Case {
            name: "posix_for_loop_variable_retains_value_for_empty_list",
            script: "ITEM=before\nfor ITEM in; do printf 'unexpected\\n'; done\nprintf 'empty=<%s>\\n' \"$ITEM\"\nfor ITEM in first last; do :; done\nprintf 'nonempty=<%s>\\n' \"$ITEM\"\n",
            expected_status: 0,
            expected_stdout: "empty=<before>\nnonempty=<last>\n",
            expected_file: None,
        },
        Case {
            name: "posix_for_header_newline_after_name",
            script: "for ITEM\nin first second\ndo\nprintf '<%s>' \"$ITEM\"\ndone\nprintf '\\n'\n",
            expected_status: 0,
            expected_stdout: "<first><second>\n",
            expected_file: None,
        },
        Case {
            name: "posix_errexit_exempts_negated_test_condition",
            script: r#"set -e
if ! test -f absent.txt; then printf 'expected-missing\n'; fi
printf 'continued\n'
"#,
            expected_status: 0,
            expected_stdout: "expected-missing\ncontinued\n",
            expected_file: None,
        },
        Case {
            name: "posix_errexit_exempts_elif_and_until_tests",
            script: r#"set -e
if false; then printf 'wrong-if\n'; elif false; then printf 'wrong-elif\n'; else printf 'if-continued\n'; fi
until false; do printf 'until-continued\n'; break; done
printf 'after-tests\n'
"#,
            expected_status: 0,
            expected_stdout: "if-continued\nuntil-continued\nafter-tests\n",
            expected_file: None,
        },
        Case {
            name: "posix_errexit_is_suppressed_inside_tested_function_bodies",
            script: "set -e\nchecked() { printf 'checked-before\\n'; false; printf 'checked-after\\n'; }\nif checked; then printf 'if-then\\n'; fi\nchained() { printf 'chained-before\\n'; false; printf 'chained-after\\n'; }\nchained || printf 'fallback\\n'\ngated() { printf 'gated-before\\n'; false; printf 'gated-after\\n'; }\ngated && printf 'and-rhs\\n'\nprintf 'done\\n'\n",
            expected_status: 0,
            expected_stdout: "checked-before\nchecked-after\nif-then\nchained-before\nchained-after\ngated-before\ngated-after\nand-rhs\ndone\n",
            expected_file: None,
        },
        Case {
            name: "posix_errexit_is_suppressed_inside_functions_used_as_loop_tests",
            script: "set -e\nCOUNT=0\nwhile_check() { false; COUNT=$((COUNT + 1)); [ \"$COUNT\" -lt 2 ]; }\nwhile while_check; do printf 'while-body\\n'; done\nCOUNT=0\nuntil_check() { false; COUNT=$((COUNT + 1)); [ \"$COUNT\" -ge 2 ]; }\nuntil until_check; do printf 'until-body\\n'; done\nfailed_check() { printf 'failed-check\\n'; false; }\nif failed_check; then printf 'unexpected-if\\n'; else printf 'if-else\\n'; fi\nfailed_left() { printf 'failed-left\\n'; false; }\nfailed_left || printf 'fallback\\n'\nprintf 'done\\n'\n",
            expected_status: 0,
            expected_stdout: "while-body\nuntil-body\nfailed-check\nif-else\nfailed-left\nfallback\ndone\n",
            expected_file: None,
        },
        Case {
            name: "posix_errexit_is_inherited_by_assignment_command_substitution",
            script: "set -e\nVALUE=$(false; printf sub-after)\nprintf 'assignment=<%s>\\n' \"$VALUE\"\nprintf parent\n",
            expected_status: 1,
            expected_stdout: "",
            expected_file: None,
        },
        Case {
            name: "posix_errexit_stops_only_the_substitution_inside_a_successful_command",
            script: "set -e\nprintf 'argument=<%s>\\n' \"$(false; printf sub-after)\"\nprintf 'parent\\n'\n",
            expected_status: 0,
            expected_stdout: "argument=<>\nparent\n",
            expected_file: None,
        },
        Case {
            name: "posix_loop_status_distinguishes_empty_and_executed_bodies",
            script: r#"while false; do :; done
WHILE_STATUS=$?
until true; do :; done
UNTIL_STATUS=$?
for ITEM in; do :; done
EMPTY_FOR_STATUS=$?
for ITEM in one; do false; done
FOR_BODY_STATUS=$?
printf '%s:%s:%s:%s\n' "$WHILE_STATUS" "$UNTIL_STATUS" "$EMPTY_FOR_STATUS" "$FOR_BODY_STATUS"
"#,
            expected_status: 0,
            expected_stdout: "0:0:0:1\n",
            expected_file: None,
        },
        Case {
            name: "posix_break_and_continue_levels_target_outer_loop",
            script: r#"for OUTER in a b; do
    for INNER in 1 2; do
        printf 'break:%s:%s\n' "$OUTER" "$INNER"
        break 2
    done
done
for OUTER in a b; do
    for INNER in 1 2; do
        printf 'continue:%s:%s\n' "$OUTER" "$INNER"
        continue 2
    done
    printf 'unreachable-inner\n'
done
for OUTER in c d; do
    for INNER in 1 2; do
        printf 'break-large:%s:%s\n' "$OUTER" "$INNER"
        break 99
    done
done
for OUTER in e f; do
    for INNER in 1 2; do
        printf 'continue-large:%s:%s\n' "$OUTER" "$INNER"
        continue 99
    done
done
printf 'done\n'
"#,
            expected_status: 0,
            expected_stdout: "break:a:1\ncontinue:a:1\ncontinue:b:1\nbreak-large:c:1\ncontinue-large:e:1\ncontinue-large:f:1\ndone\n",
            expected_file: None,
        },
        Case {
            name: "posix_newlines_continue_after_pipeline_and_list_operators",
            script: "printf 'pipeline\\n' |\ncat |\ncat\ntrue &&\nprintf 'and-list\\n'\nfalse ||\nprintf 'or-list\\n'\n! false\nprintf 'negation-status=%s\\n' \"$?\"\n",
            expected_status: 0,
            expected_stdout: "pipeline\nand-list\nor-list\nnegation-status=0\n",
            expected_file: None,
        },
        Case {
            name: "posix_pipeline_negation_inverts_status_without_suppressing_output",
            script: r#"! false
printf 'false=%s\n' "$?"
! true
printf 'true=%s\n' "$?"
! printf 'pipeline-output\n' | cat
printf 'pipeline=%s\n' "$?"
"#,
            expected_status: 0,
            expected_stdout: "false=0\ntrue=1\npipeline-output\npipeline=1\n",
            expected_file: None,
        },
        Case {
            name: "posix_pipeline_status_comes_from_the_last_stage_by_default",
            script: r#"false | true
FIRST_FAILED_STATUS=$?
true | false
LAST_FAILED_STATUS=$?
printf '%s:%s\n' "$FIRST_FAILED_STATUS" "$LAST_FAILED_STATUS"
"#,
            expected_status: 0,
            expected_stdout: "0:1\n",
            expected_file: None,
        },
        Case {
            name: "posix_pipeline_negation_with_compound_final_stage",
            script: r#"! printf 'compound-success\n' | { cat; } >result.txt
SUCCESS_STATUS=$?
printf 'success=%s:<%s>\n' "$SUCCESS_STATUS" "$(cat result.txt)"
! printf 'compound-failure\n' | { cat; false; } >result.txt
FAILURE_STATUS=$?
printf 'failure=%s:<%s>\n' "$FAILURE_STATUS" "$(cat result.txt)"
"#,
            expected_status: 0,
            expected_stdout: "success=1:<compound-success>\nfailure=0:<compound-failure>\n",
            expected_file: Some("compound-failure\n"),
        },
        Case {
            name: "posix_case_bracket_negation_and_quoted_literal",
            script: r#"for ITEM in a1 a0 b1; do
    case "$ITEM" in
        [!a]1)
            printf '%s=not-a\n' "$ITEM"
            ;;
        a[!0])
            printf '%s=nonzero\n' "$ITEM"
            ;;
        *)
            printf '%s=other\n' "$ITEM"
            ;;
    esac
done
case '*' in
    '*')
        printf 'quoted-wildcard=literal\n'
        ;;
    *)
        printf 'quoted-wildcard=active\n'
        ;;
esac
"#,
            expected_status: 0,
            expected_stdout: "a1=nonzero\na0=other\nb1=not-a\nquoted-wildcard=literal\n",
            expected_file: None,
        },
        Case {
            name: "posix_case_expanded_patterns_preserve_quote_provenance",
            script: r#"PATTERN='*.txt'
case file.txt in
    $PATTERN)
        printf 'expanded-pattern-active\n'
        ;;
esac
case file.txt in
    "$PATTERN")
        printf 'quoted-expansion-active\n'
        ;;
    *)
        printf 'quoted-expansion-literal\n'
        ;;
esac
PREFIX='file.'
case file.txt in
    "$PREFIX"*)
        printf 'quoted-prefix-active-suffix\n'
        ;;
esac
"#,
            expected_status: 0,
            expected_stdout: "expanded-pattern-active\nquoted-expansion-literal\nquoted-prefix-active-suffix\n",
            expected_file: None,
        },
        Case {
            name: "posix_empty_expansions_remain_empty_in_patterns",
            script: r#"VALUE=abc
PATTERN=
case "$VALUE" in
    $PATTERN) printf 'unexpected-empty-case-match\n' ;;
    *) printf 'empty-case-pattern-does-not-match\n' ;;
esac
case '' in
    $PATTERN) printf 'empty-case-pattern-matches-empty-subject\n' ;;
    *) printf 'empty-case-pattern-missed-empty-subject\n' ;;
esac
printf 'empty-parameter-pattern=<%s>\n' "${VALUE#$PATTERN}"
printf 'empty-substitution-pattern=<%s>\n' "${VALUE#$(printf '')}"
"#,
            expected_status: 0,
            expected_stdout: "empty-case-pattern-does-not-match\nempty-case-pattern-matches-empty-subject\nempty-parameter-pattern=<abc>\nempty-substitution-pattern=<abc>\n",
            expected_file: None,
        },
        Case {
            name: "posix_pattern_expansion_preserves_spaces_without_field_splitting",
            script: r#"IFS=' '
VALUE='a b-tail'
PATTERN='a b*'
case "$VALUE" in
    $PATTERN) printf 'case-pattern-kept-space-and-active-star\n' ;;
    *) printf 'case-pattern-split-or-literal\n' ;;
esac
printf 'removed=<%s>\n' "${VALUE#$PATTERN}"
"#,
            expected_status: 0,
            expected_stdout: "case-pattern-kept-space-and-active-star\nremoved=<-tail>\n",
            expected_file: None,
        },
        Case {
            name: "posix_case_pattern_alternatives_and_no_match_status",
            script: r#"for ITEM in archive.tar.gz archive.tgz notes.txt; do
    case "$ITEM" in
        *.tar.gz|*.gz)
            printf '%s=archive\n' "$ITEM"
            ;;
        *.tgz)
            printf '%s=compressed\n' "$ITEM"
            ;;
        *)
            printf '%s=other\n' "$ITEM"
            ;;
    esac
done
case plain in
    x)
        false
        ;;
esac
printf 'no-match-status=%s\n' "$?"
"#,
            expected_status: 0,
            expected_stdout: "archive.tar.gz=archive\narchive.tgz=compressed\nnotes.txt=other\nno-match-status=0\n",
            expected_file: None,
        },
        Case {
            name: "posix_case_status_is_last_selected_arm_command",
            script: "case selected in\n  selected)\n    false\n    ;;\nesac\nSELECTED_STATUS=$?\ncase selected in\n  other)\n    true\n    ;;\nesac\nNO_MATCH_STATUS=$?\nprintf 'selected=%s no-match=%s\\n' \"$SELECTED_STATUS\" \"$NO_MATCH_STATUS\"\n",
            expected_status: 0,
            expected_stdout: "selected=1 no-match=0\n",
            expected_file: None,
        },
        Case {
            name: "posix_case_accepts_empty_arm_body_and_empty_arm_list",
            script: "case value in\n  value)\n    ;;\nesac\ncase other in\nesac\nprintf 'accepted\\n'\n",
            expected_status: 0,
            expected_stdout: "accepted\n",
            expected_file: None,
        },
        Case {
            name: "posix_case_bracket_expressions_with_literal_closing_bracket_and_dash",
            script: r#"case ']' in
    []])
        printf 'literal-close-bracket\n'
        ;;
    *)
        printf 'miss-close-bracket\n'
        ;;
esac
case '-' in
    [-])
        printf 'literal-dash\n'
        ;;
    *)
        printf 'miss-dash\n'
        ;;
esac
case '[' in
    [[])
        printf 'literal-open-bracket-class\n'
        ;;
    *)
        printf 'miss-open-bracket-class\n'
        ;;
esac
VALUE='[x'
printf 'literal-open-bracket-removal=<%s>\n' "${VALUE#[[]}"
for ITEM in x ']'; do
    case "$ITEM" in
        [!]])
            printf 'negated-close-bracket:%s\n' "$ITEM"
            ;;
        *)
            printf 'other:%s\n' "$ITEM"
            ;;
    esac
done
VALUE='x-tail'
printf 'negated-close-bracket-removal=<%s>\n' "${VALUE#[!]]}"
"#,
            expected_status: 0,
            expected_stdout: "literal-close-bracket\nliteral-dash\nliteral-open-bracket-class\nliteral-open-bracket-removal=<x>\nnegated-close-bracket:x\nother:]\nnegated-close-bracket-removal=<-tail>\n",
            expected_file: None,
        },
        Case {
            name: "posix_escaped_hyphen_in_bracket_expression_is_literal",
            script: r#"case '-' in
    [a\-z]) printf 'literal-hyphen\n' ;;
    *) printf 'unexpected-match\n' ;;
esac
case m in
    [a\-z]) printf 'unexpected-range-match\n' ;;
    *) printf 'not-range\n' ;;
esac
case '!' in
    [\!a]) printf 'literal-bang\n' ;;
    *) printf 'unexpected-bang\n' ;;
esac
case b in
    [\!a]) printf 'unexpected-negation\n' ;;
    *) printf 'not-negated\n' ;;
esac
VALUE='-tail'
printf 'removed=<%s>\n' "${VALUE#[a\-z]}"
VALUE='!tail'
printf 'bang-removed=<%s>\n' "${VALUE#[\!a]}"
"#,
            expected_status: 0,
            expected_stdout: "literal-hyphen\nnot-range\nliteral-bang\nnot-negated\nremoved=<tail>\nbang-removed=<tail>\n",
            expected_file: None,
        },
        Case {
            name: "posix_patterns_match_literal_backslash",
            script: r#"case '\' in
    \\)
        printf 'case-backslash\n'
        ;;
    *)
        printf 'case-miss\n'
        ;;
esac
VALUE='tail\'
printf 'removed=<%s>\n' "${VALUE%\\}"
"#,
            expected_status: 0,
            expected_stdout: "case-backslash\nremoved=<tail>\n",
            expected_file: None,
        },
        Case {
            name: "posix_named_classes_and_negated_ranges_in_case_and_removal",
            script: r#"for ITEM in ABx a2x 42x; do
    case "$ITEM" in
        [[:alpha:]][!0-9]?)
            printf '%s:match\n' "$ITEM"
            ;;
        *)
            printf '%s:no\n' "$ITEM"
            ;;
    esac
done
VALUE=ABx-tail
printf 'removed=%s\n' "${VALUE#[[:alpha:]][!0-9]?}"
"#,
            expected_status: 0,
            expected_stdout: "ABx:match\na2x:no\n42x:no\nremoved=-tail\n",
            expected_file: None,
        },
        Case {
            name: "posix_case_and_removal_patterns_treat_slash_and_dot_as_ordinary",
            script: r#"case 'a/b' in
    */b) printf 'case-slash\n' ;;
    *) printf 'case-slash-miss\n' ;;
esac
case '.hidden' in
    .*) printf 'case-dot\n' ;;
    *) printf 'case-dot-miss\n' ;;
esac
VALUE='a/b/c'
printf 'slash=<%s>|<%s>\n' "${VALUE#*/}" "${VALUE##*/}"
VALUE='.hidden'
printf 'dot=<%s>\n' "${VALUE##.*}"
"#,
            expected_status: 0,
            expected_stdout: "case-slash\ncase-dot\nslash=<b/c>|<c>\ndot=<>\n",
            expected_file: None,
        },
        Case {
            name: "posix_case_and_removal_patterns_match_across_newlines",
            script: r#"VALUE='line1
line2-tail'
case "$VALUE" in
    line1*tail) printf 'case-star-spans-newline\n' ;;
    *) printf 'case-star-stopped-at-newline\n' ;;
esac
printf 'removed=<%s>\n' "${VALUE#*2-}"
"#,
            expected_status: 0,
            expected_stdout: "case-star-spans-newline\nremoved=<tail>\n",
            expected_file: None,
        },
        Case {
            name: "posix_case_all_ascii_named_character_classes",
            script: r#"LC_ALL=C
export LC_ALL
case A in [[:alnum:]]) printf 'alnum\n'; ;; esac
case A in [[:alpha:]]) printf 'alpha\n'; ;; esac
case ' ' in [[:blank:]]) printf 'blank-space\n'; ;; esac
case "$(printf '\t')" in [[:blank:]]) printf 'blank-tab\n'; ;; esac
case "$(printf '\001')" in [[:cntrl:]]) printf 'cntrl\n'; ;; esac
case 7 in [[:digit:]]) printf 'digit\n'; ;; esac
case '!' in [[:graph:]]) printf 'graph\n'; ;; esac
case a in [[:lower:]]) printf 'lower\n'; ;; esac
case ' ' in [[:print:]]) printf 'print\n'; ;; esac
case '!' in [[:punct:]]) printf 'punct\n'; ;; esac
case "$(printf '\t')" in [[:space:]]) printf 'space\n'; ;; esac
case Z in [[:upper:]]) printf 'upper\n'; ;; esac
case F in [[:xdigit:]]) printf 'xdigit\n'; ;; esac
"#,
            expected_status: 0,
            expected_stdout: "alnum\nalpha\nblank-space\nblank-tab\ncntrl\ndigit\ngraph\nlower\nprint\npunct\nspace\nupper\nxdigit\n",
            expected_file: None,
        },
        Case {
            name: "posix_case_unmatched_open_bracket_is_literal",
            script: r#"case '[' in
    [)
        echo literal-open-bracket
        ;;
    *)
        echo miss
        ;;
esac
"#,
            expected_status: 0,
            expected_stdout: "literal-open-bracket\n",
            expected_file: None,
        },
        Case {
            name: "posix_pattern_question_mark_matches_one_multibyte_character",
            script: r#"case 'éx' in
    ??)
        printf 'two-characters\n'
        ;;
    *)
        printf 'other\n'
        ;;
esac
VALUE='éx'
printf 'removed=<%s>\n' "${VALUE#?}"
"#,
            expected_status: 0,
            expected_stdout: "two-characters\nremoved=<x>\n",
            expected_file: None,
        },
        Case {
            name: "posix_parameter_removal_shortest_longest_and_empty_patterns",
            script: r#"VALUE=abc
printf '<%s>|<%s>|<%s>|<%s>\n' "${VALUE#*}" "${VALUE##*}" "${VALUE%*}" "${VALUE%%*}"
printf '<%s>|<%s>|<%s>|<%s>\n' "${VALUE#}" "${VALUE##}" "${VALUE%}" "${VALUE%%}"
"#,
            expected_status: 0,
            expected_stdout: "<abc>|<>|<abc>|<>\n<abc>|<abc>|<abc>|<abc>\n",
            expected_file: None,
        },
        Case {
            name: "posix_parameter_removal_escaped_and_quoted_metacharacters",
            script: r#"VALUE='pre*middle?tail'
printf '<%s>|<%s>|<%s>|<%s>\n' \
    "${VALUE#pre\*}" "${VALUE%\?tail}" \
    "${VALUE#'pre*'}" "${VALUE%"?tail"}"
"#,
            expected_status: 0,
            expected_stdout: "<middle?tail>|<pre*middle>|<middle?tail>|<pre*middle>\n",
            expected_file: None,
        },
        Case {
            name: "posix_shell_patterns_treat_slash_as_an_ordinary_character",
            script: r#"case dir/file in
    */*)
        printf 'case=slash\n'
        ;;
    *)
        printf 'case=miss\n'
        ;;
esac
VALUE=dir/file
printf 'removed=<%s>\n' "${VALUE#*/}"
case / in
    ?)
        printf 'question=slash\n'
        ;;
    *)
        printf 'question=miss\n'
        ;;
esac
"#,
            expected_status: 0,
            expected_stdout: "case=slash\nremoved=<file>\nquestion=slash\n",
            expected_file: None,
        },
        Case {
            name: "posix_test_common_operand_counts_and_predicates",
            script: r#"probe() {
    if test "$@"; then printf T; else printf F; fi
}
probe
probe ""
probe '!'
probe '!' ""
probe -n value
probe left = left
probe left != left
probe 10 -gt 2
printf '\n'
"#,
            expected_status: 0,
            expected_stdout: "FFTTTTFT\n",
            expected_file: None,
        },
        Case {
            name: "posix_test_parentheses_negation_and_boolean_precedence",
            script: r#"if test '(' x = x -o x = y ')' -a '!' x = y; then GROUP=T; else GROUP=F; fi
if test x = x -o a = a -a b = c; then PRECEDENCE=T; else PRECEDENCE=F; fi
printf '%s:%s\n' "$GROUP" "$PRECEDENCE"
"#,
            expected_status: 0,
            expected_stdout: "T:T\n",
            expected_file: None,
        },
        Case {
            name: "posix_test_dash_a_unary_and_logical_forms",
            script: r#"if test -a .; then EXISTING=T; else EXISTING=F; fi
if test left -a right; then LOGICAL=T; else LOGICAL=F; fi
printf '%s:%s\n' "$EXISTING" "$LOGICAL"
"#,
            expected_status: 0,
            expected_stdout: "T:T\n",
            expected_file: None,
        },
        Case {
            name: "posix_test_three_operand_binary_precedence_over_unary",
            script: r#"if test -n = -n; then printf 'binary=true\n'; else printf 'binary=false\n'; fi
if test -n = x; then printf 'comparison=true\n'; else printf 'comparison=false\n'; fi
if test -n -a -n; then printf 'logical=true\n'; else printf 'logical=false\n'; fi
"#,
            expected_status: 0,
            expected_stdout: "binary=true\ncomparison=false\nlogical=true\n",
            expected_file: None,
        },
        Case {
            name: "posix_printf_dynamic_width_precision_and_missing_operands",
            script: "printf '[%*.*s][%#x]\\n' -5 2 abc 16\nprintf '<%s><%d><%b>\\n'\nprintf '<%s:%s>\\n' one two three\n",
            expected_status: 0,
            expected_stdout: "[ab   ][0x10]\n<><0><>\n<one:two>\n<three:>\n",
            expected_file: None,
        },
        Case {
            name: "posix_printf_missing_dynamic_width_precision_and_values",
            script: "printf '[%*s][%.*s][%*d]\\n'\nprintf '<%*s><%.*s><%*d>\\n' 3\n",
            expected_status: 0,
            expected_stdout: "[][][0]\n<   ><><0>\n",
            expected_file: None,
        },
        Case {
            name: "posix_command_lookup_and_function_bypass",
            script: r#"printf '<%s>\n' "$(command -v printf)"
echo() { printf 'function:%s\n' "$1"; }
command echo builtin
echo ordinary
"#,
            expected_status: 0,
            expected_stdout: "<printf>\nbuiltin\nfunction:ordinary\n",
            expected_file: None,
        },
        Case {
            name: "posix_command_search_uses_unexported_path_variable",
            script: r#"mkdir bin
printf '#!/bin/sh
printf "tool-ran\n"
' >bin/path-tool
chmod +x bin/path-tool
unset PATH
PATH=bin
command -v path-tool >/dev/null
path-tool
"#,
            expected_status: 0,
            expected_stdout: "tool-ran\n",
            expected_file: None,
        },
        Case {
            name: "posix_export_print_is_sourceable",
            script: "export ESH_EXPORT_PRINT_PROBE='spaces and '\\''quotes'\nexport -p > export-state\nunset ESH_EXPORT_PRINT_PROBE\n. ./export-state\nprintf '%s\\n' \"$ESH_EXPORT_PRINT_PROBE\"\n",
            expected_status: 0,
            expected_stdout: "spaces and 'quotes\n",
            expected_file: None,
        },
        Case {
            name: "posix_export_accepts_double_dash_before_names",
            script: "ESH_EXPORT_END_OPTIONS=existing\nexport -- ESH_EXPORT_END_OPTIONS ESH_EXPORT_END_OPTIONS_NEW=assigned\nprintf '%s:%s\\n' \"$ESH_EXPORT_END_OPTIONS\" \"$ESH_EXPORT_END_OPTIONS_NEW\"\n",
            expected_status: 0,
            expected_stdout: "existing:assigned\n",
            expected_file: None,
        },
        Case {
            name: "posix_export_print_preserves_multiline_and_inert_metacharacters",
            script: r#"ESH_EXPORT_ROUNDTRIP='line one
line two $HOME `literal` $(touch should-not-exist) '\''quoted'\'' \\ tail'
export ESH_EXPORT_ROUNDTRIP
export -p > export-state
unset ESH_EXPORT_ROUNDTRIP
. ./export-state
printf '<%s>\n' "$ESH_EXPORT_ROUNDTRIP"
if test -e should-not-exist; then printf 'unsafe\n'; else printf 'safe\n'; fi
"#,
            expected_status: 0,
            expected_stdout: "<line one\nline two $HOME `literal` $(touch should-not-exist) 'quoted' \\\\ tail>\nsafe\n",
            expected_file: None,
        },
        Case {
            name: "posix_set_allexport_marks_future_assignments",
            script: "set -a\nAUTO_EXPORTED=visible\nset +a\nNOT_EXPORTED=private\nsh -c 'printf \"%s:%s\\n\" \"$AUTO_EXPORTED\" \"${NOT_EXPORTED-unset}\"'\n",
            expected_status: 0,
            expected_stdout: "visible:unset\n",
            expected_file: None,
        },
        Case {
            name: "posix_cd_dash_updates_pwd_and_oldpwd",
            script: "mkdir child\ncd child\nfirst_old=$OLDPWD\nfirst_pwd=$PWD\ncd - >/dev/null\nif test \"$PWD\" = \"$first_old\" && test \"$OLDPWD\" = \"$first_pwd\"; then printf 'previous-directory-restored\\n'; else printf 'state-mismatch\\n'; fi\n",
            expected_status: 0,
            expected_stdout: "previous-directory-restored\n",
            expected_file: None,
        },
        Case {
            name: "posix_pwd_option_delimiter_and_cluster_order",
            script: "pwd -- >/dev/null || exit 10\npwd -LP >/dev/null || exit 11\npwd -PL >/dev/null || exit 12\nprintf 'pwd-options-ok\\n'\n",
            expected_status: 0,
            expected_stdout: "pwd-options-ok\n",
            expected_file: None,
        },
        Case {
            name: "posix_pwd_logical_mode_rejects_dot_components_in_pwd",
            script: "START=$(pwd -P)\nmkdir child\ncd child\nPWD=\"$START/./child\"\nACTUAL=$(pwd -L)\nif test \"$ACTUAL\" = \"$START/child\"; then printf 'pwd-invalid-logical-fallback-ok\\n'; else printf 'pwd-invalid-logical-fallback-mismatch\\n'; fi\n",
            expected_status: 0,
            expected_stdout: "pwd-invalid-logical-fallback-ok\n",
            expected_file: None,
        },
        Case {
            name: "posix_basename_dirname_empty_and_separator_free_operands",
            script: "dirname ''\nbasename ''\ndirname foo\nbasename foo\ndirname foo/bar\nbasename foo/bar\nbasename foo foo\nbasename foo ''\n",
            expected_status: 0,
            expected_stdout: ".\n\n.\nfoo\nfoo\nbar\nfoo\nfoo\n",
            expected_file: None,
        },
        Case {
            name: "posix_basename_dirname_slash_reduction",
            script: "dirname ///\nbasename ///\ndirname //\nbasename //\ndirname /foo/bar///\nbasename /foo/bar///\ndirname -- -name/leaf\nbasename -- -name/leaf\ndirname -- -name\nbasename -- -name\n",
            expected_status: 0,
            expected_stdout: "/\n/\n/\n/\n/foo\nbar\n-name\nleaf\n.\n-name\n",
            expected_file: None,
        },
        Case {
            name: "posix_basename_multiple_operands_and_suffix_option",
            script: "basename -a /src/one.rs /src/two.rs\nbasename -s .rs /src/one.rs /src/two.rs\nbasename /src/one.rs .rs\n",
            expected_status: 0,
            expected_stdout: "one.rs\ntwo.rs\none\ntwo\none\n",
            expected_file: None,
        },
        Case {
            name: "posix_cd_double_dash_ends_options_and_selects_home_without_operand",
            script: "START=$(pwd)\nmkdir -p home ./-leading\nHOME=\"$START/home\"\ncd -- -leading\nFIRST=$PWD\ncd --\nif test \"$FIRST\" = \"$START/-leading\" && test \"$PWD\" = \"$HOME\"; then printf 'cd-double-dash-ok\\n'; else printf 'cd-double-dash-mismatch:<%s>:<%s>\\n' \"$FIRST\" \"$PWD\"; fi\n",
            expected_status: 0,
            expected_stdout: "cd-double-dash-ok\n",
            expected_file: None,
        },
        Case {
            name: "posix_cdpath_search_order_and_output",
            script: "START=$(pwd)\nmkdir -p first/shared second/shared\nCDPATH=first:second\ncd shared >\"$START/printed\"\nOUTPUT=$(cat \"$START/printed\")\nif test \"$PWD\" = \"$START/first/shared\" && test \"$OUTPUT\" = \"$PWD\"; then printf 'cdpath-ok\\n'; else printf 'cdpath-mismatch:<%s>:<%s>:<%s>\\n' \"$START\" \"$PWD\" \"$OUTPUT\"; fi\n",
            expected_status: 0,
            expected_stdout: "cdpath-ok\n",
            expected_file: None,
        },
        Case {
            name: "posix_cdpath_falls_back_and_ignores_explicit_relative_paths",
            script: "START=$(pwd)\nmkdir -p local/child search/child\nCDPATH=search\ncd local\ncd ./child\nif test \"$PWD\" = \"$START/local/child\"; then printf 'cdpath-fallback-ok\\n'; else printf 'cdpath-fallback-mismatch\\n'; fi\n",
            expected_status: 0,
            expected_stdout: "cdpath-fallback-ok\n",
            expected_file: None,
        },
        Case {
            name: "posix_cdpath_searches_a_relative_name_with_trailing_slash",
            script: "START=$(pwd)\nmkdir -p direct/name search/name\ncd direct\nCDPATH=\"$START/search\"\ncd name/ >/dev/null\nif test \"$PWD\" = \"$START/search/name\"; then printf 'cdpath-trailing-slash-ok\\n'; else printf 'cdpath-trailing-slash-mismatch\\n'; fi\n",
            expected_status: 0,
            expected_stdout: "cdpath-trailing-slash-ok\n",
            expected_file: None,
        },
        Case {
            name: "posix_cdpath_empty_component_prefers_current_directory_without_printing",
            script: "START=$(pwd)\nmkdir -p name search/name\nCDPATH=:search\ncd name\nif test \"$PWD\" = \"$START/name\"; then printf 'cdpath-empty-component-ok\\n'; else printf 'cdpath-empty-component-mismatch\\n'; fi\n",
            expected_status: 0,
            expected_stdout: "cdpath-empty-component-ok\n",
            expected_file: None,
        },
        Case {
            name: "posix_test_integer_operands_are_decimal",
            script: r#"if test 010 -eq 10; then printf T; else printf F; fi
if test 08 -eq 8; then printf T; else printf F; fi
if test +08 -eq 8; then printf T; else printf F; fi
if test -010 -eq -10; then printf T; else printf F; fi
printf '\n'
"#,
            expected_status: 0,
            expected_stdout: "TTTT\n",
            expected_file: None,
        },
        Case {
            name: "posix_test_operator_looking_operands_and_boolean_precedence",
            script: r#"probe() {
    if test "$@"; then printf T; else printf F; fi
}
probe -a
probe !
probe ! ""
probe -n -a
probe x -a ""
probe "" -o x -a ""
probe '(' x = x ')' -a '(' y != z ')'
probe '(' x = y ')' -o x = x
probe '(' '(' x = x ')' -a y = y ')'
printf '\n'
"#,
            expected_status: 0,
            expected_stdout: "TTTTFFTTT\n",
            expected_file: None,
        },
        Case {
            name: "posix_test_four_operand_negated_binary_expression",
            script: r#"if test ! left = right; then printf T; else printf F; fi
if [ ! left = left ]; then printf T; else printf F; fi
printf '\n'
"#,
            expected_status: 0,
            expected_stdout: "TF\n",
            expected_file: None,
        },
        Case {
            name: "posix_getopts_consumes_dash_prefixed_argument_and_stops_at_double_dash",
            script: r#"set -- -a -b -value -- tail
OPTIND=1
while getopts ab: option; do
    printf '%s:<%s>:%s;' "$option" "${OPTARG-unset}" "$OPTIND"
done
printf 'end:%s:%s\n' "$OPTIND" "${OPTARG-unset}"
"#,
            expected_status: 0,
            expected_stdout: "a:<unset>:2;b:<-value>:4;end:5:unset\n",
            expected_file: None,
        },
        Case {
            name: "posix_getopts_option_argument_attached_to_cluster_or_separate",
            script: r#"set -- -abVALUE tail
OPTIND=1
while getopts ab: option; do printf '%s:<%s>:%s;' "$option" "${OPTARG-unset}" "$OPTIND"; done
printf 'end:%s\n' "$OPTIND"
set -- -ab VALUE tail
OPTIND=1
while getopts ab: option; do printf '%s:<%s>:%s;' "$option" "${OPTARG-unset}" "$OPTIND"; done
printf 'end:%s\n' "$OPTIND"
"#,
            expected_status: 0,
            expected_stdout: "a:<unset>:1;b:<VALUE>:2;end:2\na:<unset>:1;b:<VALUE>:3;end:3\n",
            expected_file: None,
        },
        Case {
            name: "posix_getopts_empty_option_argument_differs_from_missing",
            script: r#"set -- -b ''
OPTIND=1
getopts :b: option
printf 'present-option=<%s> optarg=<%s> index=%s set=<%s>\n' "$option" "$OPTARG" "$OPTIND" "${OPTARG-unset}"
set -- -b
OPTIND=1
getopts :b: option
printf 'missing-option=<%s> optarg=<%s> index=%s\n' "$option" "$OPTARG" "$OPTIND"
"#,
            expected_status: 0,
            expected_stdout: "present-option=<b> optarg=<> index=3 set=<>\nmissing-option=<:> optarg=<b> index=2\n",
            expected_file: None,
        },
        Case {
            name: "posix_getopts_uses_function_positional_parameters",
            script: r#"set -- -a parent
option=stale
OPTARG=stale
parse() {
    getopts ab: option
    printf 'function:%s:%s:%s\n' "$option" "$OPTIND" "$OPTARG"
}
OPTIND=1
parse -b function-value
printf 'caller:%s:%s:%s:%s:%s\n' "$OPTIND" "$1" "$2" "$option" "$OPTARG"
OPTIND=1
while getopts ab: option; do
    printf 'outer:%s:%s:%s\n' "$option" "$OPTIND" "${OPTARG-unset}"
done
"#,
            expected_status: 0,
            expected_stdout: "function:b:3:function-value\ncaller:3:-a:parent:b:function-value\nouter:a:2:unset\n",
            expected_file: None,
        },
        Case {
            name: "posix_test_file_identity_predicate",
            script: "printf payload >result.txt\nln result.txt hardlink.txt\nln -s result.txt symlink.txt\nprintf other >other.txt\nln -s missing.txt dangling.txt\nfor path in hardlink.txt symlink.txt other.txt missing.txt dangling.txt; do\n    if test result.txt -ef \"$path\"; then printf '%s=same\\n' \"$path\"; else printf '%s=different\\n' \"$path\"; fi\ndone\n",
            expected_status: 0,
            expected_stdout: "hardlink.txt=same\nsymlink.txt=same\nother.txt=different\nmissing.txt=different\ndangling.txt=different\n",
            expected_file: Some("payload"),
        },
        Case {
            name: "posix_test_common_file_type_and_size_predicates",
            script: "mkdir directory\nchmod 700 directory\n: >empty.txt\nprintf x >result.txt\n: >executable.txt\nchmod 700 executable.txt\nln -s result.txt link.txt\nfor path in directory empty.txt result.txt executable.txt link.txt missing.txt; do\n    printf '%s:' \"$path\"\n    for predicate in e f d s x; do\n        if test -\"$predicate\" \"$path\"; then printf '%s' \"$predicate\"; fi\n    done\n    printf '\\n'\ndone\n",
            expected_status: 0,
            expected_stdout: "directory:edsx\nempty.txt:ef\nresult.txt:efs\nexecutable.txt:efx\nlink.txt:efs\nmissing.txt:\n",
            expected_file: Some("x"),
        },
        Case {
            name: "posix_test_special_file_and_set_id_predicates",
            script: "set -e\nmkfifo fifo\n: >plain\n: >setgid\nchmod g+s setgid\n: >setuid\nchmod u+s setuid\nfor path in fifo plain setgid setuid /dev/null; do\n    printf '%s:' \"$path\"\n    for predicate in b c g p u; do\n        if test -\"$predicate\" \"$path\"; then printf '%s' \"$predicate\"; fi\n    done\n    printf '\\n'\ndone\n",
            expected_status: 0,
            expected_stdout: "fifo:p\nplain:\nsetgid:g\nsetuid:u\n/dev/null:c\n",
            expected_file: None,
        },
        Case {
            name: "posix_test_file_time_predicates_with_missing_operands",
            script: r#"printf data >result.txt
if test missing -nt result.txt; then printf T; else printf F; fi
if test result.txt -nt missing; then printf T; else printf F; fi
if test missing -ot result.txt; then printf T; else printf F; fi
if test result.txt -ot missing; then printf T; else printf F; fi
if test missing -nt missing; then printf T; else printf F; fi
if test missing -ot missing; then printf T; else printf F; fi
if test result.txt -nt result.txt; then printf T; else printf F; fi
if test result.txt -ot result.txt; then printf T; else printf F; fi
printf '\n'
"#,
            expected_status: 0,
            expected_stdout: "FTTFFFFF\n",
            expected_file: Some("data"),
        },
        Case {
            name: "posix_test_dangling_symbolic_link_predicates",
            script: "printf payload >result.txt\nln -s missing.txt link.txt\nif test -L link.txt; then echo link; fi\nif test -h link.txt; then echo alias; fi\nif test -e link.txt; then echo target-exists; else echo dangling; fi\n",
            expected_status: 0,
            expected_stdout: "link\nalias\ndangling\n",
            expected_file: Some("payload"),
        },
        Case {
            name: "posix_multiple_here_documents_are_consumed_in_source_order",
            script: "cat <<FIRST <<SECOND\nfirst body\nFIRST\nsecond body\nSECOND\n",
            expected_status: 0,
            expected_stdout: "second body\n",
            expected_file: None,
        },
        Case {
            name: "posix_heredoc_expands_positional_lists_without_removing_body_quotes",
            script: "IFS=:\nset -- 'a b' c\ncat <<EOF\n<$@>|<\"$@\">|<$*>|<\"$*\">\nEOF\n",
            expected_status: 0,
            expected_stdout: "<a b c>|<\"a b c\">|<a b c>|<\"a b c\">\n",
            expected_file: None,
        },
        Case {
            name: "posix_heredoc_empty_positional_lists_preserve_only_literal_quotes",
            script: "set --\ncat <<EOF\nx$@y\nx\"$@\"y\nEOF\n",
            expected_status: 0,
            expected_stdout: "xy\nx\"\"y\n",
            expected_file: None,
        },
        Case {
            name: "posix_empty_quoted_heredoc_delimiter",
            script: "VALUE=expanded\ncat <<''\n$VALUE\n\nprintf 'after\\n'\n",
            expected_status: 0,
            expected_stdout: "$VALUE\nafter\n",
            expected_file: None,
        },
        Case {
            name: "posix_heredoc_before_later_command_word_and_another_redirect",
            script: "read <<FIRST VALUE <<SECOND\ndiscarded body\nFIRST\nselected body\nSECOND\nprintf '<%s>\\n' \"$VALUE\"\n",
            expected_status: 0,
            expected_stdout: "<selected body>\n",
            expected_file: None,
        },
        Case {
            name: "posix_heredoc_pipeline_inside_conditional",
            script: "if cat <<EOF | tr 'a-z' 'A-Z'\ncompound body\nEOF\nthen printf 'pipeline succeeded\\n'; fi\n",
            expected_status: 0,
            expected_stdout: "COMPOUND BODY\npipeline succeeded\n",
            expected_file: None,
        },
        Case {
            name: "posix_heredoc_inside_loop_body_preserves_compound_boundary",
            script: "N=0\nwhile [ \"$N\" -lt 2 ]; do\ncat <<EOF\nitem $N\nEOF\nN=$((N + 1))\ndone\nprintf 'after-loop\\n'\n",
            expected_status: 0,
            expected_stdout: "item 0\nitem 1\nafter-loop\n",
            expected_file: None,
        },
        Case {
            name: "posix_heredoc_inside_case_arm_preserves_arm_boundary",
            script: "VALUE=expanded\ncase selected in\nselected)\ncat <<EOF\nvalue=$VALUE\nEOF\n;;\n*) printf 'wrong arm\\n' ;;\nesac\nprintf 'after-case\\n'\n",
            expected_status: 0,
            expected_stdout: "value=expanded\nafter-case\n",
            expected_file: None,
        },
        Case {
            name: "while_until_state_and_final_status",
            script: r#"N=0
while [ "$N" -lt 2 ]; do
    printf 'w%s ' "$N"
    N=$((N + 1))
done
until [ "$N" -eq 4 ]; do
    printf 'u%s ' "$N"
    N=$((N + 1))
done
printf 'n=%s\n' "$N"
"#,
            expected_status: 0,
            expected_stdout: "w0 w1 u2 u3 n=4\n",
            expected_file: None,
        },
        Case {
            name: "function_positionals_and_status",
            script: r#"
                describe() { printf '%s:%s\n' "$1" "$#"; return 7; }
                describe alpha beta
                SAVED_STATUS=$?
                printf 'status=%s\n' "$SAVED_STATUS"
                iterate() { for ITEM; do printf '<%s>' "$ITEM"; done; }
                iterate first second third
                printf '\n'
            "#,
            expected_status: 0,
            expected_stdout: "alpha:2\nstatus=7\n<first><second><third>\n",
            expected_file: None,
        },
        Case {
            name: "posix_for_without_in_preserves_empty_function_positionals",
            script: r#"iterate() {
    for ITEM; do
        printf '<%s>' "$ITEM"
    done
    printf '\n'
}
iterate '' 'two words' ''
iterate
"#,
            expected_status: 0,
            expected_stdout: "<><two words><>\n\n",
            expected_file: None,
        },
        Case {
            name: "posix_function_definition_spacing_and_linebreak",
            script: "spaced ( )\n{ printf 'function:%s\\n' \"$1\"; }\nspaced argument\n",
            expected_status: 0,
            expected_stdout: "function:argument\n",
            expected_file: None,
        },
        Case {
            name: "posix_function_body_accepts_subshell_compound_command",
            script: r#"INITIAL_ZERO=$0
RESULT=outer
run_body() (
    if [ "$0" = "$INITIAL_ZERO" ]; then printf 'zero=kept '; fi
    printf 'args=%s:%s\n' "$#" "$1"
    RESULT=inner
)
run_body argument
printf 'result=%s\n' "$RESULT"
"#,
            expected_status: 0,
            expected_stdout: "zero=kept args=1:argument\nresult=outer\n",
            expected_file: None,
        },
        Case {
            name: "posix_function_definition_redirection_runs_at_call_time",
            script: r#"VALUE=defined
capture() { printf '<%s:%s>\n' "$VALUE" "$1"; } >result.txt
VALUE=called
capture argument
printf 'contents=<%s>\n' "$(cat result.txt)"
"#,
            expected_status: 0,
            expected_stdout: "contents=<<called:argument>>\n",
            expected_file: Some("<called:argument>\n"),
        },
        Case {
            name: "posix_function_and_source_preserve_shell_zero",
            script: r#"INITIAL_ZERO=$0
report_zero() {
    if [ "$0" = "$INITIAL_ZERO" ]; then
        printf 'function=kept:%s:%s\n' "$#" "$1"
    else
        printf 'function=changed:%s\n' "$0"
    fi
}
report_zero function-argument
cat >result.txt <<'EOF'
if [ "$0" = "$INITIAL_ZERO" ]; then
    printf 'source=kept:%s:%s\n' "$#" "$1"
else
    printf 'source=changed:%s\n' "$0"
fi
EOF
. ./result.txt source-argument
"#,
            expected_status: 0,
            expected_stdout: "function=kept:1:function-argument\nsource=kept:1:source-argument\n",
            expected_file: Some(
                "if [ \"$0\" = \"$INITIAL_ZERO\" ]; then\n    printf 'source=kept:%s:%s\\n' \"$#\" \"$1\"\nelse\n    printf 'source=changed:%s\\n' \"$0\"\nfi\n",
            ),
        },
        Case {
            name: "posix_command_substitution_and_subshell_preserve_shell_zero",
            script: r#"INITIAL_ZERO=$0
printf 'command=%s subshell=%s\n' "$( [ "$0" = "$INITIAL_ZERO" ] && printf yes )" "$( ( [ "$0" = "$INITIAL_ZERO" ] && printf yes ) )"
"#,
            expected_status: 0,
            expected_stdout: "command=yes subshell=yes\n",
            expected_file: None,
        },
        Case {
            name: "posix_shift_zero_preserves_parameters_and_default_shifts_one",
            script: r#"set -- first second third
shift 0
printf 'zero:%s:%s:%s\n' "$#" "$1" "$2"
shift
printf 'default:%s:%s:%s\n' "$#" "$1" "$2"
"#,
            expected_status: 0,
            expected_stdout: "zero:3:first:second\ndefault:2:second:third\n",
            expected_file: None,
        },
        Case {
            name: "posix_shift_inside_function_preserves_caller_positionals",
            script: r#"set -- outer-one outer-two
consume() {
    shift
    printf 'function:%s:%s\n' "$#" "$1"
}
consume inner-one inner-two
printf 'caller:%s:%s:%s\n' "$#" "$1" "$2"
"#,
            expected_status: 0,
            expected_stdout: "function:1:inner-two\ncaller:2:outer-one:outer-two\n",
            expected_file: None,
        },
        Case {
            name: "posix_colon_expands_arguments_and_honors_redirection",
            script: "ESH_COLON_VALUE=; : \"${ESH_COLON_VALUE:=set}\" >result.txt; printf '%s\\n' \"$ESH_COLON_VALUE\"\n",
            expected_status: 0,
            expected_stdout: "set\n",
            expected_file: Some(""),
        },
        Case {
            name: "posix_assignment_only_command_status",
            script: "PLAIN=value\nPLAIN_STATUS=$?\nFAILED=$(false)\nFAILED_STATUS=$?\nFIRST=$(false) LAST=$(true)\nMULTI_STATUS=$?\nFIRST=$(true) LAST=$(false)\nREVERSE_STATUS=$?\nX=1 <\"$(printf /dev/null; false)\"\nREDIRECTION_STATUS=$?\nprintf '%s:%s:%s:%s:%s\\n' \"$PLAIN_STATUS\" \"$FAILED_STATUS\" \"$MULTI_STATUS\" \"$REVERSE_STATUS\" \"$REDIRECTION_STATUS\"\n",
            expected_status: 0,
            expected_stdout: "0:1:0:1:0\n",
            expected_file: None,
        },
        Case {
            name: "posix_redirection_only_command_truncates_target",
            script: "printf old >result.txt\n>result.txt\nSTATUS=$?\nprintf 'status=%s content=<%s>\\n' \"$STATUS\" \"$(cat result.txt)\"\n",
            expected_status: 0,
            expected_stdout: "status=0 content=<>\n",
            expected_file: Some(""),
        },
        Case {
            name: "posix_compound_command_redirections_scope_output_and_state",
            script: r#"VALUE=before
{ VALUE=group; printf group-output; } >result.txt
( VALUE=subshell; printf sub-output; ) >subshell.txt
if true; then printf if-output; fi >if.txt
GROUP_STATUS=$?
printf 'value=%s group=<%s> sub=<%s> if=<%s> status=%s\n' \
    "$VALUE" "$(cat result.txt)" "$(cat subshell.txt)" "$(cat if.txt)" "$GROUP_STATUS"
"#,
            expected_status: 0,
            expected_stdout: "value=group group=<group-output> sub=<sub-output> if=<if-output> status=0\n",
            expected_file: Some("group-output"),
        },
        Case {
            name: "posix_pipeline_mixes_simple_and_compound_stages",
            script: r#"printf 'brace-pipeline\n' | { cat; } >result.txt
printf 'subshell-pipeline\n' | (cat) >subshell.txt
printf 'brace=<%s> subshell=<%s>\n' "$(cat result.txt)" "$(cat subshell.txt)"
"#,
            expected_status: 0,
            expected_stdout: "brace=<brace-pipeline> subshell=<subshell-pipeline>\n",
            expected_file: Some("brace-pipeline\n"),
        },
        Case {
            name: "posix_unset_removes_a_shell_variable",
            script: "ESH_POSIX_UNSET=present; unset ESH_POSIX_UNSET; printf '<%s>\\n' \"${ESH_POSIX_UNSET-unset}\"\n",
            expected_status: 0,
            expected_stdout: "<unset>\n",
            expected_file: None,
        },
        Case {
            name: "posix_unset_accepts_double_dash_before_names",
            script: "ESH_POSIX_UNSET=present; unset -- ESH_POSIX_UNSET; printf '<%s>\\n' \"${ESH_POSIX_UNSET-unset}\"\n",
            expected_status: 0,
            expected_stdout: "<unset>\n",
            expected_file: None,
        },
        Case {
            name: "posix_unset_selects_variables_and_functions_independently",
            script: r#"kept=variable
kept() { :; }
removed() { :; }
unset -v kept
unset -f removed
if test "${kept-unset}" = unset && command -v kept >/dev/null && ! command -v removed >/dev/null; then
    printf 'unset-namespaces-ok\n'
else
    printf 'unset-namespaces-mismatch\n'
fi
"#,
            expected_status: 0,
            expected_stdout: "unset-namespaces-ok\n",
            expected_file: None,
        },
        Case {
            name: "posix_dot_sources_in_caller_and_restores_positionals",
            script: "cat >result.txt <<'EOF'\nSOURCE_VALUE=loaded\nSOURCE_ARGS=\"$#:$1\"\nreturn 7\nSOURCE_AFTER=unreachable\nEOF\nset -- outer\n. ./result.txt first second\nprintf '<%s><%s><%s><%s>\\n' \"$SOURCE_VALUE\" \"$SOURCE_ARGS\" \"$1\" \"${SOURCE_AFTER-unset}\"\n",
            expected_status: 0,
            expected_stdout: "<loaded><2:first><outer><unset>\n",
            expected_file: Some(
                "SOURCE_VALUE=loaded\nSOURCE_ARGS=\"$#:$1\"\nreturn 7\nSOURCE_AFTER=unreachable\n",
            ),
        },
        Case {
            name: "posix_dot_without_operands_preserves_positionals",
            script: "cat >result.txt <<'EOF'\nSOURCE_ARGS=\"$#:$1\"\nEOF\nset -- outer\n. ./result.txt\nprintf '<%s><%s><%s>\\n' \"$SOURCE_ARGS\" \"$#\" \"$1\"\n",
            expected_status: 0,
            expected_stdout: "<1:outer><1><outer>\n",
            expected_file: Some("SOURCE_ARGS=\"$#:$1\"\n"),
        },
        Case {
            name: "posix_dot_searches_path_for_bare_source_name",
            script: "printf 'SOURCE_VALUE=loaded\\n' >result.txt\nPATH=.\n. result.txt\nprintf '<%s>\\n' \"$SOURCE_VALUE\"\n",
            expected_status: 0,
            expected_stdout: "<loaded>\n",
            expected_file: Some("SOURCE_VALUE=loaded\n"),
        },
        Case {
            name: "posix_dot_searches_current_directory_for_empty_path_component",
            script: "printf 'SOURCE_VALUE=loaded\\n' >result.txt\nPATH=/missing:\n. result.txt\nprintf '<%s>\\n' \"$SOURCE_VALUE\"\n",
            expected_status: 0,
            expected_stdout: "<loaded>\n",
            expected_file: Some("SOURCE_VALUE=loaded\n"),
        },
        Case {
            name: "posix_return_without_status_uses_last_command_status",
            script: r#"failed() { false; return; }
succeeded() { true; return; }
failed
printf 'function-failure=%s\n' "$?"
succeeded
printf 'function-success=%s\n' "$?"
cat >result.txt <<'EOF'
false
return
EOF
. ./result.txt
printf 'source-failure=%s\n' "$?"
"#,
            expected_status: 0,
            expected_stdout: "function-failure=1\nfunction-success=0\nsource-failure=1\n",
            expected_file: Some("false\nreturn\n"),
        },
        Case {
            name: "posix_exit_without_status_uses_last_command_status",
            script: "false\nexit\n",
            expected_status: 1,
            expected_stdout: "",
            expected_file: None,
        },
        Case {
            name: "zero_positional_at_and_star",
            script: r#"count_args() { printf '%s\n' "$#"; }
set --
count_args "$@"
count_args "$*"
"#,
            expected_status: 0,
            expected_stdout: "0\n1\n",
            expected_file: None,
        },
        Case {
            name: "posix_embedded_at_with_no_positional_parameters",
            script: r#"capture() { printf '%s:<%s>\n' "$#" "$1"; }
set --
capture prefix"$@"suffix
capture prefix$@suffix
"#,
            expected_status: 0,
            expected_stdout: "1:<prefixsuffix>\n1:<prefixsuffix>\n",
            expected_file: None,
        },
        Case {
            name: "exit_trap_runs_only_for_root_execution",
            script: r#"trap 'echo cleanup' EXIT
VALUE=$(printf nested)
(echo subshell)
printf 'outer:%s\n' "$VALUE"
"#,
            expected_status: 0,
            expected_stdout: "subshell\nouter:nested\ncleanup\n",
            expected_file: None,
        },
        Case {
            name: "posix_trap_zero_is_exit_alias",
            script: "trap 'echo cleanup' 0\necho body\n",
            expected_status: 0,
            expected_stdout: "body\ncleanup\n",
            expected_file: None,
        },
        Case {
            name: "posix_exit_trap_observes_pre_cleanup_status",
            script: "trap 'printf \"status=%s\\n\" \"$?\"' 0\nfalse\n",
            expected_status: 1,
            expected_stdout: "status=1\n",
            expected_file: None,
        },
        Case {
            name: "posix_exit_trap_expands_action_when_invoked",
            script: r#"VALUE=early
trap 'printf "trap-value=%s\n" "$VALUE"' EXIT
VALUE=late
"#,
            expected_status: 0,
            expected_stdout: "trap-value=late\n",
            expected_file: None,
        },
        Case {
            name: "simple_redirection_and_input",
            script: "printf 'payload\\n' >result.txt; cat <result.txt",
            expected_status: 0,
            expected_stdout: "payload\n",
            expected_file: Some("payload\n"),
        },
        Case {
            name: "ordered_standard_stream_duplication",
            script: "sh -c 'printf out; printf err >&2' 2>&1 >result.txt",
            expected_status: 0,
            expected_stdout: "err",
            expected_file: Some("out"),
        },
        Case {
            name: "standard_stream_duplication_into_pipeline",
            script: "sh -c 'printf out; printf err >&2' 2>&1 | tr 'a-z' 'A-Z'",
            expected_status: 0,
            expected_stdout: "OUTERR",
            expected_file: None,
        },
        Case {
            name: "posix_arithmetic_radices_and_precedence",
            script: "EXPR=1+2; printf '%s\\n' \"$((010))\" \"$((0x10))\" \"$((0X10))\" \"$((3 + 4 * 2))\" \"$((EXPR * 3))\"",
            expected_status: 0,
            expected_stdout: "8\n16\n16\n11\n9\n",
            expected_file: None,
        },
        Case {
            name: "posix_arithmetic_signed_division_and_remainder",
            script: "printf '%s:%s:%s:%s\\n' \"$((-7 / 3))\" \"$((-7 % 3))\" \"$((7 / -3))\" \"$((7 % -3))\"",
            expected_status: 0,
            expected_stdout: "-2:-1:-2:1\n",
            expected_file: None,
        },
        Case {
            name: "posix_arithmetic_variable_integer_constant_radices",
            script: "OCTAL=010; SIGNED_OCTAL=+010; NEGATIVE_OCTAL=-010; HEX=0x10; UPPER_HEX=0X10; SIGNED_HEX=+0X10; NEGATIVE_HEX=-0X10; printf '%s:%s:%s:%s:%s:%s:%s\\n' \"$((OCTAL))\" \"$((SIGNED_OCTAL))\" \"$((NEGATIVE_OCTAL))\" \"$((HEX))\" \"$((UPPER_HEX))\" \"$((SIGNED_HEX))\" \"$((NEGATIVE_HEX))\"",
            expected_status: 0,
            expected_stdout: "8:8:-8:16:16:16:-16\n",
            expected_file: None,
        },
        Case {
            name: "posix_arithmetic_shift_and_bitwise_precedence",
            script: "printf '%s:%s:%s:%s\\n' \"$((1 << 2 + 1))\" \"$((16 >> 1 + 1))\" \"$((1 | 2 & 4))\" \"$((1 & 3 == 1))\"",
            expected_status: 0,
            expected_stdout: "8:4:1:0\n",
            expected_file: None,
        },
        Case {
            name: "posix_arithmetic_compound_shift_and_bitwise_assignments",
            script: "LEFT=1; RIGHT=16; AND=7; XOR=5; OR=4; printf '%s:%s:%s:%s:%s\\n' \"$((LEFT <<= 2))\" \"$((RIGHT >>= 2))\" \"$((AND &= 3))\" \"$((XOR ^= 3))\" \"$((OR |= 3))\"; printf '%s:%s:%s:%s:%s\\n' \"$LEFT\" \"$RIGHT\" \"$AND\" \"$XOR\" \"$OR\"",
            expected_status: 0,
            expected_stdout: "4:4:3:6:7\n4:4:3:6:7\n",
            expected_file: None,
        },
        Case {
            name: "posix_arithmetic_negative_division_truncates_toward_zero",
            script: "printf '%s:%s:%s:%s\\n' \"$((-7 / 3))\" \"$((-7 % 3))\" \"$((7 / -3))\" \"$((7 % -3))\"",
            expected_status: 0,
            expected_stdout: "-2:-1:-2:1\n",
            expected_file: None,
        },
        Case {
            name: "posix_arithmetic_assignment_short_circuit_and_comma",
            script: "A=1; B=0; printf '%s\\n' \"$((A += 4))\" \"$((B = A > 4 ? A : 9))\" \"$((0 && (A = 99)))\" \"$A\" \"$((1 || (A = 88)))\" \"$A\" \"$((1, A + 1))\" \"$B\"",
            expected_status: 0,
            expected_stdout: "5\n5\n0\n5\n1\n5\n6\n5\n",
            expected_file: None,
        },
        Case {
            name: "posix_arithmetic_chained_assignment_is_right_associative",
            script: "A=0; B=0; printf '%s:%s:%s\\n' \"$((A = B = 7))\" \"$A\" \"$B\"",
            expected_status: 0,
            expected_stdout: "7:7:7\n",
            expected_file: None,
        },
        Case {
            name: "posix_arithmetic_conditional_operator_is_right_associative",
            script: "printf '%s:%s\\n' \"$((1 ? 0 : 1 ? 2 : 3))\" \"$((0 ? 1 : 1 ? 2 : 3))\"",
            expected_status: 0,
            expected_stdout: "0:2\n",
            expected_file: None,
        },
        Case {
            name: "posix_printf_percent_b_c_escape_stops_output",
            script: r#"printf '<%b>|%s' 'left\cright' tail
printf '\n'
printf '%b' 'slash\\ctext'
printf '\n'
"#,
            expected_status: 0,
            expected_stdout: "<left\nslash\\ctext\n",
            expected_file: None,
        },
        Case {
            name: "posix_printf_percent_b_precision_before_c_escape",
            script: "printf '<%.2b>|%s' 'abc\\cignored' tail\nprintf '\\nnext\\n'\n",
            expected_status: 0,
            expected_stdout: "<ab\nnext\n",
            expected_file: None,
        },
        Case {
            name: "posix_printf_octal_bytes_survive_redirection",
            script: "printf '\\303\\251' >result.txt\ncat result.txt\n",
            expected_status: 0,
            expected_stdout: "é",
            expected_file: Some("é"),
        },
        Case {
            name: "posix_printf_percent_b_octal_bytes_are_preserved",
            script: r#"printf '%b' '\0303\0251'"#,
            expected_status: 0,
            expected_stdout: "é",
            expected_file: None,
        },
        Case {
            name: "posix_printf_percent_b_zero_octal_length_boundaries",
            script: r#"printf '%b' '\0|\00|\000|\0000|\00000'"#,
            expected_status: 0,
            expected_stdout: "\0|\0|\0|\0|\x000",
            expected_file: None,
        },
        Case {
            name: "posix_printf_named_backslash_escape_sequences",
            script: r#"printf '\a\b\f\n\r\t\v\\|%b' '\a\b\f\n\r\t\v\\'"#,
            expected_status: 0,
            expected_stdout: "\x07\x08\x0c\n\r\t\x0b\\|\x07\x08\x0c\n\r\t\x0b\\",
            expected_file: None,
        },
        Case {
            name: "posix_printf_integer_base_and_unsigned_conversions",
            script: r##"printf '%d %i %o %u %x %X %d\n' 010 0x10 8 -1 15 15 "'A""##,
            expected_status: 0,
            expected_stdout: "8 16 10 18446744073709551615 f F 65\n",
            expected_file: None,
        },
        Case {
            name: "posix_printf_field_width_flags_and_precision",
            script: "printf '[%-6.3s][%+06d][%#06x][%.2f]\\n' alphabet 42 42 1.2",
            expected_status: 0,
            expected_stdout: "[alp   ][+00042][0x002a][1.20]\n",
            expected_file: None,
        },
        Case {
            name: "posix_printf_conflicting_flags_follow_precedence",
            script: r#"printf '<%+ d><% +d><%0-5d>\n' 7 7 7"#,
            expected_status: 0,
            expected_stdout: "<+7><+7><7    >\n",
            expected_file: None,
        },
        Case {
            name: "posix_printf_repeats_format_and_defaults_missing_operands",
            script: "printf '<%s:%d>\\n' one 2 three\nprintf '<%s:%d>\\n'\n",
            expected_status: 0,
            expected_stdout: "<one:2>\n<three:0>\n<:0>\n",
            expected_file: None,
        },
        Case {
            name: "posix_printf_alternate_octal_with_zero_precision",
            script: "printf '[%#.0o][%#.0x][%#.0f]\\n' 0 0 2",
            expected_status: 0,
            expected_stdout: "[0][][2.]\n",
            expected_file: None,
        },
        Case {
            name: "posix_printf_dynamic_width_and_precision",
            script: "printf '[%*.*s][%*d]\\n' 6 3 alphabet -5 12",
            expected_status: 0,
            expected_stdout: "[   alp][12   ]\n",
            expected_file: None,
        },
        Case {
            name: "posix_printf_negative_dynamic_precision_is_omitted",
            script: "printf '[%.*f][%*.*s][%.*d]\\n' -1 1.25 -4 -1 abc -1 7",
            expected_status: 0,
            expected_stdout: "[1.250000][abc ][7]\n",
            expected_file: None,
        },
        Case {
            name: "posix_printf_scientific_and_general_float_conversions",
            script: "printf '[%e][%E][%.3g][%.3G][%F]\\n' 1.25 1.25 12345 0.0012 1.5",
            expected_status: 0,
            expected_stdout: "[1.250000e+00][1.250000E+00][1.23e+04][0.0012][1.500000]\n",
            expected_file: None,
        },
        Case {
            name: "posix_printf_float_alternate_and_precision_zero",
            script: "printf '[%.0e][%#.4g][%+g]\\n' 12 1.2 2",
            expected_status: 0,
            expected_stdout: "[1e+01][1.200][+2]\n",
            expected_file: None,
        },
        Case {
            name: "posix_printf_float_sign_flags_apply_to_nan",
            script: "printf '<%+f><% f><%F><%+g><% f>\\n' nan nan -nan nan -nan",
            expected_status: 0,
            expected_stdout: "<+nan>< nan><-NAN><+nan><-nan>\n",
            expected_file: None,
        },
        Case {
            name: "posix_printf_zero_padding_is_ignored_for_infinity_and_nan",
            script: "printf '[%010f][%010f][%010f][%010f][%+010f][%010F]\\n' inf -inf nan -nan nan nan",
            expected_status: 0,
            expected_stdout: "[       inf][      -inf][       nan][      -nan][      +nan][       NAN]\n",
            expected_file: None,
        },
        Case {
            name: "posix_printf_scientific_alternate_form_keeps_decimal_point",
            script: "printf '[%#.0e][%#.0E]\\n' 12 12",
            expected_status: 0,
            expected_stdout: "[1.e+01][1.E+01]\n",
            expected_file: None,
        },
        Case {
            name: "posix_printf_general_float_rounding_thresholds",
            script: "printf '[%.6g][%.6G][%.6g][%.6g][%.0g][%#.0g]\\n' 999999.5 999999.5 0.00009999995 0.000009999995 42 42",
            expected_status: 0,
            expected_stdout: "[1e+06][1E+06][0.0001][1e-05][4e+01][4.e+01]\n",
            expected_file: None,
        },
        Case {
            name: "posix_printf_hexadecimal_float_operands",
            script: "printf '[%f][%e][%g]\\n' 0x1.8p+2 0x1p4 0x1p-4",
            expected_status: 0,
            expected_stdout: "[6.000000][1.600000e+01][0.0625]\n",
            expected_file: None,
        },
        Case {
            name: "posix_printf_length_modifiers",
            script: "printf '%hhd %hd %lld %zu %Lf\\n' 257 257 257 257 1.5",
            expected_status: 0,
            expected_stdout: "257 257 257 257 1.500000\n",
            expected_file: None,
        },
        Case {
            name: "posix_arithmetic_signed_minimum_literal",
            script: "printf '%s\\n' \"$((-9223372036854775808))\"",
            expected_status: 0,
            expected_stdout: "-9223372036854775808\n",
            expected_file: None,
        },
        Case {
            name: "posix_parameter_default_alternate_and_length",
            script: r#"EMPTY=
printf '%s|%s|%s|%s|%s|%s|%s|%s|%s\n' \
    "${ESH_POSIX_UNSET_71-word}" "${ESH_POSIX_UNSET_71:-fallback}" \
    "${EMPTY-word}" "${EMPTY:-fallback}" \
    "${ESH_POSIX_UNSET_71+alt}" "${EMPTY+alt}" \
    "${ESH_POSIX_UNSET_71:+alt}" "${EMPTY:+alt}" "${#EMPTY}"
"#,
            expected_status: 0,
            expected_stdout: "word|fallback||fallback||alt|||0\n",
            expected_file: None,
        },
        Case {
            name: "posix_number_parameter_operators_are_not_string_length_expansions",
            script: r#"set -u
set -- one two
printf '<%s><%s><%s><%s><%s><%s>\n' "${#-fallback}" "${#:-fallback}" "${#+alternate}" "${#:+alternate}" "${#?missing}" "${#:?missing}"
set --
printf '<%s><%s><%s><%s><%s><%s>\n' "${#-fallback}" "${#:-fallback}" "${#+alternate}" "${#:+alternate}" "${#?missing}" "${#:?missing}"
"#,
            expected_status: 0,
            expected_stdout: "<2><2><alternate><alternate><2><2>\n<0><0><alternate><alternate><0><0>\n",
            expected_file: None,
        },
        Case {
            name: "posix_special_parameter_default_operators_preserve_at_fields",
            script: r#"show_args() { printf '%s|' "$#"; printf '[%s]' "$@"; printf '\n'; }
set -- 'one two' three
show_args "${@:-fallback}"
printf 'star=<%s>\n' "${*:-fallback}"
printf 'star-no-colon=<%s>\n' "${*-fallback}"
printf 'star-plus=<%s>\n' "${*+alternate}"
show_args "${@:+alternate}"
show_args "${@+alternate}"
VALUE=${@:-fallback}
printf 'assigned=<%s>\n' "$VALUE"
set --
show_args "${@:-fallback}"
show_args "${@:+alternate}"
show_args "${@+alternate}"
printf 'star-empty=<%s>\n' "${*:-fallback}"
printf 'star-unset-no-colon=<%s>\n' "${*-fallback}"
printf 'star-unset-plus=<%s>\n' "${*+alternate}"
show_args "${@:-fallback value}"
set -- ''
show_args "${@-fallback}"
show_args "${@:-fallback}"
show_args "${@:+alternate}"
show_args "${@+alternate}"
printf 'star-null-no-colon=<%s>\n' "${*-fallback}"
printf 'star-null-plus=<%s>\n' "${*+alternate}"
"#,
            expected_status: 0,
            expected_stdout: "2|[one two][three]\nstar=<one two three>\nstar-no-colon=<one two three>\nstar-plus=<alternate>\n1|[alternate]\n1|[alternate]\nassigned=<one two three>\n1|[fallback]\n0|[]\n0|[]\nstar-empty=<fallback>\nstar-unset-no-colon=<fallback>\nstar-unset-plus=<>\n1|[fallback value]\n1|[]\n1|[fallback]\n1|[]\n1|[alternate]\nstar-null-no-colon=<>\nstar-null-plus=<alternate>\n",
            expected_file: None,
        },
        Case {
            name: "posix_nested_parameter_operators_and_braced_positionals",
            script: r#"set -- one two three four five six seven eight nine ten
EMPTY=
printf '<%s><%s><%s>\n' "${10}" "${ESH_POSIX_NESTED_UNSET_72:-${EMPTY:-fallback}}" "${EMPTY:+${10}}"
printf '<%s>\n' "${ESH_NESTED_BRACE_UNSET:-${ESH_INNER_BRACE_UNSET:-"quoted } fallback"}}"
printf '<%s><%s>\n' "${ESH_SPACE_DEFAULT_UNSET:-space\ value}" "${ESH_DOLLAR_DEFAULT_UNSET:-\$literal}"
"#,
            expected_status: 0,
            expected_stdout: "<ten><fallback><>\n<quoted } fallback>\n<space\\ value><$literal>\n",
            expected_file: None,
        },
        Case {
            name: "posix_nested_default_expansion_inside_patterns",
            script: r#"VALUE=pre-middle-end
printf '<%s><%s><%s>\n' "${VALUE#${PREFIX:-pre}-}" "${VALUE%-${SUFFIX:-end}}" "${MISSING:-${VALUE#pre-}}"
"#,
            expected_status: 0,
            expected_stdout: "<middle-end><pre-middle><middle-end>\n",
            expected_file: None,
        },
        Case {
            name: "posix_nounset_nested_defaults_inside_pattern_removal",
            script: r#"set -u
VALUE=prefix-middle-suffix
printf '<%s><%s>\n' "${VALUE#${UNSET_PREFIX:-prefix-}}" "${VALUE%-${UNSET_SUFFIX:-suffix}}"
"#,
            expected_status: 0,
            expected_stdout: "<middle-suffix><prefix-middle>\n",
            expected_file: None,
        },
        Case {
            name: "posix_nested_pattern_removal_inside_pattern_defaults",
            script: r#"VALUE=prefix-middle-suffix
PREFIX_SOURCE=pre-prefix
SUFFIX_SOURCE=tail-suffix
printf '<%s><%s>\n' "${VALUE#${PREFIX:-${PREFIX_SOURCE#pre-}}-}" "${VALUE%-${SUFFIX:-${SUFFIX_SOURCE#tail-}}}"
"#,
            expected_status: 0,
            expected_stdout: "<middle-suffix><prefix-middle>\n",
            expected_file: None,
        },
        Case {
            name: "posix_nested_parameter_assignments_inside_pattern_removal",
            script: r#"VALUE=prefix-middle-suffix
unset PREFIX SUFFIX
PREFIX_RESULT=${VALUE#${PREFIX:=prefix}-}
SUFFIX_RESULT=${VALUE%-${SUFFIX=middle-suffix}}
printf '<%s><%s><%s><%s>\n' "$PREFIX_RESULT" "$PREFIX" "$SUFFIX_RESULT" "$SUFFIX"
"#,
            expected_status: 0,
            expected_stdout: "<middle-suffix><prefix><prefix><middle-suffix>\n",
            expected_file: None,
        },
        Case {
            name: "posix_nested_required_operators_inside_pattern_removal",
            script: r#"VALUE=prefix-middle-suffix
PREFIX=prefix
SUFFIX=suffix
PREFIX_RESULT=${VALUE#${PREFIX?missing-prefix}-}
SUFFIX_RESULT=${VALUE%-${SUFFIX:?missing-suffix}}
printf '<%s><%s>\n' "$PREFIX_RESULT" "$SUFFIX_RESULT"
"#,
            expected_status: 0,
            expected_stdout: "<middle-suffix><prefix-middle>\n",
            expected_file: None,
        },
        Case {
            name: "posix_arithmetic_expansion_inside_pattern_removal",
            script: r#"VALUE=pre4-middle-suffix
COUNT=2
RESULT=${VALUE#pre$((COUNT + 2))*-}
printf '<%s>\n' "$RESULT"
"#,
            expected_status: 0,
            expected_stdout: "<middle-suffix>\n",
            expected_file: None,
        },
        Case {
            name: "posix_nested_alternate_operators_inside_pattern_removal",
            script: r#"VALUE=prefix-middle-suffix
PREFIX=prefix
SUFFIX=suffix
EMPTY=
PREFIX_RESULT=${VALUE#${PREFIX+prefix}-}
SUFFIX_RESULT=${VALUE%-${SUFFIX+suffix}}
EMPTY_PREFIX_RESULT=${VALUE#${EMPTY+prefix}-}
EMPTY_SUFFIX_RESULT=${VALUE%-${EMPTY+suffix}}
EMPTY_COLON_PREFIX_RESULT=${VALUE#${EMPTY:+prefix}-}
EMPTY_COLON_SUFFIX_RESULT=${VALUE%-${EMPTY:+suffix}}
unset PREFIX SUFFIX EMPTY
UNSET_PREFIX_RESULT=${VALUE#${PREFIX+prefix}-}
UNSET_SUFFIX_RESULT=${VALUE%-${SUFFIX+suffix}}
UNSET_COLON_PREFIX_RESULT=${VALUE#${EMPTY:+prefix}-}
UNSET_COLON_SUFFIX_RESULT=${VALUE%-${EMPTY:+suffix}}
printf '<%s><%s><%s><%s><%s><%s><%s><%s><%s><%s>\n' \
    "$PREFIX_RESULT" "$SUFFIX_RESULT" "$EMPTY_PREFIX_RESULT" "$EMPTY_SUFFIX_RESULT" \
    "$EMPTY_COLON_PREFIX_RESULT" "$EMPTY_COLON_SUFFIX_RESULT" \
    "$UNSET_PREFIX_RESULT" "$UNSET_SUFFIX_RESULT" "$UNSET_COLON_PREFIX_RESULT" "$UNSET_COLON_SUFFIX_RESULT"
"#,
            expected_status: 0,
            expected_stdout: "<middle-suffix><prefix-middle><middle-suffix><prefix-middle><prefix-middle-suffix><prefix-middle-suffix><prefix-middle-suffix><prefix-middle-suffix><prefix-middle-suffix><prefix-middle-suffix>\n",
            expected_file: None,
        },
        Case {
            name: "posix_nested_default_and_assignment_inside_pattern_removal",
            script: r#"VALUE=prefix-middle-suffix
unset PREFIX FALLBACK
RESULT=${VALUE#${PREFIX:-${FALLBACK:=prefix}}-}
printf '<%s><%s><%s>\n' "$RESULT" "${PREFIX-unset}" "$FALLBACK"
"#,
            expected_status: 0,
            expected_stdout: "<middle-suffix><unset><prefix>\n",
            expected_file: None,
        },
        Case {
            name: "posix_parameter_pattern_expansion_preserves_quote_provenance",
            script: r#"VALUE=prefix-tail
PATTERN='pre*'
printf '<%s><%s>\n' "${VALUE#$(printf '%s' "$PATTERN")}" "${VALUE#"$PATTERN"}"
"#,
            expected_status: 0,
            expected_stdout: "<fix-tail><prefix-tail>\n",
            expected_file: None,
        },
        Case {
            name: "posix_nested_command_substitution_inside_parameter_pattern",
            script: r#"VALUE=prefix-tail
RESULT=${VALUE#$(printf '%s' "$(printf 'pre' ')')")}
printf '<%s>\n' "$RESULT"
"#,
            expected_status: 0,
            expected_stdout: "<fix-tail>\n",
            expected_file: None,
        },
        Case {
            name: "posix_command_substitution_ignores_parentheses_in_quotes_and_comments",
            script: "SINGLE=$(printf '%s' '(')\nDOUBLE=$(printf '%s' \"(\")\nCLOSE=$(printf '%s' ')')\nARITH=$(printf '%s' \"$((1+1))\")\nSHIFT=$(printf '%s' $((8<<1)))\nCOMMENT=$(printf value # )\nprintf continued\n)\nprintf '<%s><%s><%s><%s><%s><%s>\\n' \"$SINGLE\" \"$DOUBLE\" \"$CLOSE\" \"$ARITH\" \"$SHIFT\" \"$COMMENT\"\n",
            expected_status: 0,
            expected_stdout: "<(><(><)><2><16><valuecontinued>\n",
            expected_file: None,
        },
        Case {
            name: "posix_command_substitution_skips_fifo_heredoc_bodies",
            script: "RESULT=$(cat <<'FIRST' <<-SECOND\nfirst (\nFIRST\n\tsecond )\n\tSECOND\n)\nprintf '<%s>\\n' \"$RESULT\"\n",
            expected_status: 0,
            expected_stdout: "<second )>\n",
            expected_file: None,
        },
        Case {
            name: "posix_leading_home_tilde_expansion",
            script: r#"HOME='/esh home'
printf '<%s>\n' ~ ~/release "~" \~/literal prefix~/literal
"#,
            expected_status: 0,
            expected_stdout: "</esh home>\n</esh home/release>\n<~>\n<~/literal>\n<prefix~/literal>\n",
            expected_file: None,
        },
        Case {
            name: "posix_tilde_expansion_in_assignment_and_redirection",
            script: r#"mkdir -p "home with spaces"
HOME="home with spaces"
OUTPUT=~/result.txt
printf '<%s>\n' "$OUTPUT"
printf payload > ~/result.txt
test -f "$OUTPUT" && echo redirection=ok
"#,
            expected_status: 0,
            expected_stdout: "<home with spaces/result.txt>\nredirection=ok\n",
            expected_file: None,
        },
        Case {
            name: "posix_getopts_cluster_and_optind",
            script: r#"set -- -abc -- tail
OPTIND=1
while getopts abc option; do
    printf '%s:%s\n' "$option" "$OPTIND"
done
printf 'end:%s\n' "$OPTIND"
"#,
            expected_status: 0,
            expected_stdout: "a:1\nb:1\nc:2\nend:3\n",
            expected_file: None,
        },
        Case {
            name: "posix_getopts_reset_optind_restarts_cluster",
            script: r#"set -- -abc
OPTIND=1
getopts abc option
printf 'first=%s:%s\n' "$option" "$OPTIND"
OPTIND=1
getopts abc option
printf 'reset=%s:%s\n' "$option" "$OPTIND"
"#,
            expected_status: 0,
            expected_stdout: "first=a:1\nreset=a:1\n",
            expected_file: None,
        },
        Case {
            name: "posix_getopts_end_sets_result_and_unsets_optarg",
            script: r#"set -- operand
option=stale
OPTARG=stale
OPTIND=1
getopts a option
printf '%s:<%s>:%s\n' "$option" "${OPTARG-unset}" "$OPTIND"
set -- -- tail
OPTIND=1
getopts a option
printf '%s:<%s>:%s\n' "$option" "${OPTARG-unset}" "$OPTIND"
"#,
            expected_status: 0,
            expected_stdout: "?:<unset>:1\n?:<unset>:2\n",
            expected_file: None,
        },
        Case {
            name: "posix_getopts_silent_unknown_and_missing_argument",
            script: r#"set -- -az -b
OPTIND=1
while getopts :ab: option; do
    printf '%s:<%s>:%s\n' "$option" "$OPTARG" "$OPTIND"
done
printf 'end:%s\n' "$OPTIND"
"#,
            expected_status: 0,
            expected_stdout: "a:<>:1\n?:<z>:2\n::<b>:3\nend:3\n",
            expected_file: None,
        },
        Case {
            name: "legacy_backtick_command_substitution",
            script: r#"VALUE=`printf bare`
printf 'bare=<%s>\n' "$VALUE"
printf 'quoted=<%s>\n' "value-`printf nested`"
printf 'escaped=<%s>\n' "literal \`"
printf 'dollar=<%s>\n' "`printf \$`"
printf 'backslash=<%s>\n' "`printf \\\\`"
printf 'ordinary=<%s>\n' "`printf \q`"
cat <<EOF
heredoc=`printf expanded`
EOF
"#,
            expected_status: 0,
            expected_stdout: "bare=<bare>\nquoted=<value-nested>\nescaped=<literal `>\ndollar=<$>\nbackslash=<\\>\nordinary=<q>\nheredoc=expanded\n",
            expected_file: None,
        },
        Case {
            name: "posix_double_quoted_backslash_escape_set",
            script: r#"printf '<%s>\n' "dollar=\$HOME" "backtick=\`" "quote=\"" "slash=\\" "ordinary=\q"
"#,
            expected_status: 0,
            expected_stdout: "<dollar=$HOME>\n<backtick=`>\n<quote=\">\n<slash=\\>\n<ordinary=\\q>\n",
            expected_file: None,
        },
        Case {
            name: "posix_double_quoted_physical_newline_is_preserved",
            script: "printf '<%s>\\n' \"first\nsecond\"\n",
            expected_status: 0,
            expected_stdout: "<first\nsecond>\n",
            expected_file: None,
        },
        Case {
            name: "posix_single_quoted_physical_newline_is_preserved",
            script: "printf '<%s>\\n' 'first\nsecond'\n",
            expected_status: 0,
            expected_stdout: "<first\nsecond>\n",
            expected_file: None,
        },
        Case {
            name: "posix_double_quoted_backslash_newline_continuation",
            script: "VALUE=\"left\\\nright\"\nprintf '<%s>\\n' \"$VALUE\" \"first\\\nsecond\"\n",
            expected_status: 0,
            expected_stdout: "<leftright>\n<firstsecond>\n",
            expected_file: None,
        },
        Case {
            name: "posix_backslash_newline_is_context_sensitive",
            script: "printf '<%s>\\n' left\\\nright\nprintf '<%s>\\n' 'single\\\nquoted'\n",
            expected_status: 0,
            expected_stdout: "<leftright>\n<single\\\nquoted>\n",
            expected_file: None,
        },
        Case {
            name: "posix_printf_b_c_suppresses_remaining_invocation_output",
            script: r#"printf '<%b%s\n>' 'value\cignored' hidden
printf '<next-command>\n'
"#,
            expected_status: 0,
            expected_stdout: "<value<next-command>\n",
            expected_file: None,
        },
        Case {
            name: "posix_multiple_heredocs_collected_inside_command_substitution",
            script: "VALUE=$(cat <<FIRST <<'SECOND'\ndiscarded body\nFIRST\ninside substitution\nSECOND\n)\nprintf '<%s>\\n' \"$VALUE\"\n",
            expected_status: 0,
            expected_stdout: "<inside substitution>\n",
            expected_file: None,
        },
        Case {
            name: "posix_tab_stripping_quoted_heredoc_inside_command_substitution",
            script: "VALUE=expanded\nRESULT=$(cat <<-'EOF'\n\t$VALUE\n\t$(printf nested)\n\tEOF\n)\nprintf '<%s>\\n' \"$RESULT\"\n",
            expected_status: 0,
            expected_stdout: "<$VALUE\n$(printf nested)>\n",
            expected_file: None,
        },
        Case {
            name: "posix_command_substitution_removes_only_trailing_newlines",
            script: "printf '<%s>\\n' \"$(printf 'modern\\r\\r\\n\\n')\"\nprintf '<%s>\\n' \"`printf 'legacy\\r\\r\\n\\n'`\"\nprintf '<%s>\\n' \"$(printf 'first\\nsecond\\n\\n')\"\nprintf '<%s>\\n' \"`printf 'third\\nfourth\\n\\n'`\"\ncat <<EOF\n$(printf 'body\\r\\r\\n\\n')\nEOF\n",
            expected_status: 0,
            expected_stdout: "<modern\r\r>\n<legacy\r\r>\n<first\nsecond>\n<third\nfourth>\nbody\r\r\n",
            expected_file: None,
        },
        Case {
            name: "posix_nested_legacy_backtick_substitution",
            script: r#"VALUE=`printf '<%s>' \`printf inner\``
printf '%s\n' "$VALUE"
"#,
            expected_status: 0,
            expected_stdout: "<inner>\n",
            expected_file: None,
        },
        Case {
            name: "posix_heredoc_backslashes_and_continuation",
            script: r#"VALUE=expanded
cat <<EOF
$VALUE
\$VALUE
\\$VALUE
escaped-backtick: \`printf must-not-run\`
escaped-single: \`
ordinary-backslash: \q
escaped-double-quote: \"quoted\"
"quotes stay literal"
left\
right
EOF
"#,
            expected_status: 0,
            expected_stdout: r#"expanded
$VALUE
\expanded
escaped-backtick: `printf must-not-run`
escaped-single: `
ordinary-backslash: \q
escaped-double-quote: \"quoted\"
"quotes stay literal"
leftright
"#,
            expected_file: None,
        },
        Case {
            name: "posix_unquoted_heredoc_backslash_parity_controls_expansion",
            script: r#"VALUE=expanded
cat <<EOF
\$VALUE
\\$VALUE
\\\$VALUE
\\\\$VALUE
EOF
"#,
            expected_status: 0,
            expected_stdout: r#"$VALUE
\expanded
\$VALUE
\\expanded
"#,
            expected_file: None,
        },
        Case {
            name: "posix_heredoc_continuation_precedes_delimiter_recognition",
            script: "cat <<EOF\nleft\\\nEOF\nright\nEOF\n",
            expected_status: 0,
            expected_stdout: "leftEOF\nright\n",
            expected_file: None,
        },
        Case {
            name: "posix_heredoc_mixed_quoted_delimiters",
            script: r#"VALUE=expanded
cat <<E"OF"
$VALUE
EOF
cat <<\LITERAL
$VALUE
LITERAL
"#,
            expected_status: 0,
            expected_stdout: "$VALUE\n$VALUE\n",
            expected_file: None,
        },
        Case {
            name: "posix_heredoc_delimiter_words_preserve_quoted_whitespace",
            script: r#"VALUE=expanded
cat <<END\ TAG
$VALUE
END TAG
cat <<'END TAG'
$VALUE
END TAG
"#,
            expected_status: 0,
            expected_stdout: "$VALUE\n$VALUE\n",
            expected_file: None,
        },
        Case {
            name: "posix_heredoc_double_quoted_delimiter_backslash_rules",
            script: r#"VALUE=expanded
cat <<"E\OF"
$VALUE
E\OF
cat <<"E\"OF"
$VALUE
E"OF
"#,
            expected_status: 0,
            expected_stdout: "$VALUE\n$VALUE\n",
            expected_file: None,
        },
        Case {
            name: "posix_quoted_heredoc_preserves_backslash_newline",
            script: "cat <<'EOF'\nleft\\\nright\nEOF\n",
            expected_status: 0,
            expected_stdout: "left\\\nright\n",
            expected_file: None,
        },
        Case {
            name: "posix_tab_stripping_heredoc_removes_only_leading_tabs",
            script: "cat <<-END\n\tstripped\n \tspace-before-tab\n \tEND\n\tEND\n",
            expected_status: 0,
            expected_stdout: "stripped\n \tspace-before-tab\n \tEND\n",
            expected_file: None,
        },
        Case {
            name: "posix_heredoc_delimiter_expansion_is_not_performed",
            script: r#"END=CHANGED
cat <<$END
variable delimiter
$END
"#,
            expected_status: 0,
            expected_stdout: "variable delimiter\n",
            expected_file: None,
        },
        Case {
            name: "posix_heredoc_command_and_arithmetic_delimiters_are_not_expanded",
            script: r#"cat <<$(printf END)
command delimiter
$(printf END)
cat <<$((1+1))
arithmetic delimiter
$((1+1))
"#,
            expected_status: 0,
            expected_stdout: "command delimiter\narithmetic delimiter\n",
            expected_file: None,
        },
        Case {
            name: "posix_unquoted_heredoc_keeps_quote_bytes_while_expanding",
            script: r#"VALUE=word
cat <<EOF
"$VALUE" '$VALUE' "$(printf cmd)" '$((1+1))'
EOF
"#,
            expected_status: 0,
            expected_stdout: "\"word\" 'word' \"cmd\" '2'\n",
            expected_file: None,
        },
        Case {
            name: "posix_heredoc_expansion_does_not_split_or_expand_globs",
            script: r#": >match.txt
VALUE='alpha beta *.txt'
cat <<EOF
<$VALUE>
EOF
"#,
            expected_status: 0,
            expected_stdout: "<alpha beta *.txt>\n",
            expected_file: None,
        },
        Case {
            name: "posix_unquoted_heredoc_keeps_unmatched_quotes_and_syntax_bytes",
            script: r#"VALUE=expanded
cat <<EOF
unmatched " and ' quotes; # if then $VALUE
EOF
"#,
            expected_status: 0,
            expected_stdout: "unmatched \" and ' quotes; # if then expanded\n",
            expected_file: None,
        },
        Case {
            name: "posix_heredoc_delimiter_joins_escaped_source_newline",
            script: "cat <<EO\\\nF\nbody\nEOF\n",
            expected_status: 0,
            expected_stdout: "body\n",
            expected_file: None,
        },
        Case {
            name: "posix_heredoc_nested_command_substitution",
            script: r#"VALUE=outer
cat <<EOF
$(printf 'inner:%s' "$(printf '%s' "$VALUE")")
EOF
"#,
            expected_status: 0,
            expected_stdout: "inner:outer\n",
            expected_file: None,
        },
        Case {
            name: "posix_heredoc_command_substitution_contains_nested_heredoc",
            script: r#"cat <<OUTER
before
$(cat <<INNER
inside
INNER
)
after
OUTER
"#,
            expected_status: 0,
            expected_stdout: "before\ninside\nafter\n",
            expected_file: None,
        },
        Case {
            name: "posix_heredoc_nested_default_and_command_substitution",
            script: r#"VALUE=
FALLBACK=
cat <<EOF
${VALUE:-${FALLBACK:-$(printf '<%s>' "$(printf nested)")}}
EOF
"#,
            expected_status: 0,
            expected_stdout: "<nested>\n",
            expected_file: None,
        },
        Case {
            name: "posix_heredoc_nested_legacy_backtick_substitution",
            script: r#"cat <<EOF
nested=`printf '<%s>' \`printf inner\``
EOF
"#,
            expected_status: 0,
            expected_stdout: "nested=<inner>\n",
            expected_file: None,
        },
        Case {
            name: "posix_heredoc_arithmetic_expansion_and_escape",
            script: r#"VALUE=6
cat <<EOF
$((VALUE * 7))
\$((VALUE * 7))
EOF
"#,
            expected_status: 0,
            expected_stdout: "42\n$((VALUE * 7))\n",
            expected_file: None,
        },
        Case {
            name: "posix_heredoc_delimiter_rejects_trailing_blanks",
            script: "cat <<EOF\nbefore\nEOF \nafter\nEOF\n",
            expected_status: 0,
            expected_stdout: "before\nEOF \nafter\n",
            expected_file: None,
        },
        Case {
            name: "heredoc_closing_delimiter_at_eof_without_final_newline",
            script: "cat <<EOF\nbody\nEOF",
            expected_status: 0,
            expected_stdout: "body\n",
            expected_file: None,
        },
        Case {
            name: "posix_heredoc_crlf_recognizes_delimiter_and_preserves_body_bytes",
            script: "cat <<EOF\r\nbody\r\nEOF\r\n",
            expected_status: 0,
            expected_stdout: "body\r\n",
            expected_file: None,
        },
        Case {
            name: "posix_heredoc_strip_removes_only_leading_tabs",
            script: r#"cat <<-EOF
	leading-tab
	 	after-space-tab
  leading-spaces
	EOF
"#,
            expected_status: 0,
            expected_stdout: "leading-tab\n \tafter-space-tab\n  leading-spaces\n",
            expected_file: None,
        },
        Case {
            name: "posix_read_ifs_and_backslash_quoting",
            script: r#"IFS=:
read FIRST REST <<EOF
one\:two:three
EOF
printf '<%s><%s>\n' "$FIRST" "$REST"
"#,
            expected_status: 0,
            expected_stdout: "<one:two><three>\n",
            expected_file: None,
        },
        Case {
            name: "posix_read_empty_delimiter_reads_through_nul",
            script: "printf 'alpha\\000omega\\n' >input\n{ read -r -d '' FIRST; STATUS=$?; read -r SECOND; printf '%s:<%s><%s>\\n' \"$STATUS\" \"$FIRST\" \"$SECOND\"; } <input\n",
            expected_status: 0,
            expected_stdout: "0:<alpha><omega>\n",
            expected_file: None,
        },
        Case {
            name: "posix_read_uses_multibyte_ifs_characters",
            script: r#"IFS='§'
read FIRST SECOND REST <<EOF
alpha§beta§gamma
EOF
printf '<%s><%s><%s>\n' "$FIRST" "$SECOND" "$REST"
"#,
            expected_status: 0,
            expected_stdout: "<alpha><beta><gamma>\n",
            expected_file: None,
        },
        Case {
            name: "posix_read_ifs_delimiter_escape_backslash_parity",
            script: r#"IFS=:
read FIRST REST <<'EOF'
one\:two:tail
EOF
printf 'one=<%s><%s>\n' "$FIRST" "$REST"
read FIRST REST <<'EOF'
two\\:tail
EOF
printf 'two=<%s><%s>\n' "$FIRST" "$REST"
read FIRST REST <<'EOF'
three\\\:four:tail
EOF
printf 'three=<%s><%s>\n' "$FIRST" "$REST"
"#,
            expected_status: 0,
            expected_stdout: "one=<one:two><tail>\ntwo=<two\\><tail>\nthree=<three\\:four><tail>\n",
            expected_file: None,
        },
        Case {
            name: "posix_read_ifs_whitespace_escape_backslash_parity",
            script: r#"IFS=' '
read FIRST REST <<'EOF'
one\ two three
EOF
printf 'one=<%s><%s>\n' "$FIRST" "$REST"
read FIRST REST <<'EOF'
two\\ three four
EOF
printf 'two=<%s><%s>\n' "$FIRST" "$REST"
read FIRST REST <<'EOF'
three\\\ four five
EOF
printf 'three=<%s><%s>\n' "$FIRST" "$REST"
"#,
            expected_status: 0,
            expected_stdout: "one=<one two><three>\ntwo=<two\\><three four>\nthree=<three\\ four><five>\n",
            expected_file: None,
        },
        Case {
            name: "posix_read_collapses_ifs_whitespace",
            script: "read FIRST SECOND THIRD <<EOF\n  one \t two   three  \nEOF\nprintf '<%s><%s><%s>\\n' \"$FIRST\" \"$SECOND\" \"$THIRD\"\n",
            expected_status: 0,
            expected_stdout: "<one><two><three>\n",
            expected_file: None,
        },
        Case {
            name: "posix_read_preserves_empty_nonwhitespace_ifs_fields",
            script: "IFS=:\nread FIRST SECOND REST <<EOF\n:a::b:\nEOF\nprintf '<%s><%s><%s>\\n' \"$FIRST\" \"$SECOND\" \"$REST\"\n",
            expected_status: 0,
            expected_stdout: "<><a><:b:>\n",
            expected_file: None,
        },
        Case {
            name: "posix_read_single_variable_receives_unsplit_remainder",
            script: "IFS=:\nread VALUE <<EOF\nalpha\\:beta:gamma:\nEOF\nprintf '<%s>\\n' \"$VALUE\"\n",
            expected_status: 0,
            expected_stdout: "<alpha:beta:gamma:>\n",
            expected_file: None,
        },
        Case {
            name: "posix_read_continuation_at_eof_assigns_partial_value_and_fails",
            script: r#"printf 'partial\\\n' >input
read VALUE <input
STATUS=$?
printf '%s:<%s>\n' "$STATUS" "$VALUE"
"#,
            expected_status: 0,
            expected_stdout: "1:<partial>\n",
            expected_file: None,
        },
        Case {
            name: "posix_read_combines_ifs_whitespace_and_nonwhitespace_delimiters",
            script: r#"IFS=' :'
read FIRST SECOND THIRD FOURTH <<EOF
  alpha : beta::gamma  
EOF
printf '<%s><%s><%s><%s>\n' "$FIRST" "$SECOND" "$THIRD" "$FOURTH"
"#,
            expected_status: 0,
            expected_stdout: "<alpha><beta><><gamma>\n",
            expected_file: None,
        },
        Case {
            name: "posix_read_raw_mode_preserves_backslash",
            script: r#"read -r VALUE <<EOF
one\ two
EOF
printf '<%s>\n' "$VALUE"
"#,
            expected_status: 0,
            expected_stdout: "<one\\ two>\n",
            expected_file: None,
        },
        Case {
            name: "posix_read_default_backslash_quotes_ordinary_character",
            script: r#"read DEFAULT <<EOF
left\q
EOF
read -r RAW <<EOF
left\q
EOF
printf '<%s><%s>\n' "$DEFAULT" "$RAW"
"#,
            expected_status: 0,
            expected_stdout: "<leftq><left\\q>\n",
            expected_file: None,
        },
        Case {
            name: "posix_read_raw_mode_does_not_join_backslash_newline",
            script: r#"printf 'left\\\nright\n' >input
{
    read -r FIRST
    read -r SECOND
} <input
printf '<%s><%s>\n' "$FIRST" "$SECOND"
"#,
            expected_status: 0,
            expected_stdout: "<left\\><right>\n",
            expected_file: None,
        },
        Case {
            name: "posix_read_empty_ifs_preserves_whitespace_and_joins_continuation",
            script: r#"IFS=
read VALUE <<EOF
  leading and trailing  
EOF
printf '<%s>\n' "$VALUE"
IFS=' '
read VALUE <<EOF
left\
right
EOF
printf '<%s>\n' "$VALUE"
IFS=
read VALUE <<EOF
escaped\ space
EOF
printf 'protected=<%s>\n' "$VALUE"
read -r RAW <<EOF
literal\ space
EOF
printf 'raw=<%s>\n' "$RAW"
"#,
            expected_status: 0,
            expected_stdout: "<  leading and trailing  >\n<leftright>\nprotected=<escaped space>\nraw=<literal\\ space>\n",
            expected_file: None,
        },
        Case {
            name: "posix_read_unset_ifs_uses_default_whitespace",
            script: r#"unset IFS
read FIRST SECOND <<EOF
  one 	 two  
EOF
printf '<%s><%s>\n' "$FIRST" "$SECOND"
"#,
            expected_status: 0,
            expected_stdout: "<one><two>\n",
            expected_file: None,
        },
        Case {
            name: "posix_sort_field_key_with_numeric_modifier",
            script: "printf 'x:10\\nx:2\\ny:1\\n' | sort -t: -k2,2n\n",
            expected_status: 0,
            expected_stdout: "y:1\nx:2\nx:10\n",
            expected_file: None,
        },
        Case {
            name: "posix_sort_distinguishes_omitted_end_from_end_of_field",
            script: "printf 'a x1 z\\nb x1 a\\n' | sort -k2,2; printf 'a x1 z\\nb x1 a\\n' | sort -k2\nprintf 'a x12 z\\nb x13 a\\n' | sort -k2.2,2.0\n",
            expected_status: 0,
            expected_stdout: "a x1 z\nb x1 a\nb x1 a\na x1 z\na x12 z\nb x13 a\n",
            expected_file: None,
        },
        Case {
            name: "posix_sort_check_mode_checks_order_and_unique_keys",
            script: "LC_ALL=C; export LC_ALL\nprintf 'a\\nb\\n' | sort -c\nprintf 'sorted=%s\\n' \"$?\"\nprintf 'b\\na\\n' | sort -c 2>/dev/null\nprintf 'unsorted=%s\\n' \"$?\"\nprintf 'b\\na\\n' | sort -C\nprintf 'quiet-unsorted=%s\\n' \"$?\"\nprintf 'a\\nA\\n' | sort -Cfu\nprintf 'quiet-duplicate=%s\\n' \"$?\"\nprintf 'a\\nA\\n' | sort -cfu 2>/dev/null\nprintf 'duplicate=%s\\n' \"$?\"\n",
            expected_status: 0,
            expected_stdout: "sorted=0\nunsorted=1\nquiet-unsorted=1\nquiet-duplicate=1\nduplicate=1\n",
            expected_file: None,
        },
        Case {
            name: "posix_sort_output_file",
            script: "printf 'pear\\napple\\n' >input.txt\nsort -o result.txt input.txt\ncat result.txt\n",
            expected_status: 0,
            expected_stdout: "apple\npear\n",
            expected_file: Some("apple\npear\n"),
        },
        Case {
            name: "posix_sort_merge_presorted_files",
            script: "printf 'b:1\\nd:2\\n' >left.txt\nprintf 'a:1\\nc:2\\n' >right.txt\nsort -m -t: -k2,2n left.txt right.txt\n",
            expected_status: 0,
            expected_stdout: "a:1\nb:1\nc:2\nd:2\n",
            expected_file: None,
        },
        Case {
            name: "posix_sort_stable_preserves_equal_key_input_order",
            script: "printf 'a\\nA\\n' | sort -sf; printf 'z:1\\na:1\\n' | sort -s -t: -k2,2\n",
            expected_status: 0,
            expected_stdout: "a\nA\nz:1\na:1\n",
            expected_file: None,
        },
        Case {
            name: "posix_sort_default_separator_is_part_of_the_following_field",
            script: "LC_ALL=C; export LC_ALL\nprintf 'y\\tb\\nx a\\n' | sort -k 2,2\n",
            expected_status: 0,
            expected_stdout: "y\tb\nx a\n",
            expected_file: None,
        },
        Case {
            name: "posix_uniq_count_repeated_and_unique_modes",
            script: "printf 'a\\na\\nb\\nc\\nc\\nc\\nd\\n' | uniq; printf 'a\\na\\nb\\nc\\nc\\nc\\nd\\n' | uniq -c | sed 's/^ *//'; printf 'a\\na\\nb\\nc\\nc\\nc\\nd\\n' | uniq -d; printf 'a\\na\\nb\\nc\\nc\\nc\\nd\\n' | uniq -u\n",
            expected_status: 0,
            expected_stdout: "a\nb\nc\nd\n2 a\n1 b\n3 c\n1 d\na\nc\nb\nd\n",
            expected_file: None,
        },
        Case {
            name: "posix_uniq_skip_one_field_and_filter_adjacent_matches",
            script: "uniq -f 1 <<'EOF'\n#01 foo0 bar0 foo1 bar1\n#02 bar0 foo1 bar1 foo1\n#03 foo0 bar0 foo1 bar1\n#04\n#05 foo0 bar0 foo1 bar1\n#06 foo0 bar0 foo1 bar1\n#07 bar0 foo1 bar1 foo0\nEOF\n",
            expected_status: 0,
            expected_stdout: "#01 foo0 bar0 foo1 bar1\n#02 bar0 foo1 bar1 foo1\n#03 foo0 bar0 foo1 bar1\n#04\n#05 foo0 bar0 foo1 bar1\n#07 bar0 foo1 bar1 foo0\n",
            expected_file: None,
        },
        Case {
            name: "posix_uniq_skips_fields_before_characters",
            script: "printf '1 axx\\n2 bxx\\n3 cxy\\n' | uniq -f 1 -s 2\n",
            expected_status: 0,
            expected_stdout: "1 axx\n3 cxy\n",
            expected_file: None,
        },
        Case {
            name: "posix_head_and_tail_multi_file_headers",
            script: "printf 'a1\\na2\\n' >first; printf 'b1\\nb2\\n' >second; head -n 1 first second; tail -n 1 first second\n",
            expected_status: 0,
            expected_stdout: "==> first <==\na1\n\n==> second <==\nb1\n==> first <==\na2\n\n==> second <==\nb2\n",
            expected_file: None,
        },
        Case {
            name: "posix_head_line_count_preserves_unterminated_final_data",
            script: "printf 'one\\ntwo\\nthree' | head -n 3\n",
            expected_status: 0,
            expected_stdout: "one\ntwo\nthree",
            expected_file: None,
        },
        Case {
            name: "posix_tail_positive_line_offset",
            script: "printf 'one\\ntwo\\nthree' | tail -n +2\n",
            expected_status: 0,
            expected_stdout: "two\nthree",
            expected_file: None,
        },
        Case {
            name: "posix_tail_positive_byte_offset",
            script: "printf 'abcdef' | tail -c +3\n",
            expected_status: 0,
            expected_stdout: "cdef",
            expected_file: None,
        },
        Case {
            name: "posix_tail_negative_counts_and_zero",
            script: "printf 'one\\ntwo\\nthree\\n' | tail -n -2; printf 'abcdef' | tail -c -2; printf 'ignored\\n' | tail -n 0; printf 'ignored' | tail -c 0\n",
            expected_status: 0,
            expected_stdout: "two\nthree\nef",
            expected_file: None,
        },
        Case {
            name: "posix_sort_accepts_stdin_and_end_of_options",
            script: "printf 'zeta\\nalpha\\n' >-input; sort -- -input; printf 'delta\\nbeta\\n' | sort -\n",
            expected_status: 0,
            expected_stdout: "alpha\nzeta\nbeta\ndelta\n",
            expected_file: None,
        },
        Case {
            name: "posix_sort_global_blank_option_for_key_positions",
            script: "printf 'x: b\\nx:a\\n' | sort -b -t: -k2,2\n",
            expected_status: 0,
            expected_stdout: "x:a\nx: b\n",
            expected_file: None,
        },
        Case {
            name: "posix_sort_blank_modifier_is_scoped_to_its_endpoint",
            script: "printf 'x: aZ\\nx:bY\\n' | sort -t: -k2.2b,2.2\n",
            expected_status: 0,
            expected_stdout: "x: aZ\nx:bY\n",
            expected_file: None,
        },
        Case {
            name: "posix_sort_key_modifier_overrides_global_reverse",
            script: "printf '10:a\\n2:b\\n' | sort -r -t: -k1,1n\n",
            expected_status: 0,
            expected_stdout: "2:b\n10:a\n",
            expected_file: None,
        },
        Case {
            name: "posix_sort_reverse_applies_to_last_resort_comparison",
            script: "printf 'a:1\\na:2\\nb:1\\n' | sort -r -t: -k1,1\n",
            expected_status: 0,
            expected_stdout: "b:1\na:2\na:1\n",
            expected_file: None,
        },
        Case {
            name: "posix_date_utc_format_timezone",
            script: "date -u +%Z\n",
            expected_status: 0,
            expected_stdout: "UTC\n",
            expected_file: None,
        },
        Case {
            name: "posix_sleep_accepts_zero_seconds",
            script: "sleep 0; printf 'slept\\n'\n",
            expected_status: 0,
            expected_stdout: "slept\n",
            expected_file: None,
        },
        Case {
            name: "posix_env_isolates_child_environment_and_preserves_assignments",
            script: "PROBE=parent; env -i PATH=\"$PATH\" PROBE='two words' sh -c 'printf \"%s\\n\" \"$PROBE\"'; env -u PROBE sh -c 'if [ -z \"${PROBE+x}\" ]; then printf unset; fi'; printf '\\n%s\\n' \"$PROBE\"\n",
            expected_status: 0,
            expected_stdout: "two words\nunset\nparent\n",
            expected_file: None,
        },
        Case {
            name: "posix_wc_counts_newlines_and_keeps_count_order",
            script: "printf 'one\\r\\ntwo\\nlast' | wc -wl | tr -d ' '; printf 'one\\r\\ntwo\\nlast' | wc -l | tr -d ' '; printf '' | wc -l | tr -d ' '; printf '\\n' | wc -l | tr -d ' '\n",
            expected_status: 0,
            expected_stdout: "23\n2\n0\n1\n",
            expected_file: None,
        },
        Case {
            name: "posix_grep_multi_file_count_listing_and_quiet_status",
            script: "printf 'hit\\nhit\\n' >one; printf 'hit\\nmiss\\n' >two; printf 'miss\\n' >three; grep -c hit one two three; grep -l hit one two three; grep -lc hit one two three; grep -qs hit missing one; printf 'quiet-status=%s\\n' \"$?\"\n",
            expected_status: 0,
            expected_stdout: "one:2\ntwo:1\nthree:0\none\ntwo\none\ntwo\nquiet-status=0\n",
            expected_file: None,
        },
        Case {
            name: "posix_grep_line_numbers_reset_per_file_and_compose_with_invert",
            script: "printf 'alpha\\nbeta\\nalpha\\n' >one.txt; printf 'beta\\nalpha\\n' >two.txt; grep -n alpha one.txt; grep -n alpha one.txt two.txt; grep -vn alpha two.txt\n",
            expected_status: 0,
            expected_stdout: "1:alpha\n3:alpha\none.txt:1:alpha\none.txt:3:alpha\ntwo.txt:2:alpha\n1:beta\n",
            expected_file: None,
        },
        Case {
            name: "posix_grep_case_insensitive_clustered_with_line_numbers_and_invert",
            script: "printf 'Alpha\\nbeta\\nALPHA\\n' | grep -in alpha; printf 'Alpha\\nbeta\\nALPHA\\n' | grep -iv alpha\n",
            expected_status: 0,
            expected_stdout: "1:Alpha\n3:ALPHA\nbeta\n",
            expected_file: None,
        },
        Case {
            name: "posix_grep_count_reports_zero_for_empty_stdin",
            script: "grep -c needle\n",
            expected_status: 1,
            expected_stdout: "0\n",
            expected_file: None,
        },
        Case {
            name: "posix_grep_quiet_stops_after_first_match",
            script: "printf 'hit\\n' >result.txt\ngrep -q hit result.txt missing.txt\n",
            expected_status: 0,
            expected_stdout: "",
            expected_file: Some("hit\n"),
        },
        Case {
            name: "posix_grep_basic_and_extended_regular_expression_modes",
            script: "printf 'a+\\naa\\nab\\nabab\\n' | grep 'a+'; printf 'abab\\naa\\nab\\n' | grep '\\(ab\\)\\{2\\}'; printf 'aa\\nab\\n' | grep -E 'a+'\n",
            expected_status: 0,
            expected_stdout: "a+\nabab\naa\nab\n",
            expected_file: None,
        },
        Case {
            name: "posix_grep_bracket_expression_closing_member_and_named_class",
            script: "printf ']\\na\\nb\\n1\\n' | grep '[]a]'; printf 'a\\n1\\n' | grep '[[:digit:]]'\n",
            expected_status: 0,
            expected_stdout: "]\na\n1\n",
            expected_file: None,
        },
        Case {
            name: "posix_grep_pattern_files_and_null_patterns",
            script: "printf 'alpha\\nbeta\\ngamma\\n' >input\nprintf 'alpha\\nbeta\\n' >patterns\nprintf '\\n' >null-pattern\ngrep -F -f patterns input\ngrep -F -f null-pattern input\n",
            expected_status: 0,
            expected_stdout: "alpha\nbeta\nalpha\nbeta\ngamma\n",
            expected_file: None,
        },
        Case {
            name: "posix_grep_pattern_list_empty_member_selects_every_line",
            script: "printf 'alpha\\nbeta\\ngamma\\n' >input\ngrep -F -e 'alpha\n\nbeta' input\n",
            expected_status: 0,
            expected_stdout: "alpha\nbeta\ngamma\n",
            expected_file: None,
        },
        Case {
            name: "posix_cut_grouped_field_and_suppression_options",
            script: "printf 'a\\tone\\na\\ttwo\\nb\\tthree\\n' | cut -sf1\n",
            expected_status: 0,
            expected_stdout: "a\na\nb\n",
            expected_file: None,
        },
        Case {
            name: "posix_cut_byte_and_character_ranges",
            script: "printf 'abcdef\\n' | cut -b2-3; printf 'abcdef\\n' | cut -c1,3-4\n",
            expected_status: 0,
            expected_stdout: "bc\nacd\n",
            expected_file: None,
        },
        Case {
            name: "posix_cut_blank_separated_selection_list",
            script: "printf 'abcdef\\n' | cut -c '1 3-4'\n",
            expected_status: 0,
            expected_stdout: "acd\n",
            expected_file: None,
        },
        Case {
            name: "posix_cut_non_splitting_option_on_ascii",
            script: "printf 'abcdef\\n' | cut -b1-3 -n\n",
            expected_status: 0,
            expected_stdout: "abc\n",
            expected_file: None,
        },
        Case {
            name: "posix_cut_field_order_and_trailing_empty_field",
            script: "printf 'a::c:\\nplain\\n' | cut -d: -f4,1,3\n",
            expected_status: 0,
            expected_stdout: "a:c:\nplain\n",
            expected_file: None,
        },
        Case {
            name: "posix_cut_undelimited_lines_and_empty_selected_fields",
            script: "printf 'a::c\\nplain\\na::c:\\n' | cut -d: -f2,4; printf 'a::c\\nplain\\na::c:\\n' | cut -s -d: -f2,4\n",
            expected_status: 0,
            expected_stdout: "\nplain\n:\n\n:\n",
            expected_file: None,
        },
        Case {
            name: "posix_cut_continues_after_an_unreadable_input_file",
            script: "printf 'first:second\\n' >readable.txt\ncut -d: -f2 missing.txt readable.txt 2>/dev/null\n",
            expected_status: 1,
            expected_stdout: "second\n",
            expected_file: None,
        },
        Case {
            name: "posix_mkdir_double_dash_allows_leading_hyphen_path",
            script: "mkdir -- -leading\nif test -d ./-leading; then printf 'created\\n'; fi\n",
            expected_status: 0,
            expected_stdout: "created\n",
            expected_file: None,
        },
        Case {
            name: "posix_mkdir_continues_after_operand_error",
            script: ": >occupied\nmkdir occupied created 2>/dev/null\nresult=$?\nif test \"$result\" -eq 1 && test -d created; then printf 'continued\\n'; fi\n",
            expected_status: 0,
            expected_stdout: "continued\n",
            expected_file: None,
        },
        Case {
            name: "posix_mkdir_parents_continues_after_operand_error",
            script: ": >occupied\nmkdir -p occupied created nested/child 2>/dev/null\nresult=$?\nif test \"$result\" -eq 1 && test -d created && test -d nested/child; then printf 'parents-continued\\n'; fi\n",
            expected_status: 0,
            expected_stdout: "parents-continued\n",
            expected_file: None,
        },
        Case {
            name: "posix_mkdir_numeric_mode_sets_owner_permissions",
            script: "mkdir -m 700 private\nif test -r private && test -w private && test -x private; then printf 'created-private\\n'; fi\n",
            expected_status: 0,
            expected_stdout: "created-private\n",
            expected_file: None,
        },
        Case {
            name: "posix_mkdir_symbolic_x_depends_on_directory_type",
            script: "mkdir -m a=,u=rw,a+X searchable\nmkdir -m a=,u=rw noexecute\nif test -x searchable && test ! -x noexecute; then printf 'symbolic-X-ok\\n'; fi\n",
            expected_status: 0,
            expected_stdout: "symbolic-X-ok\n",
            expected_file: None,
        },
        Case {
            name: "posix_rm_continues_after_missing_operand",
            script: "printf x >later\nrm missing later 2>/dev/null\nresult=$?\nif test \"$result\" -eq 1 && test ! -e later; then printf 'continued\\n'; fi\n",
            expected_status: 0,
            expected_stdout: "continued\n",
            expected_file: None,
        },
        Case {
            name: "posix_touch_preserves_contents_and_honors_no_create",
            script: "printf 'keep me\\n' >existing\ntouch -m -- existing\ntouch -c absent\ncat existing\nif test ! -e absent; then printf 'not-created\\n'; fi\n",
            expected_status: 0,
            expected_stdout: "keep me\nnot-created\n",
            expected_file: None,
        },
        Case {
            name: "posix_touch_uses_reference_timestamps_and_creates_targets",
            script: "printf reference >reference\ntouch -r reference created\ntouch -amr reference access-modified\nif test -e created && test -e access-modified; then printf 'created\\n'; fi\n",
            expected_status: 0,
            expected_stdout: "created\n",
            expected_file: None,
        },
        Case {
            name: "posix_touch_numeric_time_option_and_grouped_flags",
            script: "printf content >target\ntouch -at202001020304.05 target\ntouch -cm target absent\nif test -e target && test ! -e absent; then printf 'touch-time-ok\\n'; fi\n",
            expected_status: 0,
            expected_stdout: "touch-time-ok\n",
            expected_file: None,
        },
        Case {
            name: "posix_touch_iso_date_time_option",
            script: "touch -d '2020-01-02T03:04:05.123Z' target\nif test -e target; then printf 'touch-date-ok\\n'; fi\n",
            expected_status: 0,
            expected_stdout: "touch-date-ok\n",
            expected_file: None,
        },
        Case {
            name: "posix_mv_moves_multiple_sources_into_directory",
            script: "mkdir destination\nprintf one >first\nprintf two >second\nmv -- first second destination\ncat destination/first destination/second\n",
            expected_status: 0,
            expected_stdout: "onetwo",
            expected_file: None,
        },
        Case {
            name: "posix_mv_continues_after_missing_source",
            script: "mkdir destination\nprintf later >later\nmv missing later destination 2>/dev/null\nresult=$?\nif test \"$result\" -eq 1 && test -f destination/later; then cat destination/later; fi\n",
            expected_status: 0,
            expected_stdout: "later",
            expected_file: None,
        },
        Case {
            name: "posix_cp_copies_multiple_sources_into_directory",
            script: "mkdir destination\nprintf one >first\nprintf two >second\ncp -- first second destination\ncat destination/first destination/second\n",
            expected_status: 0,
            expected_stdout: "onetwo",
            expected_file: None,
        },
        Case {
            name: "posix_cp_continues_after_missing_source",
            script: "mkdir destination\nprintf later >later\ncp missing later destination 2>/dev/null\nresult=$?\nif test \"$result\" -eq 1 && test -f destination/later; then cat destination/later; fi\n",
            expected_status: 0,
            expected_stdout: "later",
            expected_file: None,
        },
        Case {
            name: "posix_cp_force_option_copies_file",
            script: "mkdir destination\nprintf one >first\ncp -f -- first destination/copied\ncat destination/copied\n",
            expected_status: 0,
            expected_stdout: "one",
            expected_file: None,
        },
        Case {
            name: "posix_tr_class_and_escaped_hyphen_do_not_form_ranges",
            script: "printf '9:-a' | tr '[:digit:]-a' X; printf '%s' '-az' | tr 'a\\-z' X\n",
            expected_status: 0,
            expected_stdout: "X:XXXXX",
            expected_file: None,
        },
        Case {
            name: "posix_tr_leading_and_trailing_hyphens_are_literal",
            script: "printf '%s' '-a' | tr -- '-a' XY; printf '%s' 'a-' | tr -- 'a-' XY; printf ab | tr -- ab '-X'\n",
            expected_status: 0,
            expected_stdout: "XYXY-X",
            expected_file: None,
        },
        Case {
            name: "posix_tr_repetition_sets_support_complement_squeezing",
            script: "printf 'abc xyz!\\n' | tr -cs '[:alpha:]' '[\\n*]'; printf ab | tr ab '[x*2]'\n",
            expected_status: 0,
            expected_stdout: "abc\nxyz\nxx",
            expected_file: None,
        },
        Case {
            name: "posix_cp_preserve_accepts_grouped_recursive_option",
            script: "mkdir source destination\nprintf data >source/file\ncp -Rp source destination\ncat destination/source/file\n",
            expected_status: 0,
            expected_stdout: "data",
            expected_file: None,
        },
        Case {
            name: "posix_cp_H_and_L_select_symlink_traversal_modes",
            script: "mkdir source\nprintf data >source/file\nln -s file source/link\nln -s source top\ncp -RH top copied-h\ncp -RL top copied-l\nif test -L copied-h/link && test ! -L copied-l/link; then printf 'link-modes-ok\\n'; fi\n",
            expected_status: 0,
            expected_stdout: "link-modes-ok\n",
            expected_file: None,
        },
    ];

    let reference_shells = [("bash", vec!["--posix", "-c"])]
        .into_iter()
        .filter(|(shell, args)| {
            Command::new(shell)
                .args(args)
                .arg(":")
                .output()
                .is_ok_and(|output| output.status.success())
        })
        .collect::<Vec<_>>();
    if reference_shells.is_empty() {
        eprintln!("no POSIX reference shell is available; running ESH expectations only");
    }

    let engine = Engine::new();
    for case in cases {
        let esh_dir = tempdir().unwrap();
        let mut ctx = ExecutionContext::new();
        ctx.enable_capture();
        ctx.set_cwd(esh_dir.path()).unwrap();
        let esh_status = engine
            .eval_str(case.script, &mut ctx)
            .await
            .unwrap_or_else(|error| panic!("ESH case {} failed: {error}", case.name));
        let esh_stdout = ctx.get_captured_stdout().unwrap();

        assert_eq!(
            esh_status, case.expected_status,
            "ESH status: {}",
            case.name
        );
        assert_eq!(
            esh_stdout, case.expected_stdout,
            "ESH output: {}",
            case.name
        );
        assert_eq!(
            ctx.get_captured_stderr().unwrap_or_default(),
            "",
            "ESH stderr: {}",
            case.name
        );
        if let Some(expected) = case.expected_file {
            assert_eq!(
                std::fs::read_to_string(esh_dir.path().join("result.txt")).unwrap(),
                expected,
                "ESH filesystem result: {}",
                case.name
            );
        }

        for (shell, args) in &reference_shells {
            // macOS ships Bash 3.2, whose behavior differs from the behavior
            // these ESH cases test for CDPATH, here-documents, assignment-only
            // status, NaN formatting flags, and special-parameter defaults.
            if cfg!(target_os = "macos")
                && *shell == "bash"
                && matches!(
                    case.name,
                    "posix_cdpath_falls_back_and_ignores_explicit_relative_paths"
                        | "posix_heredoc_expands_positional_lists_without_removing_body_quotes"
                        | "posix_assignment_only_command_status"
                        | "posix_printf_float_sign_flags_apply_to_nan"
                        | "posix_printf_zero_padding_is_ignored_for_infinity_and_nan"
                        | "posix_special_parameter_default_operators_preserve_at_fields"
                )
            {
                continue;
            }
            let shell_dir = tempdir().unwrap();
            let reference = Command::new(shell)
                .args(args)
                .arg(case.script)
                .current_dir(shell_dir.path())
                .output()
                .unwrap_or_else(|error| panic!("could not run {shell}: {error}"));
            let reference_stdout = String::from_utf8_lossy(&reference.stdout);
            assert_eq!(
                reference.status.code(),
                Some(case.expected_status),
                "{shell} status: {}",
                case.name
            );
            assert_eq!(
                reference_stdout, case.expected_stdout,
                "{shell} output: {}",
                case.name
            );
            assert_eq!(
                String::from_utf8_lossy(&reference.stderr),
                "",
                "{shell} stderr: {}",
                case.name
            );
            assert_eq!(
                esh_stdout, reference_stdout,
                "ESH vs {shell} output: {}",
                case.name
            );

            if let Some(expected) = case.expected_file {
                assert_eq!(
                    std::fs::read_to_string(shell_dir.path().join("result.txt")).unwrap(),
                    expected,
                    "{shell} filesystem result: {}",
                    case.name
                );
            }
        }
    }
}

#[cfg(unix)]
#[tokio::test]
async fn pwd_logical_and_physical_symlink_modes_match_posix_shells() {
    let esh_dir = tempdir().unwrap();
    let esh_root = std::fs::canonicalize(esh_dir.path()).unwrap();
    std::fs::create_dir(esh_dir.path().join("real")).unwrap();
    std::os::unix::fs::symlink(esh_dir.path().join("real"), esh_dir.path().join("alias")).unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(esh_dir.path()).unwrap();
    let esh_script = "START=$(pwd)\ncd real\nPWD=\"$START/alias\"\npwd -L\npwd -P\n";
    assert_eq!(engine.eval_str(esh_script, &mut ctx).await.unwrap(), 0);
    let esh_stdout = ctx.get_captured_stdout().unwrap();
    assert_eq!(
        esh_stdout,
        format!(
            "{}/alias\n{}/real\n",
            esh_root.display(),
            esh_root.display()
        )
    );

    let reference_script = "cd alias\npwd -L\npwd -P\n";
    for (shell, args) in [("bash", vec!["--posix", "-c"]), ("dash", vec!["-c"])] {
        let shell_dir = tempdir().unwrap();
        let shell_root = std::fs::canonicalize(shell_dir.path()).unwrap();
        std::fs::create_dir(shell_dir.path().join("real")).unwrap();
        std::os::unix::fs::symlink(
            shell_dir.path().join("real"),
            shell_dir.path().join("alias"),
        )
        .unwrap();
        let output = match Command::new(shell)
            .args(&args)
            .arg(reference_script)
            .current_dir(shell_dir.path())
            .output()
        {
            Ok(output) => output,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => panic!("could not run {shell}: {error}"),
        };
        assert_eq!(output.status.code(), Some(0), "{shell} status");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            format!(
                "{}/alias\n{}/real\n",
                shell_root.display(),
                shell_root.display()
            ),
            "{shell} output"
        );
        assert_eq!(output.stderr, b"", "{shell} stderr");
    }
}

#[cfg(unix)]
#[tokio::test]
async fn cd_logical_and_physical_symlink_modes_match_posix_shells() {
    let script = r#"START=$(pwd -P)
cd -L alias/sub
printf 'logical:%s\n' "$PWD"
cd -L ..
printf 'logical-parent:%s\n' "$PWD"
cd -P "$START/alias/sub"
printf 'physical:%s\n' "$PWD"
cd -PL "$START/alias/sub"
printf 'last-logical:%s\n' "$PWD"
cd -LP "$START/alias/sub"
printf 'last-physical:%s\n' "$PWD"
"#;

    let esh_dir = tempdir().unwrap();
    let root = std::fs::canonicalize(esh_dir.path()).unwrap();
    std::fs::create_dir_all(esh_dir.path().join("real/sub")).unwrap();
    std::os::unix::fs::symlink(esh_dir.path().join("real"), esh_dir.path().join("alias")).unwrap();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(esh_dir.path()).unwrap();
    assert_eq!(Engine::new().eval_str(script, &mut ctx).await.unwrap(), 0);
    let root = root.display().to_string();
    let expected = format!(
        "logical:{root}/alias/sub\nlogical-parent:{root}/alias\nphysical:{root}/real/sub\nlast-logical:{root}/alias/sub\nlast-physical:{root}/real/sub\n"
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), expected);

    for (shell, args) in [("bash", vec!["--posix", "-c"]), ("dash", vec!["-c"])] {
        let shell_dir = tempdir().unwrap();
        let shell_root = std::fs::canonicalize(shell_dir.path()).unwrap();
        std::fs::create_dir_all(shell_dir.path().join("real/sub")).unwrap();
        std::os::unix::fs::symlink(
            shell_dir.path().join("real"),
            shell_dir.path().join("alias"),
        )
        .unwrap();
        let output = match Command::new(shell)
            .args(&args)
            .arg(script)
            .current_dir(shell_dir.path())
            .output()
        {
            Ok(output) => output,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => panic!("could not run {shell}: {error}"),
        };
        assert_eq!(output.status.code(), Some(0), "{shell} status");
        let shell_root = shell_root.display().to_string();
        let shell_expected = format!(
            "logical:{shell_root}/alias/sub\nlogical-parent:{shell_root}/alias\nphysical:{shell_root}/real/sub\nlast-logical:{shell_root}/alias/sub\nlast-physical:{shell_root}/real/sub\n"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            shell_expected,
            "{shell}"
        );
    }
}

#[cfg(unix)]
#[tokio::test]
async fn explicit_glob_hidden_path_behavior_matches_posix_pathname_expansion() {
    let esh_dir = tempdir().unwrap();
    let esh_hidden_dir = esh_dir.path().join(".hidden-dir");
    let esh_visible_dir = esh_dir.path().join("visible-dir");
    std::fs::create_dir_all(&esh_hidden_dir).unwrap();
    std::fs::create_dir_all(&esh_visible_dir).unwrap();
    for path in [
        esh_dir.path().join(".hidden.txt"),
        esh_dir.path().join("visible.txt"),
        esh_hidden_dir.join(".child.txt"),
        esh_visible_dir.join("child.txt"),
    ] {
        std::fs::write(path, "").unwrap();
    }

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(esh_dir.path()).unwrap();
    let status = engine
        .eval_str(
            "glob '*'\nglob '*/child.txt'\nglob '.hidden-dir/.child.txt'\n",
            &mut ctx,
        )
        .await
        .unwrap();
    assert_eq!(status, 0);
    let esh_stdout = ctx.get_captured_stdout().unwrap();
    assert_eq!(
        esh_stdout,
        "visible-dir\nvisible.txt\nvisible-dir/child.txt\n.hidden-dir/.child.txt\n"
    );

    let reference_script =
        "printf '%s\\n' *; printf '%s\\n' */child.txt; printf '%s\\n' .hidden-dir/.child.txt";
    for (shell, args) in [("bash", vec!["--posix", "-c"]), ("dash", vec!["-c"])] {
        let shell_dir = tempdir().unwrap();
        let hidden_dir = shell_dir.path().join(".hidden-dir");
        let visible_dir = shell_dir.path().join("visible-dir");
        std::fs::create_dir_all(&hidden_dir).unwrap();
        std::fs::create_dir_all(&visible_dir).unwrap();
        for path in [
            shell_dir.path().join(".hidden.txt"),
            shell_dir.path().join("visible.txt"),
            hidden_dir.join(".child.txt"),
            visible_dir.join("child.txt"),
        ] {
            std::fs::write(path, "").unwrap();
        }

        let output = match Command::new(shell)
            .args(&args)
            .arg(reference_script)
            .current_dir(shell_dir.path())
            .env("LC_ALL", "C")
            .output()
        {
            Ok(output) => output,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => panic!("could not run {shell}: {error}"),
        };
        assert_eq!(output.status.code(), Some(0), "{shell} status");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            esh_stdout,
            "{shell} output"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stderr),
            "",
            "{shell} stderr"
        );
    }
}

#[cfg(unix)]
#[tokio::test]
async fn read_escaped_trailing_ifs_whitespace_matches_dash_when_available() {
    let script = "IFS=' '\nread VALUE <<'DATA'\n  leading\\ and trailing\\  \nDATA\nprintf '<%s>\\n' \"$VALUE\"\n";
    let reference = match Command::new("dash").arg("-c").arg(script).output() {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("dash is unavailable; skipping independent read comparison");
            return;
        }
        Err(error) => panic!("could not run dash: {error}"),
    };

    assert_eq!(reference.status.code(), Some(0));

    let engine = Engine::new();
    let temp = tempdir().unwrap();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(temp.path()).unwrap();
    let status = engine.eval_str(script, &mut ctx).await.unwrap();
    let esh_stdout = ctx.get_captured_stdout().unwrap();
    let dash_stdout = String::from_utf8(reference.stdout).unwrap();

    assert_eq!(status, 0);
    assert_eq!(esh_stdout, "<leading and trailing >\n");
    assert_eq!(esh_stdout, dash_stdout);
}

#[cfg(unix)]
#[tokio::test]
async fn printf_large_octal_format_escapes_match_bash_posix_bytes() {
    let script = r"printf '\400|\445s|\777'";
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    assert_eq!(engine.eval_str(script, &mut ctx).await.unwrap(), 0);
    let esh_bytes = ctx.get_captured_stdout_bytes().unwrap();
    assert_eq!(esh_bytes, [0x00, b'|', b'%', b's', b'|', 0xff]);

    let output = match Command::new("bash")
        .args(["--posix", "-c", script])
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("bash is unavailable; skipping printf octal byte comparison");
            return;
        }
        Err(error) => panic!("could not run bash: {error}"),
    };
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, esh_bytes);
    assert_eq!(output.stderr, b"");
}

#[cfg(unix)]
#[tokio::test]
async fn printf_numeric_whitespace_conversion_uses_the_numeric_prefix() {
    let script = "printf '<%d><%f>\\n' ' 12' ' 1.5'\nprintf '<%d><%f>\\n' '12 ' '1.5 '\n";
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    let status = engine.eval_str(script, &mut ctx).await.unwrap();
    let stdout = ctx.get_captured_stdout().unwrap();

    assert_eq!(status, 1);
    assert_eq!(stdout, "<12><1.500000>\n<12><1.500000>\n");
    assert!(!ctx.get_captured_stderr().unwrap().is_empty());
}

#[cfg(unix)]
#[test]
fn reserved_words_at_command_start_require_their_grammar_context() {
    let invalid_for_header = "for\nITEM; do :; done";
    assert!(
        esh::planner::compiler::compile_script(invalid_for_header).is_err(),
        "ESH accepted a newline between 'for' and its variable name"
    );
    for (shell, args) in [
        ("bash", vec!["--posix", "-n", "-c"]),
        ("dash", vec!["-n", "-c"]),
    ] {
        if let Ok(output) = Command::new(shell)
            .args(args.iter().copied())
            .arg(invalid_for_header)
            .output()
        {
            assert!(
                !output.status.success(),
                "{shell} accepted a newline between 'for' and its variable name"
            );
        }
    }

    for word in ["then", "elif", "else", "fi", "in", "do", "done", "esac"] {
        assert!(
            esh::planner::compiler::compile_script(word).is_err(),
            "ESH accepted reserved word {word:?} as a bare command"
        );

        for (shell, args) in [
            ("bash", vec!["--posix", "-n", "-c"]),
            ("dash", vec!["-n", "-c"]),
        ] {
            if Command::new(shell)
                .args(args.iter().copied())
                .arg(word)
                .output()
                .is_ok_and(|output| output.status.success())
            {
                panic!("{shell} accepted reserved word {word:?} as a bare command");
            }
        }
    }

    for source in ["X=value then", ">/dev/null then"] {
        esh::planner::compiler::compile_script(source)
            .expect("prefix assignments and redirections allow a reserved word as command name");
        for (shell, args) in [
            ("bash", vec!["--posix", "-n", "-c"]),
            ("dash", vec!["-n", "-c"]),
        ] {
            if let Ok(output) = Command::new(shell)
                .args(args.iter().copied())
                .arg(source)
                .output()
            {
                assert!(
                    output.status.success(),
                    "{shell} rejected command word after prefix: {source:?}"
                );
            }
        }
    }
}
