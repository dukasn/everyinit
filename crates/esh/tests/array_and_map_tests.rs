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

use esh::{Engine, ExecutionContext};
#[cfg(unix)]
use std::process::Command;

#[tokio::test]
async fn test_indexed_array_creation_and_access() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        PACKAGES=("evin-core" "evin-git" "evin-sh")
        
        echo "first: ${PACKAGES[0]}"
        echo "second: ${PACKAGES[1]}"
        echo "third: ${PACKAGES[2]}"
        echo "all: ${PACKAGES[@]}"
        echo "count: ${#PACKAGES[@]}"
        echo "len0: ${#PACKAGES[0]}"
    "#;

    let code = engine.eval_str(script, &mut ctx).await.unwrap();
    assert_eq!(code, 0);

    let output = ctx.get_captured_stdout().unwrap();
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines[0], "first: evin-core");
    assert_eq!(lines[1], "second: evin-git");
    assert_eq!(lines[2], "third: evin-sh");
    assert_eq!(lines[3], "all: evin-core evin-git evin-sh");
    assert_eq!(lines[4], "count: 3");
    assert_eq!(lines[5], "len0: 9");
}

#[tokio::test]
async fn test_indexed_array_mutation() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        ARR=("alpha" "beta")
        ARR[1]="updated_beta"
        ARR[2]="gamma"
        
        echo "elem1: ${ARR[1]}"
        echo "elem2: ${ARR[2]}"
        echo "all: ${ARR[@]}"
        echo "count: ${#ARR[@]}"
    "#;

    let code = engine.eval_str(script, &mut ctx).await.unwrap();
    assert_eq!(code, 0);

    let output = ctx.get_captured_stdout().unwrap();
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines[0], "elem1: updated_beta");
    assert_eq!(lines[1], "elem2: gamma");
    assert_eq!(lines[2], "all: alpha updated_beta gamma");
    assert_eq!(lines[3], "count: 3");
}

#[tokio::test]
async fn indexed_array_literals_use_whitespace_separators_not_command_semicolons() {
    let engine = Engine::new();

    for script in ["ITEMS=(; first)", "ITEMS=(first; second)"] {
        let mut ctx = ExecutionContext::new();
        assert!(
            engine.eval_str(script, &mut ctx).await.is_err(),
            "accepted a command semicolon in array literal {script:?}"
        );
    }

    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    let status = engine
        .eval_str(
            "ITEMS=(first\nsecond)\nprintf '<%s><%s>\\n' \"${ITEMS[@]}\"",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "<first><second>\n");
}

#[tokio::test]
async fn test_associative_map_creation_and_access() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        declare -A TARGETS
        TARGETS[linux]="x86_64-unknown-linux-musl"
        TARGETS[macos]="aarch64-apple-darwin"
        TARGETS[windows]="x86_64-pc-windows-msvc"
        
        echo "linux: ${TARGETS[linux]}"
        echo "macos: ${TARGETS[macos]}"
        echo "windows: ${TARGETS[windows]}"
        OS_NAME="macos"
        echo "dynamic: ${TARGETS[$OS_NAME]}"
        echo "keys: ${!TARGETS[@]}"
        echo "count: ${#TARGETS[@]}"
    "#;

    let code = engine.eval_str(script, &mut ctx).await.unwrap();
    assert_eq!(code, 0);

    let output = ctx.get_captured_stdout().unwrap();
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines[0], "linux: x86_64-unknown-linux-musl");
    assert_eq!(lines[1], "macos: aarch64-apple-darwin");
    assert_eq!(lines[2], "windows: x86_64-pc-windows-msvc");
    assert_eq!(lines[3], "dynamic: aarch64-apple-darwin");
    assert_eq!(lines[4], "keys: linux macos windows");
    assert_eq!(lines[5], "count: 3");
}

#[tokio::test]
async fn associative_maps_keep_expanded_keys_with_spaces_as_one_key() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                declare -A MAP
                KEY='two words'
                QUOTED_KEY='quoted key'
                MAP[$KEY]='space value'
                MAP["quoted key"]='quoted value'
                printf 'values=<%s>|<%s>|<%s> count=<%s>\n' "${MAP[$KEY]}" "${MAP[$QUOTED_KEY]}" "${MAP["quoted key"]}" "${#MAP[@]}"
                printf 'key=<%s>\n' "${!MAP[@]}"
                unset 'MAP[two words]'
                printf 'remaining=<%s> count=<%s>\n' "${!MAP[@]}" "${#MAP[@]}"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "values=<space value>|<quoted value>|<quoted value> count=<2>\nkey=<quoted key>\nkey=<two words>\nremaining=<quoted key> count=<1>\n"
    );

    #[cfg(unix)]
    if bash_supports_associative_arrays() {
        let reference_script = r#"
            declare -A MAP
            KEY='two words'
            MAP[$KEY]='space value'
            MAP["quoted key"]='quoted value'
            printf 'values=<%s>|<%s> count=<%s>\n' "${MAP[$KEY]}" "${MAP["quoted key"]}" "${#MAP[@]}"
            unset 'MAP[two words]'
            printf 'remaining=<%s> count=<%s>\n' "${MAP["quoted key"]}" "${#MAP[@]}"
        "#;
        let reference = Command::new("bash")
            .args(["--posix", "-c", reference_script])
            .output()
            .expect("Bash should be available for the associative-map comparison");
        assert!(reference.status.success());
        assert_eq!(
            reference.stdout,
            b"values=<space value>|<quoted value> count=<2>\nremaining=<quoted value> count=<1>\n"
        );
        assert_eq!(reference.stderr, b"");
    }
}

#[cfg(unix)]
fn bash_supports_associative_arrays() -> bool {
    Command::new("bash")
        .args(["--posix", "-c", "declare -A MAP"])
        .output()
        .is_ok_and(|output| output.status.success())
}

#[tokio::test]
async fn indexed_array_assignments_expand_variable_subscripts_without_filling_holes() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                VALUES=()
                INDEX=3
                VALUES[$INDEX]='fourth value'
                printf 'value=<%s> indexes=<%s> count=<%s>\n' "${VALUES[$INDEX]}" "${!VALUES[@]}" "${#VALUES[@]}"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "value=<fourth value> indexes=<3> count=<1>\n"
    );
}

#[tokio::test]
async fn test_parameter_modifiers_on_arrays_and_maps() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        PATHS=("/var/log/syslog" "/etc/config.json")
        declare -A HTTP_HEADERS
        HTTP_HEADERS[auth]="Bearer secret_token_12345"
        
        echo "filename: ${PATHS[0]##*/}"
        echo "token: ${HTTP_HEADERS[auth]##* }"
        echo "missing with fallback: ${HTTP_HEADERS[cookie]:-no_cookie_present}"
        echo "sub_slice: ${HTTP_HEADERS[auth]:7:6}"
    "#;

    let code = engine.eval_str(script, &mut ctx).await.unwrap();
    assert_eq!(code, 0);

    let output = ctx.get_captured_stdout().unwrap();
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines[0], "filename: syslog");
    assert_eq!(lines[1], "token: secret_token_12345");
    assert_eq!(lines[2], "missing with fallback: no_cookie_present");
    assert_eq!(lines[3], "sub_slice: secret");
}

#[tokio::test]
async fn indexed_array_at_expansion_preserves_argument_and_empty_element_boundaries() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                capture() {
                    printf 'count=%s first=<%s> second=<%s> third=<%s>\n' "$#" "$1" "$2" "$3"
                }
                ITEMS=("two words" "" "*.txt")
                capture "${ITEMS[@]}"
                EMPTY=()
                capture "${EMPTY[@]}"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "count=3 first=<two words> second=<> third=<*.txt>\ncount=0 first=<> second=<> third=<>\n"
    );
}

#[tokio::test]
async fn unset_array_and_map_lists_produce_no_for_loop_items() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                set -u
                TRACE=
                capture() { printf '%s ' "$#"; }
                unset MISSING_ARRAY MISSING_MAP
                capture "${MISSING_ARRAY[@]}"
                capture "${!MISSING_ARRAY[@]}"
                capture "${MISSING_MAP[@]}"
                capture "${!MISSING_MAP[@]}"
                printf '\n'
                for item in "${MISSING_ARRAY[@]}"; do
                    TRACE="${TRACE}array-value,"
                done
                for key in "${!MISSING_ARRAY[@]}"; do
                    TRACE="${TRACE}array-key,"
                done
                for item in "${MISSING_MAP[@]}"; do
                    TRACE="${TRACE}map-value,"
                done
                for key in "${!MISSING_MAP[@]}"; do
                    TRACE="${TRACE}map-key,"
                done
                printf 'trace=<%s>\n' "$TRACE"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "0 0 0 0 \ntrace=<>\n");
}

#[tokio::test]
async fn array_at_pattern_removal_and_slicing_preserve_sparse_values() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                show() {
                    printf 'argc=%s' "$#"
                    for item in "$@"; do printf ' <%s>' "$item"; done
                    printf '\n'
                }
                declare -a FILES
                FILES[0]=alpha.one.txt
                FILES[3]=''
                FILES[8]=beta.tar.gz
                show "${FILES[@]#*.}"
                show "${FILES[@]:1:2}"
                show ${FILES[@]:2}
                show "${FILES[@]:8:2}"
                IFS=:
                show "${FILES[*]#*.}"
                show "${FILES[*]:1:2}"
                IFS=' '
                show ${FILES[@]##*.}
                show "${FILES[@]%.*}"
                show "${FILES[@]%%.*}"
                show "before:${FILES[@]#*.}:after"

                declare -A MAP
                MAP[z]=beta.tar.gz
                MAP[a]=alpha.one.txt
                show "${MAP[@]#*.}"
                show "${MAP[@]:1}"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "argc=3 <one.txt> <> <tar.gz>\nargc=2 <> <beta.tar.gz>\nargc=2 <> <beta.tar.gz>\nargc=1 <beta.tar.gz>\nargc=1 <one.txt  tar.gz>\nargc=1 < beta.tar.gz>\nargc=3 <txt> <> <gz>\nargc=3 <alpha.one> <> <beta.tar>\nargc=3 <alpha> <> <beta>\nargc=1 <before:one.txt  tar.gz:after>\nargc=2 <one.txt> <tar.gz>\nargc=1 <beta.tar.gz>\n"
    );

    #[cfg(unix)]
    {
        // ESH does not field-split unquoted expansions, so its empty array
        // element remains an argument where Bash removes it.
        let reference_script = r#"
            show() {
                printf 'argc=%s' "$#"
                for item in "$@"; do printf ' <%s>' "$item"; done
                printf '\n'
            }
            declare -a FILES
            FILES[0]=alpha.one.txt
            FILES[3]=''
            FILES[8]=beta.tar.gz
            show "${FILES[@]#*.}"
            show "${FILES[@]:1:2}"
            show ${FILES[@]:2}
            show "${FILES[@]:8:2}"
            IFS=:
            show "${FILES[*]#*.}"
            show "${FILES[*]:1:2}"
            IFS=' '
            show ${FILES[@]##*.}
            show "${FILES[@]%.*}"
            show "${FILES[@]%%.*}"
        "#;
        let reference = Command::new("bash")
            .args(["--posix", "-c", reference_script])
            .output()
            .expect("Bash should be available for the indexed array comparison");
        assert!(reference.status.success());
        assert_eq!(
            reference.stdout,
            b"argc=3 <one.txt> <> <tar.gz>\nargc=2 <> <beta.tar.gz>\nargc=1 <beta.tar.gz>\nargc=1 <beta.tar.gz>\nargc=1 <one.txt::tar.gz>\nargc=1 <:beta.tar.gz>\nargc=2 <txt> <gz>\nargc=3 <alpha.one> <> <beta.tar>\nargc=3 <alpha> <> <beta>\n"
        );
    }

    for expression in ["${ITEMS[@]:abc}", "${ITEMS[@]:1:}", "${ITEMS[@]:1:2:3}"] {
        let mut invalid_ctx = ExecutionContext::new();
        let script = format!("ITEMS=(one two)\nprintf '%s' \"{expression}\"\n");
        assert!(
            engine.eval_str(&script, &mut invalid_ctx).await.is_err(),
            "invalid array slice {expression} should fail"
        );
    }

    let mut nounset_ctx = ExecutionContext::new();
    assert!(
        engine
            .eval_str(
                "set -u\nprintf '%s' \"${MISSING[@]:0}\"\n",
                &mut nounset_ctx
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn associative_map_slices_use_sorted_keys_and_preserve_value_boundaries() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                declare -A VALUES
                VALUES[z]='last word'
                VALUES[m]=''
                VALUES[b]='two words'
                VALUES[a]=first

                show() {
                    printf 'count=%s' "$#"
                    for value in "$@"; do printf ' <%s>' "$value"; done
                    printf '\n'
                }

                show "${VALUES[@]:1:2}"
                show "${VALUES[@]:2}"
                show "${VALUES[@]:3:20}"
                show "${VALUES[*]:1:3}"

                declare -A PATTERNS
                PATTERNS[z]=last.tar.gz
                PATTERNS[m]=''
                PATTERNS[a]=alpha.one.txt
                show "${PATTERNS[@]#*.}"
                show "${PATTERNS[*]#*.}"
                show "${PATTERNS[@]%.*}"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "count=2 <two words> <>\ncount=2 <> <last word>\ncount=1 <last word>\ncount=1 <two words  last word>\ncount=3 <one.txt> <> <tar.gz>\ncount=1 <one.txt  tar.gz>\ncount=3 <alpha.one> <> <last.tar>\n"
    );
}

#[tokio::test]
async fn associative_map_values_and_keys_expand_as_ordered_individual_items() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                declare -A VALUES
                VALUES[z]=a
                VALUES[a]="two words"
                VALUES[m]=""
                VALUES[0]=numeric-key
                capture() {
                    printf 'count=%s first=<%s> second=<%s> third=<%s> fourth=<%s>\n' "$#" "$1" "$2" "$3" "$4"
                }
                capture "${VALUES[@]}"
                for key in "${!VALUES[@]}"; do
                    printf '%s=<%s>\n' "$key" "${VALUES[$key]}"
                done
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "count=4 first=<numeric-key> second=<two words> third=<> fourth=<a>\n0=<numeric-key>\na=<two words>\nm=<>\nz=<a>\n"
    );
}

#[tokio::test]
async fn indexed_arrays_preserve_sparse_indexes_without_creating_hole_values() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                declare -a ITEMS
                ITEMS[8]=last
                ITEMS[1]=first
                ITEMS[3]=middle
                echo "count=${#ITEMS[@]} keys=${!ITEMS[@]} values=${ITEMS[@]}"
                capture() {
                    printf 'args=%s first=<%s> second=<%s> third=<%s>\n' "$#" "$1" "$2" "$3"
                }
                capture "${ITEMS[@]}"
                for index in "${!ITEMS[@]}"; do
                    printf '%s=<%s>\n' "$index" "${ITEMS[$index]}"
                done
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "count=3 keys=1 3 8 values=first middle last\nargs=3 first=<first> second=<middle> third=<last>\n1=<first>\n3=<middle>\n8=<last>\n"
    );
}

#[tokio::test]
async fn unset_removes_individual_indexed_array_and_map_elements() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r#"
                ITEMS=(zero one two)
                unset 'ITEMS[1]' 'ITEMS[90]'
                ARRAY_COUNT=${#ITEMS[@]}
                ARRAY_INDEXES=${!ITEMS[@]}
                ARRAY_VALUES=${ITEMS[@]}
                ARRAY_MISSING=${ITEMS[1]}
                printf 'array-count=%s indexes=%s values=%s missing=<%s>\n' \
                    "$ARRAY_COUNT" "$ARRAY_INDEXES" "$ARRAY_VALUES" "$ARRAY_MISSING"

                mutate_local_array() {
                    local ITEMS
                    ITEMS=(inner second)
                    LOCAL_BEFORE=${ITEMS[@]}
                    unset 'ITEMS[0]'
                    LOCAL_VALUES=${ITEMS[@]}
                    printf 'local-before=%s after=%s\n' "$LOCAL_BEFORE" "$LOCAL_VALUES"
                }
                mutate_local_array
                GLOBAL_VALUES=${ITEMS[@]}
                printf 'global-values=%s\n' "$GLOBAL_VALUES"

                declare -A COLORS
                COLORS[blue]=ocean
                COLORS[green]=forest
                COLORS[0]=numeric-key
                unset 'COLORS[blue]' 'COLORS[missing]' 'COLORS[0]'
                MAP_COUNT=${#COLORS[@]}
                MAP_KEYS=${!COLORS[@]}
                MAP_MISSING=${COLORS[blue]}
                MAP_GREEN=${COLORS[green]}
                printf 'map-count=%s keys=%s missing=<%s> green=<%s>\n' \
                    "$MAP_COUNT" "$MAP_KEYS" "$MAP_MISSING" "$MAP_GREEN"
            "#,
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "array-count=2 indexes=0 2 values=zero two missing=<>\nlocal-before=inner second after=second\nglobal-values=zero two\nmap-count=1 keys=green missing=<> green=<forest>\n"
    );
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}
