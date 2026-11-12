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

#[tokio::test]
async fn printf_requires_a_format_operand() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine.eval_str("printf\n", &mut ctx).await.unwrap();

    assert_eq!(status, 2);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "");
    assert_eq!(
        ctx.get_captured_stderr().unwrap(),
        "esh: printf: missing format operand\n"
    );
}

#[tokio::test]
async fn printf_numeric_formatting_is_independent_of_lc_numeric() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str("LC_NUMERIC=fr_FR.UTF-8 printf '%.2f\\n' 1234.5\n", &mut ctx)
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "1234.50\n");
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}

#[tokio::test]
async fn printf_unknown_backslash_escapes_emit_the_following_character() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str("printf '<\\q|%b>\\n' '\\z'\n", &mut ctx)
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "<q|z>\n");
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}

#[tokio::test]
async fn unsupported_and_incomplete_printf_conversions_are_reported() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str("printf 'before:%q:%s\\n' kept\nprintf 'after%'\n", &mut ctx)
        .await
        .unwrap();

    assert_eq!(status, 1);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "before::kept\nafter");
    assert_eq!(
        ctx.get_captured_stderr().unwrap(),
        "esh: printf: invalid conversion specifier %q\nesh: printf: incomplete format conversion\n"
    );
}

#[tokio::test]
async fn numeric_conversion_errors_emit_diagnostics_and_continue_processing() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "printf '%d:%s;' '12x' first 2 second\nprintf '%f:%s\\n' '2.5tail' continued\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 1);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "12:first;2:second;2.500000:continued\n"
    );
    assert_eq!(
        ctx.get_captured_stderr().unwrap(),
        "esh: printf: 12x: expected numeric value\nesh: printf: 2.5tail: expected numeric value\n"
    );
}

#[tokio::test]
async fn integer_overflow_clamps_to_conversion_range_and_returns_failure() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "printf '%d:%u:%d:%u\\n' 9223372036854775808 18446744073709551616 -9223372036854775809 -18446744073709551616\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 1);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "9223372036854775807:18446744073709551615:-9223372036854775808:18446744073709551615\n"
    );
    assert_eq!(
        ctx.get_captured_stderr().unwrap(),
        "esh: printf: 9223372036854775808: expected numeric value\nesh: printf: 18446744073709551616: expected numeric value\nesh: printf: -9223372036854775809: expected numeric value\nesh: printf: -18446744073709551616: expected numeric value\n"
    );
}

#[tokio::test]
async fn floating_range_errors_report_and_continue() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "printf '%g:%g:%g:%s\\n' 1e400 1e-4000 1e-320 finished\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 1);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "inf:0:9.99989e-321:finished\n"
    );
    assert_eq!(
        ctx.get_captured_stderr().unwrap(),
        "esh: printf: 1e400: numeric result out of range\nesh: printf: 1e-4000: numeric result out of range\nesh: printf: 1e-320: numeric result out of range\n"
    );
}

#[tokio::test]
async fn floating_printf_sign_flags_and_negative_nan_keep_the_sign() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "printf '<%+f><% f><%F><%+g><% f>\\n' nan nan -nan nan -nan\nprintf '<%+a><% A>\\n' nan -nan\nprintf '[%010a][%010A]\\n' inf nan\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "<+nan>< nan><-NAN><+nan><-nan>\n<+nan><-NAN>\n[       inf][       NAN]\n"
    );
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}

#[tokio::test]
async fn general_float_rounding_trims_zeros_but_alternate_form_keeps_them() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "printf '[%.6g][%#.6g][%.6g][%.6g][%.0g][%#.0g]\\n' 999999.5 999999.5 0.00009999995 0.000009999995 42 42\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "[1e+06][1.00000e+06][0.0001][1e-05][4e+01][4.e+01]\n"
    );
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}

#[tokio::test]
async fn scientific_float_alternate_form_keeps_decimal_point_at_zero_precision() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str("printf '[%#.0e][%#.0E]\\n' 12 12\n", &mut ctx)
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "[1.e+01][1.E+01]\n");
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}

#[tokio::test]
async fn hexadecimal_exponents_beyond_i32_range_are_clamped_and_reported() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "printf '%g:%g\\n' 0x1p+999999999999999999999 0x1p-999999999999999999999\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 1);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "inf:0\n");
    assert_eq!(
        ctx.get_captured_stderr().unwrap(),
        "esh: printf: 0x1p+999999999999999999999: numeric result out of range\nesh: printf: 0x1p-999999999999999999999: numeric result out of range\n"
    );
}

#[tokio::test]
async fn hexadecimal_float_conversions_support_case_precision_and_width() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "printf '[%a][%A][%.0a][%#.0a][%020a][%a]\\n' 6 6 1.75 2 6 -0\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "[0x1.8p+2][0X1.8P+2][0x1p+1][0x1.p+1][0x0000000000001.8p+2][-0x0p+0]\n"
    );
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");

    let status = engine
        .eval_str("printf '%a %A\\n' 2.2250738585072014e-308 inf\n", &mut ctx)
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "[0x1.8p+2][0X1.8P+2][0x1p+1][0x1.p+1][0x0000000000001.8p+2][-0x0p+0]\n0x1p-1022 INF\n"
    );
}

#[tokio::test]
async fn hexadecimal_float_precision_uses_ties_to_even_and_rounds_subnormals() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "printf '[%.1a][%.1a][%.0a]\\n' 1.03125 1.09375 0x0.fffffffffffffp-1022\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 1);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "[0x1.0p+0][0x1.2p+0][0x1p-1022]\n"
    );
    assert_eq!(
        ctx.get_captured_stderr().unwrap(),
        "esh: printf: 0x0.fffffffffffffp-1022: numeric result out of range\n"
    );
}

#[tokio::test]
async fn format_octal_escapes_are_literals_before_conversion_scanning() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str("printf '\\101|\\0123|\\045s\\n'\n", &mut ctx)
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "A|\n3|%s\n");
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}

#[tokio::test]
async fn octal_format_escapes_write_raw_bytes() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str("printf '\\303\\251'\n", &mut ctx)
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout_bytes().unwrap(), [0xc3, 0xa9]);
}

#[tokio::test]
async fn octal_format_escapes_above_one_byte_wrap_to_output_bytes() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str("printf '\\400|\\445s|\\777'\n", &mut ctx)
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout_bytes().unwrap(),
        [0x00, b'|', b'%', b's', b'|', 0xff]
    );
}

#[tokio::test]
async fn percent_b_octal_escapes_write_raw_bytes() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str("printf '%b' '\\0303\\0251\\0377\\0'\n", &mut ctx)
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout_bytes().unwrap(),
        [0xc3, 0xa9, 0xff, 0x00]
    );
}

#[tokio::test]
async fn percent_b_supports_posix_escapes_and_stops_at_backslash_c() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            r"printf '%b' '\a\b\f\n\r\t\v\\\0101'
printf 'prefix:%b:suffix:%s\n' 'x\cy' ignored
",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(
        ctx.get_captured_stdout_bytes().unwrap(),
        [
            0x07, 0x08, 0x0c, b'\n', b'\r', b'\t', 0x0b, b'\\', b'A', b'p', b'r', b'e', b'f', b'i',
            b'x', b':', b'x'
        ]
    );
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}

#[tokio::test]
async fn percent_b_precision_applies_before_backslash_c_stops_the_format() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "printf '<%.2b>|%s' 'abc\\cignored' tail\nprintf '\\nnext\\n'\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "<ab\nnext\n");
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}

#[tokio::test]
async fn string_printf_conversions_count_and_select_unicode_scalars() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "printf '[%.1s][%.1b][%c]\\n' '雪a' '雪a' '雪a'\nprintf '<%c>' ''\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "[雪][雪][雪]\n<\0>");
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}

#[tokio::test]
async fn quoted_integer_operands_use_the_first_unicode_scalar() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(r#"printf '%d:%u\n' "'雪" '"é'"#, &mut ctx)
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "38634:233\n");
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}

#[tokio::test]
async fn negative_dynamic_precision_is_omitted_for_strings_integers_and_floats() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "printf '[%.*f][%*.*s][%.*d]\\n' -1 1.25 -4 -1 abc -1 7\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 0);
    assert_eq!(ctx.get_captured_stdout().unwrap(), "[1.250000][abc ][7]\n");
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}

#[tokio::test]
async fn printf_numeric_operands_allow_leading_but_reject_trailing_whitespace() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let status = engine
        .eval_str(
            "printf '<%d><%f>\\n' ' 12' ' 1.5'\nprintf '<%d><%f>\\n' '12 ' '1.5 '\nprintf '<%d>\\n' \" 'A\"\n",
            &mut ctx,
        )
        .await
        .unwrap();

    assert_eq!(status, 1);
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        "<12><1.500000>\n<12><1.500000>\n<0>\n"
    );
    assert_eq!(
        ctx.get_captured_stderr().unwrap(),
        "esh: printf: 12 : expected numeric value\nesh: printf: 1.5 : expected numeric value\nesh: printf:  'A: expected numeric value\n"
    );
}
