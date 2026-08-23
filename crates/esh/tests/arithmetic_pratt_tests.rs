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
        BIN=$(( 0b1010 ))
        OCT=$(( 0o77 ))
        CALC=$(( 0x10 + 0b10 + 0o10 ))
    "#;

    let plan = compile_script(script).expect("compile");
    let mut ctx = ExecutionContext::new();
    plan.execute(&mut ctx).await.expect("execute");

    assert_eq!(ctx.get_var("HEX").as_deref(), Some("255"));
    assert_eq!(ctx.get_var("BIN").as_deref(), Some("10"));
    assert_eq!(ctx.get_var("OCT").as_deref(), Some("63"));
    assert_eq!(ctx.get_var("CALC").as_deref(), Some("26")); // 16 + 2 + 8
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
