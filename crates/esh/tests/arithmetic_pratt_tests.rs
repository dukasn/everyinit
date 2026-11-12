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

use esh::planner::compiler::compile_script;
use esh::runtime::context::ExecutionContext;
use esh::runtime::eval_arithmetic_expr;
use esh::runtime::traits::{Executable, ScopeManager};

#[tokio::test]
async fn test_arithmetic_all_operators() {
    let script = r#"
        A=10
        B=3
        ADD=$(( A + B ))
        SUB=$(( A - B ))
        MUL=$(( A * B ))
        DIV=$(( A / B ))
        MOD=$(( A % B ))
        NEG_DIV=$(( -7 / 3 ))
        NEG_MOD=$(( -7 % 3 ))
        NEG_DIVISOR=$(( 7 / -3 ))
        NEG_DIVISOR_MOD=$(( 7 % -3 ))
        POW=$(( 2 ** 8 ))
        SHL=$(( 1 << 4 ))
        SHR=$(( 32 >> 2 ))
        BIT_AND=$(( 15 & 6 ))
        BIT_OR=$(( 8 | 3 ))
        BIT_XOR=$(( 15 ^ 7 ))
        BIT_NOT=$(( ~0 ))
        LOG_NOT=$(( !0 ))
        LOG_NOT_NZ=$(( !42 ))
    "#;

    let plan = compile_script(script).expect("compile");
    let mut ctx = ExecutionContext::new();
    plan.execute(&mut ctx).await.expect("execute");

    assert_eq!(ctx.get_var("ADD").as_deref(), Some("13"));
    assert_eq!(ctx.get_var("SUB").as_deref(), Some("7"));
    assert_eq!(ctx.get_var("MUL").as_deref(), Some("30"));
    assert_eq!(ctx.get_var("DIV").as_deref(), Some("3"));
    assert_eq!(ctx.get_var("MOD").as_deref(), Some("1"));
    assert_eq!(ctx.get_var("NEG_DIV").as_deref(), Some("-2"));
    assert_eq!(ctx.get_var("NEG_MOD").as_deref(), Some("-1"));
    assert_eq!(ctx.get_var("NEG_DIVISOR").as_deref(), Some("-2"));
    assert_eq!(ctx.get_var("NEG_DIVISOR_MOD").as_deref(), Some("1"));
    assert_eq!(ctx.get_var("POW").as_deref(), Some("256"));
    assert_eq!(ctx.get_var("SHL").as_deref(), Some("16"));
    assert_eq!(ctx.get_var("SHR").as_deref(), Some("8"));
    assert_eq!(ctx.get_var("BIT_AND").as_deref(), Some("6"));
    assert_eq!(ctx.get_var("BIT_OR").as_deref(), Some("11"));
    assert_eq!(ctx.get_var("BIT_XOR").as_deref(), Some("8"));
    assert_eq!(ctx.get_var("BIT_NOT").as_deref(), Some("-1"));
    assert_eq!(ctx.get_var("LOG_NOT").as_deref(), Some("1"));
    assert_eq!(ctx.get_var("LOG_NOT_NZ").as_deref(), Some("0"));
}

#[tokio::test]
async fn test_arithmetic_radix_literals() {
    let script = r#"
        HEX=$(( 0xFF ))
        HEX_UPPER=$(( 0XFF ))
        BIN=$(( 0b1010 ))
        BIN_UPPER=$(( 0B1010 ))
        OCT=$(( 0o77 ))
        OCT_UPPER=$(( 0O77 ))
        POSIX_OCT=$(( 077 ))
        CALC=$(( 0x10 + 0b10 + 0o10 ))
        UNARY_POWER=$(( -2 ** 2 ))
        RIGHT_ASSOCIATIVE_POWER=$(( 2 ** 3 ** 2 ))
        NEGATIVE_POWER=$(( 2 ** -2 ))
    "#;

    let plan = compile_script(script).expect("compile");
    let mut ctx = ExecutionContext::new();
    plan.execute(&mut ctx).await.expect("execute");

    assert_eq!(ctx.get_var("HEX").as_deref(), Some("255"));
    assert_eq!(ctx.get_var("HEX_UPPER").as_deref(), Some("255"));
    assert_eq!(ctx.get_var("BIN").as_deref(), Some("10"));
    assert_eq!(ctx.get_var("BIN_UPPER").as_deref(), Some("10"));
    assert_eq!(ctx.get_var("OCT").as_deref(), Some("63"));
    assert_eq!(ctx.get_var("OCT_UPPER").as_deref(), Some("63"));
    assert_eq!(ctx.get_var("POSIX_OCT").as_deref(), Some("63"));
    assert_eq!(ctx.get_var("CALC").as_deref(), Some("26")); // 16 + 2 + 8
    assert_eq!(ctx.get_var("UNARY_POWER").as_deref(), Some("4"));
    assert_eq!(
        ctx.get_var("RIGHT_ASSOCIATIVE_POWER").as_deref(),
        Some("512")
    );
    assert_eq!(ctx.get_var("NEGATIVE_POWER").as_deref(), Some("0"));
}

#[tokio::test]
async fn test_arithmetic_ternary_and_comparisons() {
    let script = r#"
        SCORE=85
        GRADE=$(( SCORE >= 90 ? 1 : SCORE >= 80 ? 2 : 3 ))
        EQ=$(( 5 == 5 ))
        NE=$(( 5 != 5 ))
        LT=$(( 3 < 10 ))
        LE=$(( 10 <= 10 ))
        GT=$(( 3 > 10 ))
        GE=$(( 10 >= 10 ))
        AND_TRUE=$(( 1 && 5 ))
        AND_FALSE=$(( 0 && 5 ))
        OR_TRUE=$(( 0 || 7 ))
    "#;

    let plan = compile_script(script).expect("compile");
    let mut ctx = ExecutionContext::new();
    plan.execute(&mut ctx).await.expect("execute");

    assert_eq!(ctx.get_var("GRADE").as_deref(), Some("2"));
    assert_eq!(ctx.get_var("EQ").as_deref(), Some("1"));
    assert_eq!(ctx.get_var("NE").as_deref(), Some("0"));
    assert_eq!(ctx.get_var("LT").as_deref(), Some("1"));
    assert_eq!(ctx.get_var("LE").as_deref(), Some("1"));
    assert_eq!(ctx.get_var("GT").as_deref(), Some("0"));
    assert_eq!(ctx.get_var("GE").as_deref(), Some("1"));
    assert_eq!(ctx.get_var("AND_TRUE").as_deref(), Some("1"));
    assert_eq!(ctx.get_var("AND_FALSE").as_deref(), Some("0"));
    assert_eq!(ctx.get_var("OR_TRUE").as_deref(), Some("1"));
}

#[tokio::test]
async fn test_arithmetic_assignments_and_mutations() {
    let script = r#"
        COUNT=10
        RES1=$(( COUNT += 5 ))
        RES2=$(( COUNT *= 2 ))
        RES3=$(( COUNT -= 6 ))
        RES4=$(( COUNT /= 4 ))
        RES5=$(( COUNT %= 5 ))
        
        I=0
        PRE=$(( ++I ))
        POST=$(( I++ ))
        FINAL_I=$I
    "#;

    let plan = compile_script(script).expect("compile");
    let mut ctx = ExecutionContext::new();
    plan.execute(&mut ctx).await.expect("execute");

    assert_eq!(ctx.get_var("RES1").as_deref(), Some("15"));
    assert_eq!(ctx.get_var("RES2").as_deref(), Some("30"));
    assert_eq!(ctx.get_var("RES3").as_deref(), Some("24"));
    assert_eq!(ctx.get_var("RES4").as_deref(), Some("6"));
    assert_eq!(ctx.get_var("RES5").as_deref(), Some("1"));
    assert_eq!(ctx.get_var("COUNT").as_deref(), Some("1"));

    assert_eq!(ctx.get_var("PRE").as_deref(), Some("1"));
    assert_eq!(ctx.get_var("POST").as_deref(), Some("1"));
    assert_eq!(ctx.get_var("FINAL_I").as_deref(), Some("2"));
}

#[test]
fn arithmetic_rejects_malformed_input_and_zero_division() {
    let mut ctx = ExecutionContext::new();
    for expr in ["1 / 0", "5 % 0", "1 +", "1 2", "1 @ 2", "0x", "08"] {
        assert!(eval_arithmetic_expr(expr, &mut ctx).is_err(), "{expr}");
    }
}

#[test]
fn arithmetic_rejects_shift_counts_outside_signed_word_width() {
    let mut ctx = ExecutionContext::new();
    for expr in ["1 << -1", "1 >> -1", "1 << 64", "1 >> 64", "X = 1 << 64"] {
        assert!(eval_arithmetic_expr(expr, &mut ctx).is_err(), "{expr}");
    }
    assert!(ctx.get_var("X").is_none());

    assert_eq!(eval_arithmetic_expr("0 && (1 << -1)", &mut ctx).unwrap(), 0);
    assert_eq!(eval_arithmetic_expr("1 || (1 >> 64)", &mut ctx).unwrap(), 1);
    assert_eq!(eval_arithmetic_expr("-7 >> 1", &mut ctx).unwrap(), -4);
    assert_eq!(eval_arithmetic_expr("-7 >> 2", &mut ctx).unwrap(), -2);
}

#[test]
fn arithmetic_logical_and_conditional_operators_short_circuit() {
    let mut ctx = ExecutionContext::new();
    assert_eq!(
        eval_arithmetic_expr("0 && (X = 1 / 0)", &mut ctx).unwrap(),
        0
    );
    assert_eq!(
        eval_arithmetic_expr("1 || (X = 1 / 0)", &mut ctx).unwrap(),
        1
    );
    assert_eq!(
        eval_arithmetic_expr("1 ? 7 : (X = 1 / 0)", &mut ctx).unwrap(),
        7
    );
    assert_eq!(ctx.get_var("X"), None);
}

#[test]
fn arithmetic_wraps_signed_64_bit_overflow_without_panicking() {
    let mut ctx = ExecutionContext::new();
    assert_eq!(
        eval_arithmetic_expr("9223372036854775807 + 1", &mut ctx).unwrap(),
        i64::MIN
    );
    assert_eq!(
        eval_arithmetic_expr("-9223372036854775808 - 1", &mut ctx).unwrap(),
        i64::MAX
    );
    assert_eq!(
        eval_arithmetic_expr("9223372036854775807 * 2", &mut ctx).unwrap(),
        -2
    );
    assert_eq!(
        eval_arithmetic_expr("-(-9223372036854775808)", &mut ctx).unwrap(),
        i64::MIN
    );
    assert_eq!(eval_arithmetic_expr("2 ** 63", &mut ctx).unwrap(), i64::MIN);
    assert_eq!(eval_arithmetic_expr("1 << 63", &mut ctx).unwrap(), i64::MIN);
    assert_eq!(
        eval_arithmetic_expr("(9223372036854775807 + 1) / -1", &mut ctx).unwrap(),
        i64::MIN
    );
    assert_eq!(
        eval_arithmetic_expr("(-9223372036854775807 - 1) % -1", &mut ctx).unwrap(),
        0
    );

    ctx.set_var("ACC", &i64::MAX.to_string());
    assert_eq!(
        eval_arithmetic_expr("ACC += 1", &mut ctx).unwrap(),
        i64::MIN
    );
    assert_eq!(ctx.get_var("ACC").as_deref(), Some("-9223372036854775808"));

    ctx.set_var("ACC", &i64::MAX.to_string());
    assert_eq!(eval_arithmetic_expr("ACC++", &mut ctx).unwrap(), i64::MAX);
    assert_eq!(ctx.get_var("ACC").as_deref(), Some("-9223372036854775808"));

    ctx.set_var("ACC", &i64::MIN.to_string());
    assert_eq!(eval_arithmetic_expr("--ACC", &mut ctx).unwrap(), i64::MAX);
    assert_eq!(ctx.get_var("ACC").as_deref(), Some("9223372036854775807"));
}

#[test]
fn arithmetic_accepts_the_signed_minimum_literal_only_when_negated() {
    let mut ctx = ExecutionContext::new();
    assert_eq!(
        eval_arithmetic_expr("-9223372036854775808", &mut ctx).unwrap(),
        i64::MIN
    );
    assert_eq!(
        eval_arithmetic_expr("-0x8000000000000000", &mut ctx).unwrap(),
        i64::MIN
    );
    assert!(eval_arithmetic_expr("9223372036854775808", &mut ctx).is_err());
    assert_eq!(
        eval_arithmetic_expr("-9223372036854775808 / -1", &mut ctx).unwrap(),
        i64::MIN
    );
}

#[test]
fn arithmetic_nounset_errors_only_when_an_unset_variable_is_read() {
    let mut ctx = ExecutionContext::new();
    ctx.options_mut().nounset = true;

    for expression in ["MISSING + 1", "MISSING++"] {
        assert!(
            eval_arithmetic_expr(expression, &mut ctx).is_err(),
            "expected nounset error for {expression}"
        );
    }
    assert_eq!(eval_arithmetic_expr("0 && MISSING", &mut ctx).unwrap(), 0);
    assert_eq!(eval_arithmetic_expr("1 || MISSING", &mut ctx).unwrap(), 1);

    let mut assignment_ctx = ExecutionContext::new();
    assignment_ctx.options_mut().nounset = true;
    assert_eq!(
        eval_arithmetic_expr("MISSING = 4", &mut assignment_ctx).unwrap(),
        4
    );
    assert_eq!(assignment_ctx.get_var("MISSING").as_deref(), Some("4"));
}

#[test]
fn arithmetic_unset_and_empty_values_default_to_zero_without_nounset() {
    let mut ctx = ExecutionContext::new();
    ctx.set_var("EMPTY", "");

    assert_eq!(eval_arithmetic_expr("MISSING + 1", &mut ctx).unwrap(), 1);
    assert_eq!(eval_arithmetic_expr("EMPTY + 1", &mut ctx).unwrap(), 1);

    ctx.options_mut().nounset = true;
    assert_eq!(eval_arithmetic_expr("EMPTY + 1", &mut ctx).unwrap(), 1);
    assert!(eval_arithmetic_expr("MISSING + 1", &mut ctx).is_err());
}

#[test]
fn arithmetic_variable_values_are_recursively_evaluated_and_guard_cycles() {
    let mut ctx = ExecutionContext::new();
    ctx.set_var("BASE", "1 + 2");
    ctx.set_var("SCALED", "BASE * 3");
    ctx.set_var("OCTAL", "010");
    ctx.set_var("SIGNED_OCTAL", "+010");
    ctx.set_var("NEGATIVE_OCTAL", "-010");
    ctx.set_var("HEX", "0x10");
    ctx.set_var("SIGNED_HEX", "+0X10");
    ctx.set_var("NEGATIVE_HEX", "-0X10");

    assert_eq!(eval_arithmetic_expr("SCALED + 1", &mut ctx).unwrap(), 10);
    assert_eq!(eval_arithmetic_expr("OCTAL", &mut ctx).unwrap(), 8);
    assert_eq!(eval_arithmetic_expr("SIGNED_OCTAL", &mut ctx).unwrap(), 8);
    assert_eq!(
        eval_arithmetic_expr("NEGATIVE_OCTAL", &mut ctx).unwrap(),
        -8
    );
    assert_eq!(eval_arithmetic_expr("HEX", &mut ctx).unwrap(), 16);
    assert_eq!(eval_arithmetic_expr("SIGNED_HEX", &mut ctx).unwrap(), 16);
    assert_eq!(eval_arithmetic_expr("NEGATIVE_HEX", &mut ctx).unwrap(), -16);
    assert_eq!(eval_arithmetic_expr("BASE++", &mut ctx).unwrap(), 3);
    assert_eq!(ctx.get_var("BASE").as_deref(), Some("4"));

    ctx.set_var("CYCLE_A", "CYCLE_B + 1");
    ctx.set_var("CYCLE_B", "CYCLE_A + 1");
    assert!(eval_arithmetic_expr("CYCLE_A", &mut ctx).is_err());
}
