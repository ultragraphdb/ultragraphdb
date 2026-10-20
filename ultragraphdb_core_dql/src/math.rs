/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, dql/math.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! Math expressions, `math(a + exp(b) / 2)`, parsed by operator precedence into a [`MathTree`].

use ultragraphdb_core_types::{TypeId, Val, Value};

use crate::errors::dql_error::DqlError;
use crate::parser::It;
use crate::parser::parse_var_name;
use crate::parser::variables::{VarMap, parse_value};
use crate::state::{ITEM_COMMA, ITEM_DOLLAR, ITEM_LEFT_ROUND, ITEM_NAME, ITEM_RIGHT_ROUND};
use crate::types::math_tree::MathTree;

fn math_op_precedence(op: &str) -> i32 {
    match op {
        "u-" => 500,
        "floor" => 105,
        "ceil" => 104,
        "since" => 103,
        "exp" => 100,
        "ln" => 99,
        "sqrt" => 98,
        "cond" => 90,
        "pow" => 89,
        "logbase" => 88,
        "max" => 85,
        "min" => 84,
        // "/" and "*" share a precedence, so that 5 * 10 / 50 is (5*10)/50, which integer arithmetic needs.
        "/" | "*" => 50,
        // dot binds less than "/" and "*", so that c1 * v1 dot c2 * v2 is (c1 * v1) dot (c2 * v2).
        "dot" => 49,
        "%" => 48,
        "-" => 47,
        "+" => 46,
        "<" => 10,
        ">" => 9,
        "<=" => 8,
        ">=" => 7,
        "==" => 6,
        "!=" => 5,
        _ => 0,
    }
}

fn is_unary(f: &str) -> bool {
    matches!(f, "exp" | "ln" | "u-" | "sqrt" | "floor" | "ceil" | "since")
}

fn is_binary_math(f: &str) -> bool {
    matches!(f, "*" | "+" | "-" | "/" | "%")
}

fn is_ternary(f: &str) -> bool {
    f == "cond"
}

fn is_zero(f: &str, rval: &Val) -> bool {
    match (rval.tid(), rval.value()) {
        (TypeId::FLOAT, Some(Value::Float(g))) => match f {
            "floor" => *g >= 0.0 && *g < 1.0,
            "/" | "%" | "ceil" | "sqrt" | "u-" => *g == 0.0,
            "ln" => *g == 1.0,
            _ => false,
        },
        (TypeId::INT, Some(Value::Int(g))) => match f {
            "floor" | "/" | "%" | "ceil" | "sqrt" | "u-" => *g == 0,
            "ln" => *g == 1,
            _ => false,
        },
        _ => false,
    }
}

fn eval_math_stack(
    op_stack: &mut Vec<MathTree>,
    value_stack: &mut Vec<MathTree>,
) -> Result<(), DqlError> {
    let mut top_op = op_stack
        .pop()
        .ok_or_else(|| DqlError::parse("Invalid Math expression"))?;
    if is_unary(&top_op.fn_name) {
        let top_val = value_stack
            .pop()
            .ok_or_else(|| DqlError::parse("Invalid math statement. Expected 1 operands"))?;
        if op_stack.len() > 1 {
            let peek = op_stack
                .last()
                .map(|t| t.fn_name.as_str())
                .unwrap_or_default();
            if (peek == "/" || peek == "%") && is_zero(&top_op.fn_name, &top_val.const_val) {
                return Err(DqlError::parse("Division by zero"));
            }
        }
        top_op.child = vec![top_val];
    } else if is_ternary(&top_op.fn_name) {
        if value_stack.len() < 3 {
            return Err(DqlError::parse(
                "Invalid Math expression. Expected 3 operands",
            ));
        }
        let split = value_stack.len() - 3;
        top_op.child = value_stack.split_off(split);
    } else {
        if value_stack.len() < 2 {
            return Err(DqlError::parse(
                "Invalid Math expression. Expected 2 operands",
            ));
        }
        if value_stack
            .last()
            .is_some_and(|v| is_zero(&top_op.fn_name, &v.const_val))
        {
            return Err(DqlError::parse("Division by zero."));
        }
        let split = value_stack.len() - 2;
        top_op.child = value_stack.split_off(split);
    }
    // Push the new value (tree) into the valueStack.
    value_stack.push(top_op);
    Ok(())
}

fn is_math_func(f: &str) -> bool {
    // While adding an op, also add it to the corresponding function type.
    matches!(
        f,
        "*" | "%"
            | "+"
            | "-"
            | "/"
            | "exp"
            | "ln"
            | "cond"
            | "<"
            | ">"
            | ">="
            | "<="
            | "=="
            | "!="
            | "min"
            | "max"
            | "sqrt"
            | "pow"
            | "logbase"
            | "floor"
            | "ceil"
            | "since"
            | "dot"
    )
}

/// Parses the arguments of a math function (or the expression of `math`) whose name is the current item; with
/// `again`, a further argument after a comma. Whether a comma ended it, so another argument follows.
pub(crate) fn parse_math_func(
    it: &mut It<'_, '_>,
    again: bool,
) -> Result<(MathTree, bool), DqlError> {
    if !again {
        it.next();
        if it.item().typ() != ITEM_LEFT_ROUND {
            return Err(DqlError::parse("Expected ( after math"));
        }
    }
    // opStack is used to collect the operators in right order.
    let mut op_stack = vec![MathTree::with_fn("(")];
    // valueStack is used to collect the values.
    let mut value_stack: Vec<MathTree> = Vec::new();

    while it.next() {
        let item = it.item();
        let lval = item.val().to_lowercase();
        if is_math_func(&lval) {
            let mut op = lval;
            it.prev();
            let last_item = it.item();
            it.next();
            if op == "-"
                && (last_item.val() == "("
                    || last_item.val() == ","
                    || is_binary_math(last_item.val()))
            {
                op = "u-".to_string(); // This is a unary -
            }
            let op_pred = math_op_precedence(&op);
            // Evaluate the stack until we see an operator with strictly lower pred.
            while let Some(top_op) = op_stack.last() {
                if math_op_precedence(&top_op.fn_name) < op_pred {
                    break;
                }
                eval_math_stack(&mut op_stack, &mut value_stack)?;
            }
            op_stack.push(MathTree::with_fn(&op)); // Push current operator.
            if it.peek(1)?[0].typ() == ITEM_LEFT_ROUND {
                parse_math_args(it, &mut value_stack)?;
            }
        } else if item.typ() == ITEM_NAME {
            // Value.
            if it.peek(1)?[0].typ() == ITEM_LEFT_ROUND {
                if !is_math_func(item.val()) {
                    return Err(DqlError::parse(format!(
                        "Unknown math function: {}",
                        item.val()
                    )));
                }
                parse_math_args(it, &mut value_stack)?;
                continue;
            }
            // We will try to parse the constant as an Int first, if that fails we move to float.
            let mut child = MathTree::default();
            if let Ok(i) = crate::utils::go_strconv::parse_int(item.val(), 10) {
                child.const_val = Val::new(TypeId::INT, Value::Int(i));
            } else if let Ok(v) = crate::utils::go_strconv::parse_float(item.val()) {
                child.const_val = Val::new(TypeId::FLOAT, Value::Float(v));
            } else {
                child.var = item.val().to_string();
            }
            value_stack.push(child);
        } else if item.typ() == ITEM_LEFT_ROUND {
            // Just push to op stack.
            op_stack.push(MathTree::with_fn("("));
        } else if item.typ() == ITEM_COMMA || item.typ() == ITEM_RIGHT_ROUND {
            // Pop op stack until we see a (.
            while let Some(top_op) = op_stack.last() {
                if top_op.fn_name == "(" {
                    break;
                }
                eval_math_stack(&mut op_stack, &mut value_stack)?;
            }
            // Pop away the (.
            if op_stack.pop().is_none() {
                return Err(DqlError::parse("Invalid Math expression"));
            }
            if item.typ() == ITEM_COMMA {
                if !op_stack.is_empty() {
                    return Err(DqlError::parse("Invalid math expression."));
                }
                if value_stack.len() != 1 {
                    return Err(DqlError::parse(format!(
                        "Expected one item in value stack, but got {}",
                        value_stack.len()
                    )));
                }
                return value_stack
                    .pop()
                    .map(|res| (res, true))
                    .ok_or_else(|| DqlError::parse("Empty stack"));
            }
            if op_stack.is_empty() {
                // The parentheses are balanced out. Let's break.
                break;
            }
        } else if item.typ() == ITEM_DOLLAR {
            let var = parse_var_name(it)?;
            value_stack.push(MathTree {
                var,
                ..MathTree::default()
            });
        } else {
            return Err(DqlError::parse(format!(
                "Unexpected item while parsing math expression: {item}"
            )));
        }
    }

    // For math expressions, we start with ( and end with ). We expect to break out of the loop when the
    // parentheses balance off, and at that point, opStack should be empty. Go asserts it; here it is an error.
    if !op_stack.is_empty() {
        return Err(DqlError::parse("Op stack should be empty when we exit"));
    }
    if value_stack.is_empty() {
        // This happens when we have math().
        return Err(DqlError::parse("Empty () not allowed in math block."));
    }
    if value_stack.len() != 1 {
        return Err(DqlError::parse(format!(
            "Expected one item in value stack, but got {}",
            value_stack.len()
        )));
    }
    value_stack
        .pop()
        .map(|res| (res, false))
        .ok_or_else(|| DqlError::parse("Empty stack"))
}

/// Parses the comma-separated arguments of a nested math function onto the value stack.
fn parse_math_args(it: &mut It<'_, '_>, value_stack: &mut Vec<MathTree>) -> Result<(), DqlError> {
    let mut again = false;
    loop {
        let (child, more) = parse_math_func(it, again)?;
        again = more;
        value_stack.push(child);
        if !again {
            return Ok(());
        }
    }
}

/// Substitutes the query parameters of a math expression with their typed values.
pub(crate) fn subs(t: &mut MathTree, vmap: &VarMap) -> Result<(), DqlError> {
    if t.var.starts_with('$') {
        let va = vmap
            .get(&t.var)
            .ok_or_else(|| DqlError::parse("Variable not found in math"))?;
        t.const_val = parse_value(va)?;
        t.var = String::new();
    }
    for i in &mut t.child {
        subs(i, vmap)?;
    }
    Ok(())
}
