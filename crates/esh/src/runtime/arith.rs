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

use crate::error::ExecError;
use crate::runtime::context::ExecutionContext;
use crate::runtime::traits::ScopeManager;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArithToken {
    Num(i64),
    MinMagnitude,
    Var(String),
    // Operators
    Plus,      // +
    Minus,     // -
    Star,      // *
    Slash,     // /
    Percent,   // %
    Power,     // **
    Increment, // ++
    Decrement, // --
    Tilde,     // ~
    Bang,      // !
    Shl,       // <<
    Shr,       // >>
    Lt,        // <
    Le,        // <=
    Gt,        // >
    Ge,        // >=
    Eq,        // ==
    Ne,        // !=
    BitAnd,    // &
    BitXor,    // ^
    BitOr,     // |
    LogAnd,    // &&
    LogOr,     // ||
    Question,  // ?
    Colon,     // :
    Assign,    // =
    AddAssign, // +=
    SubAssign, // -=
    MulAssign, // *=
    DivAssign, // /=
    ModAssign, // %=
    ShlAssign, // <<=
    ShrAssign, // >>=
    AndAssign, // &=
    XorAssign, // ^=
    OrAssign,  // |=
    Comma,     // ,
    LParen,    // (
    RParen,    // )
}

pub fn tokenize_arith(expr: &str) -> Result<Vec<ArithToken>, ExecError> {
    let mut tokens = Vec::new();
    let bytes = expr.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_whitespace() {
            i += 1;
            continue;
        }

        // 3-char operators: <<=, >>=
        if i + 2 < bytes.len() {
            let three = &bytes[i..i + 3];
            if three == b"<<=" {
                tokens.push(ArithToken::ShlAssign);
                i += 3;
                continue;
            } else if three == b">>=" {
                tokens.push(ArithToken::ShrAssign);
                i += 3;
                continue;
            }
        }

        // 2-char operators
        if i + 1 < bytes.len() {
            let two = &bytes[i..i + 2];
            match two {
                b"++" => {
                    tokens.push(ArithToken::Increment);
                    i += 2;
                    continue;
                }
                b"--" => {
                    tokens.push(ArithToken::Decrement);
                    i += 2;
                    continue;
                }
                b"**" => {
                    tokens.push(ArithToken::Power);
                    i += 2;
                    continue;
                }
                b"<<" => {
                    tokens.push(ArithToken::Shl);
                    i += 2;
                    continue;
                }
                b">>" => {
                    tokens.push(ArithToken::Shr);
                    i += 2;
                    continue;
                }
                b"<=" => {
                    tokens.push(ArithToken::Le);
                    i += 2;
                    continue;
                }
                b">=" => {
                    tokens.push(ArithToken::Ge);
                    i += 2;
                    continue;
                }
                b"==" => {
                    tokens.push(ArithToken::Eq);
                    i += 2;
                    continue;
                }
                b"!=" => {
                    tokens.push(ArithToken::Ne);
                    i += 2;
                    continue;
                }
                b"&&" => {
                    tokens.push(ArithToken::LogAnd);
                    i += 2;
                    continue;
                }
                b"||" => {
                    tokens.push(ArithToken::LogOr);
                    i += 2;
                    continue;
                }
                b"+=" => {
                    tokens.push(ArithToken::AddAssign);
                    i += 2;
                    continue;
                }
                b"-=" => {
                    tokens.push(ArithToken::SubAssign);
                    i += 2;
                    continue;
                }
                b"*=" => {
                    tokens.push(ArithToken::MulAssign);
                    i += 2;
                    continue;
                }
                b"/=" => {
                    tokens.push(ArithToken::DivAssign);
                    i += 2;
                    continue;
                }
                b"%=" => {
                    tokens.push(ArithToken::ModAssign);
                    i += 2;
                    continue;
                }
                b"&=" => {
                    tokens.push(ArithToken::AndAssign);
                    i += 2;
                    continue;
                }
                b"^=" => {
                    tokens.push(ArithToken::XorAssign);
                    i += 2;
                    continue;
                }
                b"|=" => {
                    tokens.push(ArithToken::OrAssign);
                    i += 2;
                    continue;
                }
                _ => {}
            }
        }

        // 1-char operators
        match b {
            b'+' => {
                tokens.push(ArithToken::Plus);
                i += 1;
                continue;
            }
            b'-' => {
                tokens.push(ArithToken::Minus);
                i += 1;
                continue;
            }
            b'*' => {
                tokens.push(ArithToken::Star);
                i += 1;
                continue;
            }
            b'/' => {
                tokens.push(ArithToken::Slash);
                i += 1;
                continue;
            }
            b'%' => {
                tokens.push(ArithToken::Percent);
                i += 1;
                continue;
            }
            b'~' => {
                tokens.push(ArithToken::Tilde);
                i += 1;
                continue;
            }
            b'!' => {
                tokens.push(ArithToken::Bang);
                i += 1;
                continue;
            }
            b'<' => {
                tokens.push(ArithToken::Lt);
                i += 1;
                continue;
            }
            b'>' => {
                tokens.push(ArithToken::Gt);
                i += 1;
                continue;
            }
            b'&' => {
                tokens.push(ArithToken::BitAnd);
                i += 1;
                continue;
            }
            b'^' => {
                tokens.push(ArithToken::BitXor);
                i += 1;
                continue;
            }
            b'|' => {
                tokens.push(ArithToken::BitOr);
                i += 1;
                continue;
            }
            b'?' => {
                tokens.push(ArithToken::Question);
                i += 1;
                continue;
            }
            b':' => {
                tokens.push(ArithToken::Colon);
                i += 1;
                continue;
            }
            b'=' => {
                tokens.push(ArithToken::Assign);
                i += 1;
                continue;
            }
            b',' => {
                tokens.push(ArithToken::Comma);
                i += 1;
                continue;
            }
            b'(' => {
                tokens.push(ArithToken::LParen);
                i += 1;
                continue;
            }
            b')' => {
                tokens.push(ArithToken::RParen);
                i += 1;
                continue;
            }
            _ => {}
        }

        // Numbers (hex 0x, binary 0b, octal 0o / 0, decimal)
        if b.is_ascii_digit() {
            let start = i;
            if b == b'0' && i + 1 < bytes.len() {
                let next = bytes[i + 1];
                if next == b'x' || next == b'X' {
                    i += 2;
                    let hex_start = i;
                    while i < bytes.len() && bytes[i].is_ascii_hexdigit() {
                        i += 1;
                    }
                    let s = &expr[hex_start..i];
                    tokens.push(parse_integer_literal(s, 16, "hexadecimal", expr)?);
                    continue;
                } else if next == b'b' || next == b'B' {
                    i += 2;
                    let bin_start = i;
                    while i < bytes.len() && (bytes[i] == b'0' || bytes[i] == b'1') {
                        i += 1;
                    }
                    let s = &expr[bin_start..i];
                    tokens.push(parse_integer_literal(s, 2, "binary", expr)?);
                    continue;
                } else if next == b'o' || next == b'O' {
                    i += 2;
                    let oct_start = i;
                    while i < bytes.len() && (b'0'..=b'7').contains(&bytes[i]) {
                        i += 1;
                    }
                    let s = &expr[oct_start..i];
                    tokens.push(parse_integer_literal(s, 8, "octal", expr)?);
                    continue;
                }
            }

            if b == b'0' {
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
                let literal = &expr[start..i];
                tokens.push(parse_integer_literal(literal, 8, "octal", literal)?);
                continue;
            }

            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            let s = &expr[start..i];
            tokens.push(parse_integer_literal(s, 10, "decimal", s)?);
            continue;
        }

        // Identifiers / Variables (e.g. COUNT, $COUNT, ${COUNT})
        if b.is_ascii_alphabetic() || b == b'_' || b == b'$' {
            let start = i;
            if b == b'$' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
                i += 2;
                let var_start = i;
                while i < bytes.len() && bytes[i] != b'}' {
                    i += 1;
                }
                let name = &expr[var_start..i];
                if i < bytes.len() && bytes[i] == b'}' {
                    i += 1;
                }
                tokens.push(ArithToken::Var(name.to_string()));
                continue;
            }

            while i < bytes.len()
                && ((bytes[i] as char).is_ascii_alphanumeric()
                    || bytes[i] == b'_'
                    || bytes[i] == b'$')
            {
                i += 1;
            }
            let raw = &expr[start..i];
            let name = raw.trim_start_matches('$');
            tokens.push(ArithToken::Var(name.to_string()));
            continue;
        }

        let invalid = expr[i..]
            .chars()
            .next()
            .expect("index is within expression");
        return Err(ExecError::Custom(format!(
            "Invalid character '{invalid}' in arithmetic expression"
        )));
    }

    Ok(tokens)
}

fn parse_integer_literal(
    literal: &str,
    radix: u32,
    radix_name: &str,
    expression: &str,
) -> Result<ArithToken, ExecError> {
    let magnitude = i128::from_str_radix(literal, radix).map_err(|_| {
        ExecError::Custom(format!(
            "Invalid {radix_name} literal in arithmetic: {expression}"
        ))
    })?;
    let min_magnitude = -(i64::MIN as i128);
    if magnitude > min_magnitude {
        return Err(ExecError::Custom(format!(
            "Invalid {radix_name} literal in arithmetic: {expression}"
        )));
    }
    if magnitude == min_magnitude {
        Ok(ArithToken::MinMagnitude)
    } else {
        Ok(ArithToken::Num(magnitude as i64))
    }
}

pub struct ArithParser<'a> {
    tokens: Vec<ArithToken>,
    pos: usize,
    evaluating: bool,
    ctx: &'a mut ExecutionContext,
    resolving: &'a mut Vec<String>,
}

impl<'a> ArithParser<'a> {
    pub fn new(
        tokens: Vec<ArithToken>,
        ctx: &'a mut ExecutionContext,
        resolving: &'a mut Vec<String>,
    ) -> Self {
        Self {
            tokens,
            pos: 0,
            evaluating: true,
            ctx,
            resolving,
        }
    }

    fn peek(&self) -> Option<&ArithToken> {
        self.tokens.get(self.pos)
    }

    fn advance(&mut self) -> Option<ArithToken> {
        if self.pos < self.tokens.len() {
            let tok = self.tokens[self.pos].clone();
            self.pos += 1;
            Some(tok)
        } else {
            None
        }
    }

    fn get_var_val(&mut self, name: &str) -> Result<i64, ExecError> {
        if !self.evaluating {
            return Ok(0);
        }
        let Some(value) = self.ctx.get_var(name) else {
            if self.ctx.options().nounset {
                return Err(ExecError::Custom(format!("{name}: unbound variable")));
            }
            return Ok(0);
        };
        let value = value.trim();
        if value.is_empty() {
            return Ok(0);
        }
        if self.resolving.iter().any(|resolving| resolving == name) {
            return Err(ExecError::Custom(format!(
                "Circular arithmetic variable reference: {name}"
            )));
        }

        self.resolving.push(name.to_string());
        let result = eval_arithmetic_expr_with_stack(value, self.ctx, self.resolving);
        self.resolving.pop();
        result
    }

    fn set_var_val(&mut self, name: &str, val: i64) {
        if self.evaluating {
            self.ctx.set_var(name, &val.to_string());
        }
    }

    pub fn parse_expression(&mut self) -> Result<i64, ExecError> {
        self.parse_comma()
    }

    fn parse_comma(&mut self) -> Result<i64, ExecError> {
        let mut val = self.parse_assignment()?;
        while let Some(ArithToken::Comma) = self.peek() {
            self.advance();
            val = self.parse_assignment()?;
        }
        Ok(val)
    }

    fn parse_assignment(&mut self) -> Result<i64, ExecError> {
        // Look ahead for LValue assignment: Var OpAssign Expr
        if self.pos < self.tokens.len()
            && let ArithToken::Var(var_name) = &self.tokens[self.pos]
        {
            let var_name = var_name.clone();
            if self.pos + 1 < self.tokens.len() {
                let next_op = self.tokens[self.pos + 1].clone();
                match next_op {
                    ArithToken::Assign => {
                        self.pos += 2;
                        let rhs = self.parse_assignment()?;
                        self.set_var_val(&var_name, rhs);
                        return Ok(rhs);
                    }
                    ArithToken::AddAssign => {
                        self.pos += 2;
                        let rhs = self.parse_assignment()?;
                        let cur = self.get_var_val(&var_name)?;
                        let new_val = cur.wrapping_add(rhs);
                        self.set_var_val(&var_name, new_val);
                        return Ok(new_val);
                    }
                    ArithToken::SubAssign => {
                        self.pos += 2;
                        let rhs = self.parse_assignment()?;
                        let cur = self.get_var_val(&var_name)?;
                        let new_val = cur.wrapping_sub(rhs);
                        self.set_var_val(&var_name, new_val);
                        return Ok(new_val);
                    }
                    ArithToken::MulAssign => {
                        self.pos += 2;
                        let rhs = self.parse_assignment()?;
                        let cur = self.get_var_val(&var_name)?;
                        let new_val = cur.wrapping_mul(rhs);
                        self.set_var_val(&var_name, new_val);
                        return Ok(new_val);
                    }
                    ArithToken::DivAssign => {
                        self.pos += 2;
                        let rhs = self.parse_assignment()?;
                        let cur = self.get_var_val(&var_name)?;
                        let new_val = div_values(cur, rhs, self.evaluating)?;
                        self.set_var_val(&var_name, new_val);
                        return Ok(new_val);
                    }
                    ArithToken::ModAssign => {
                        self.pos += 2;
                        let rhs = self.parse_assignment()?;
                        let cur = self.get_var_val(&var_name)?;
                        let new_val = mod_values(cur, rhs, self.evaluating)?;
                        self.set_var_val(&var_name, new_val);
                        return Ok(new_val);
                    }
                    ArithToken::ShlAssign => {
                        self.pos += 2;
                        let rhs = self.parse_assignment()?;
                        let cur = self.get_var_val(&var_name)?;
                        let new_val = shift_value(cur, rhs, true, self.evaluating)?;
                        self.set_var_val(&var_name, new_val);
                        return Ok(new_val);
                    }
                    ArithToken::ShrAssign => {
                        self.pos += 2;
                        let rhs = self.parse_assignment()?;
                        let cur = self.get_var_val(&var_name)?;
                        let new_val = shift_value(cur, rhs, false, self.evaluating)?;
                        self.set_var_val(&var_name, new_val);
                        return Ok(new_val);
                    }
                    ArithToken::AndAssign => {
                        self.pos += 2;
                        let rhs = self.parse_assignment()?;
                        let cur = self.get_var_val(&var_name)?;
                        let new_val = cur & rhs;
                        self.set_var_val(&var_name, new_val);
                        return Ok(new_val);
                    }
                    ArithToken::XorAssign => {
                        self.pos += 2;
                        let rhs = self.parse_assignment()?;
                        let cur = self.get_var_val(&var_name)?;
                        let new_val = cur ^ rhs;
                        self.set_var_val(&var_name, new_val);
                        return Ok(new_val);
                    }
                    ArithToken::OrAssign => {
                        self.pos += 2;
                        let rhs = self.parse_assignment()?;
                        let cur = self.get_var_val(&var_name)?;
                        let new_val = cur | rhs;
                        self.set_var_val(&var_name, new_val);
                        return Ok(new_val);
                    }
                    _ => {}
                }
            }
        }

        self.parse_conditional()
    }

    fn parse_conditional(&mut self) -> Result<i64, ExecError> {
        let cond = self.parse_logical_or()?;
        if let Some(ArithToken::Question) = self.peek() {
            self.advance();
            let parent_evaluating = self.evaluating;
            self.evaluating = parent_evaluating && cond != 0;
            let true_result = self.parse_conditional();
            self.evaluating = parent_evaluating;
            let true_val = true_result?;
            if let Some(ArithToken::Colon) = self.peek() {
                self.advance();
                self.evaluating = parent_evaluating && cond == 0;
                let false_result = self.parse_conditional();
                self.evaluating = parent_evaluating;
                let false_val = false_result?;
                return Ok(if !parent_evaluating {
                    0
                } else if cond != 0 {
                    true_val
                } else {
                    false_val
                });
            } else {
                return Err(ExecError::Custom(
                    "Missing ':' in arithmetic conditional expression".to_string(),
                ));
            }
        }
        Ok(cond)
    }

    fn parse_logical_or(&mut self) -> Result<i64, ExecError> {
        let mut left = self.parse_logical_and()?;
        while let Some(ArithToken::LogOr) = self.peek() {
            self.advance();
            let parent_evaluating = self.evaluating;
            self.evaluating = parent_evaluating && left == 0;
            let right_result = self.parse_logical_and();
            self.evaluating = parent_evaluating;
            let right = right_result?;
            left = if !parent_evaluating {
                0
            } else if left != 0 || right != 0 {
                1
            } else {
                0
            };
        }
        Ok(left)
    }

    fn parse_logical_and(&mut self) -> Result<i64, ExecError> {
        let mut left = self.parse_bitwise_or()?;
        while let Some(ArithToken::LogAnd) = self.peek() {
            self.advance();
            let parent_evaluating = self.evaluating;
            self.evaluating = parent_evaluating && left != 0;
            let right_result = self.parse_bitwise_or();
            self.evaluating = parent_evaluating;
            let right = right_result?;
            left = if !parent_evaluating {
                0
            } else if left != 0 && right != 0 {
                1
            } else {
                0
            };
        }
        Ok(left)
    }

    fn parse_bitwise_or(&mut self) -> Result<i64, ExecError> {
        let mut left = self.parse_bitwise_xor()?;
        while let Some(ArithToken::BitOr) = self.peek() {
            self.advance();
            let right = self.parse_bitwise_xor()?;
            left |= right;
        }
        Ok(left)
    }

    fn parse_bitwise_xor(&mut self) -> Result<i64, ExecError> {
        let mut left = self.parse_bitwise_and()?;
        while let Some(ArithToken::BitXor) = self.peek() {
            self.advance();
            let right = self.parse_bitwise_and()?;
            left ^= right;
        }
        Ok(left)
    }

    fn parse_bitwise_and(&mut self) -> Result<i64, ExecError> {
        let mut left = self.parse_equality()?;
        while let Some(ArithToken::BitAnd) = self.peek() {
            self.advance();
            let right = self.parse_equality()?;
            left &= right;
        }
        Ok(left)
    }

    fn parse_equality(&mut self) -> Result<i64, ExecError> {
        let mut left = self.parse_relational()?;
        while let Some(tok) = self.peek() {
            match tok {
                ArithToken::Eq => {
                    self.advance();
                    let right = self.parse_relational()?;
                    left = if left == right { 1 } else { 0 };
                }
                ArithToken::Ne => {
                    self.advance();
                    let right = self.parse_relational()?;
                    left = if left != right { 1 } else { 0 };
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_relational(&mut self) -> Result<i64, ExecError> {
        let mut left = self.parse_shifts()?;
        while let Some(tok) = self.peek() {
            match tok {
                ArithToken::Lt => {
                    self.advance();
                    let right = self.parse_shifts()?;
                    left = if left < right { 1 } else { 0 };
                }
                ArithToken::Le => {
                    self.advance();
                    let right = self.parse_shifts()?;
                    left = if left <= right { 1 } else { 0 };
                }
                ArithToken::Gt => {
                    self.advance();
                    let right = self.parse_shifts()?;
                    left = if left > right { 1 } else { 0 };
                }
                ArithToken::Ge => {
                    self.advance();
                    let right = self.parse_shifts()?;
                    left = if left >= right { 1 } else { 0 };
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_shifts(&mut self) -> Result<i64, ExecError> {
        let mut left = self.parse_additive()?;
        while let Some(tok) = self.peek() {
            match tok {
                ArithToken::Shl => {
                    self.advance();
                    let right = self.parse_additive()?;
                    left = shift_value(left, right, true, self.evaluating)?;
                }
                ArithToken::Shr => {
                    self.advance();
                    let right = self.parse_additive()?;
                    left = shift_value(left, right, false, self.evaluating)?;
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_additive(&mut self) -> Result<i64, ExecError> {
        let mut left = self.parse_multiplicative()?;
        while let Some(tok) = self.peek() {
            match tok {
                ArithToken::Plus => {
                    self.advance();
                    let right = self.parse_multiplicative()?;
                    left = left.wrapping_add(right);
                }
                ArithToken::Minus => {
                    self.advance();
                    let right = self.parse_multiplicative()?;
                    left = left.wrapping_sub(right);
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> Result<i64, ExecError> {
        let mut left = self.parse_power()?;
        while let Some(tok) = self.peek() {
            match tok {
                ArithToken::Star => {
                    self.advance();
                    let right = self.parse_power()?;
                    left = left.wrapping_mul(right);
                }
                ArithToken::Slash => {
                    self.advance();
                    let right = self.parse_power()?;
                    left = div_values(left, right, self.evaluating)?;
                }
                ArithToken::Percent => {
                    self.advance();
                    let right = self.parse_power()?;
                    left = mod_values(left, right, self.evaluating)?;
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_power(&mut self) -> Result<i64, ExecError> {
        let left = self.parse_unary()?;
        if let Some(ArithToken::Power) = self.peek() {
            self.advance();
            let right = self.parse_power()?;
            if right >= 0 {
                Ok(left.wrapping_pow(right as u32))
            } else {
                Ok(0)
            }
        } else {
            Ok(left)
        }
    }

    fn parse_unary(&mut self) -> Result<i64, ExecError> {
        if let Some(tok) = self.peek() {
            match tok {
                ArithToken::Plus => {
                    self.advance();
                    self.parse_unary()
                }
                ArithToken::Minus => {
                    self.advance();
                    if matches!(self.peek(), Some(ArithToken::MinMagnitude)) {
                        self.advance();
                        return Ok(i64::MIN);
                    }
                    let val = self.parse_unary()?;
                    Ok(val.wrapping_neg())
                }
                ArithToken::Tilde => {
                    self.advance();
                    let val = self.parse_unary()?;
                    Ok(!val)
                }
                ArithToken::Bang => {
                    self.advance();
                    let val = self.parse_unary()?;
                    Ok(if val == 0 { 1 } else { 0 })
                }
                ArithToken::Increment => {
                    self.advance();
                    if let Some(ArithToken::Var(name)) = self.advance() {
                        let cur = self.get_var_val(&name)?;
                        let next = cur.wrapping_add(1);
                        self.set_var_val(&name, next);
                        Ok(next)
                    } else {
                        Err(ExecError::Custom(
                            "Expected variable name after '++'".to_string(),
                        ))
                    }
                }
                ArithToken::Decrement => {
                    self.advance();
                    if let Some(ArithToken::Var(name)) = self.advance() {
                        let cur = self.get_var_val(&name)?;
                        let next = cur.wrapping_sub(1);
                        self.set_var_val(&name, next);
                        Ok(next)
                    } else {
                        Err(ExecError::Custom(
                            "Expected variable name after '--'".to_string(),
                        ))
                    }
                }
                _ => self.parse_postfix(),
            }
        } else {
            Err(ExecError::Custom(
                "Expected operand in arithmetic expression".to_string(),
            ))
        }
    }

    fn parse_postfix(&mut self) -> Result<i64, ExecError> {
        let mut val = self.parse_primary()?;
        if let Some(tok) = self.peek() {
            match tok {
                ArithToken::Increment => {
                    // Check if previous primary was a variable
                    if self.pos > 0
                        && let ArithToken::Var(name) = &self.tokens[self.pos - 1]
                    {
                        let name = name.clone();
                        self.advance();
                        let cur = val;
                        let next = cur.wrapping_add(1);
                        self.set_var_val(&name, next);
                        val = cur; // Postfix returns old value
                    }
                }
                ArithToken::Decrement => {
                    if self.pos > 0
                        && let ArithToken::Var(name) = &self.tokens[self.pos - 1]
                    {
                        let name = name.clone();
                        self.advance();
                        let cur = val;
                        let next = cur.wrapping_sub(1);
                        self.set_var_val(&name, next);
                        val = cur; // Postfix returns old value
                    }
                }
                _ => {}
            }
        }
        Ok(val)
    }

    fn parse_primary(&mut self) -> Result<i64, ExecError> {
        if let Some(tok) = self.advance() {
            match tok {
                ArithToken::Num(n) => Ok(n),
                ArithToken::MinMagnitude => Err(ExecError::Custom(
                    "Integer literal is out of range in arithmetic expression".to_string(),
                )),
                ArithToken::Var(name) => self.get_var_val(&name),
                ArithToken::LParen => {
                    let val = self.parse_expression()?;
                    if let Some(ArithToken::RParen) = self.peek() {
                        self.advance();
                        Ok(val)
                    } else {
                        Err(ExecError::Custom(
                            "Unclosed '(' in arithmetic expression".to_string(),
                        ))
                    }
                }
                other => Err(ExecError::Custom(format!(
                    "Unexpected token in arithmetic expression: {:?}",
                    other
                ))),
            }
        } else {
            Ok(0)
        }
    }
}

pub fn eval_arithmetic_expr(expr: &str, ctx: &mut ExecutionContext) -> Result<i64, ExecError> {
    eval_arithmetic_expr_with_stack(expr, ctx, &mut Vec::new())
}

fn eval_arithmetic_expr_with_stack(
    expr: &str,
    ctx: &mut ExecutionContext,
    resolving: &mut Vec<String>,
) -> Result<i64, ExecError> {
    let tokens = tokenize_arith(expr)?;
    if tokens.is_empty() {
        return Ok(0);
    }
    let mut parser = ArithParser::new(tokens, ctx, resolving);
    let result = parser.parse_expression()?;
    if parser.pos != parser.tokens.len() {
        return Err(ExecError::Custom(
            "Unexpected trailing token in arithmetic expression".to_string(),
        ));
    }
    Ok(result)
}

fn div_values(left: i64, right: i64, evaluating: bool) -> Result<i64, ExecError> {
    if !evaluating {
        return Ok(0);
    }
    if right == 0 {
        return Err(ExecError::Custom(
            "Division by zero in arithmetic expression".to_string(),
        ));
    }
    Ok(if left == i64::MIN && right == -1 {
        i64::MIN
    } else {
        left / right
    })
}

fn shift_value(
    value: i64,
    count: i64,
    left_shift: bool,
    evaluating: bool,
) -> Result<i64, ExecError> {
    if !evaluating {
        return Ok(0);
    }
    if !(0..i64::BITS as i64).contains(&count) {
        return Err(ExecError::Custom(format!(
            "Shift count out of range in arithmetic expression: {count}"
        )));
    }
    Ok(if left_shift {
        value.wrapping_shl(count as u32)
    } else {
        value.wrapping_shr(count as u32)
    })
}

fn mod_values(left: i64, right: i64, evaluating: bool) -> Result<i64, ExecError> {
    if !evaluating {
        return Ok(0);
    }
    if right == 0 {
        return Err(ExecError::Custom(
            "Division by zero in arithmetic expression".to_string(),
        ));
    }
    Ok(if left == i64::MIN && right == -1 {
        0
    } else {
        left % right
    })
}
