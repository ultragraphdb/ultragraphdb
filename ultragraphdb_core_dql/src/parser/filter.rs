/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, dql/parser.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! The `@filter` directive: functions combined with `and`, `or`, `not` and brackets.

use super::function::parse_function;
use super::{It, ierr};
use crate::errors::dql_error::DqlError;
use crate::state::{ITEM_LEFT_CURL, ITEM_LEFT_ROUND, ITEM_NAME, ITEM_RIGHT_CURL, ITEM_RIGHT_ROUND};
use crate::types::filter_tree::FilterTree;

fn filter_op_precedence(op: &str) -> i32 {
    match op {
        "not" => 3,
        "and" => 2,
        "or" => 1,
        _ => 0,
    }
}

fn eval_stack(
    op_stack: &mut Vec<FilterTree>,
    value_stack: &mut Vec<FilterTree>,
) -> Result<(), DqlError> {
    let invalid = || DqlError::parse("Invalid filter statement");
    let mut top_op = op_stack.pop().ok_or_else(invalid)?;
    if top_op.op == "not" {
        // Since "not" is a unary operator, just pop one value.
        let top_val = value_stack.pop().ok_or_else(invalid)?;
        top_op.child = vec![top_val];
    } else {
        // "and" and "or" are binary operators, so pop two values.
        if value_stack.len() < 2 {
            return Err(invalid());
        }
        let top_val1 = value_stack.pop().ok_or_else(invalid)?;
        let top_val2 = value_stack.pop().ok_or_else(invalid)?;
        top_op.child = vec![top_val2, top_val1];
    }
    // Push the new value (tree) into the valueStack.
    value_stack.push(top_op);
    Ok(())
}

/// Parses the filter directive into a tree; `None` for `@filter()`.
pub(crate) fn parse_filter(it: &mut It<'_, '_>) -> Result<Option<FilterTree>, DqlError> {
    it.next();
    let item = it.item();
    if item.typ() != ITEM_LEFT_ROUND {
        return Err(ierr(item, "Expected ( after filter directive"));
    }
    // opStack is used to collect the operators in right order.
    let mut op_stack = vec![FilterTree::with_op("(")];
    // valueStack is used to collect the values.
    let mut value_stack: Vec<FilterTree> = Vec::new();

    while it.next() {
        let inner = it.item();
        let lval = inner.val().to_lowercase();
        if lval == "and" || lval == "or" || lval == "not" {
            // Handle operators.
            let op_pred = filter_op_precedence(&lval);
            // Evaluate the stack until we see an operator with strictly lower pred.
            while let Some(top_op) = op_stack.last() {
                if filter_op_precedence(&top_op.op) < op_pred {
                    break;
                }
                eval_stack(&mut op_stack, &mut value_stack)?;
            }
            op_stack.push(FilterTree::with_op(&lval)); // Push current operator.
        } else if inner.typ() == ITEM_NAME {
            // Value.
            it.prev();
            let f = parse_function(it, None)?;
            value_stack.push(FilterTree {
                func: Some(f),
                ..FilterTree::default()
            });
        } else if inner.typ() == ITEM_LEFT_ROUND {
            // Just push to op stack.
            op_stack.push(FilterTree::with_op("("));
        } else if inner.typ() == ITEM_RIGHT_ROUND {
            // Pop op stack until we see a (.
            while let Some(top_op) = op_stack.last() {
                if top_op.op == "(" {
                    break;
                }
                eval_stack(&mut op_stack, &mut value_stack)?;
            }
            // Pop away the (.
            if op_stack.pop().is_none() {
                return Err(ierr(inner, "Invalid filter statement"));
            }
            if op_stack.is_empty() {
                // The parentheses are balanced out. Let's break.
                break;
            }
        } else if inner.typ() == ITEM_LEFT_CURL {
            return Err(ierr(
                inner,
                "Unrecognized character inside a func: U+007B '{'",
            ));
        } else if inner.typ() == ITEM_RIGHT_CURL {
            return Err(ierr(
                inner,
                "Unrecognized character inside a func: U+007D '}'",
            ));
        } else {
            return Err(ierr(
                inner,
                format!("Unexpected item while parsing @filter: {inner}"),
            ));
        }
    }

    // For filters, we start with ( and end with ). We expect to break out of the loop when the parentheses
    // balance off, and at that point, opStack should be empty.
    if !op_stack.is_empty() {
        return Err(ierr(item, "Unbalanced parentheses in @filter statement"));
    }
    if value_stack.is_empty() {
        // This happens when we have @filter(). Ignore it and pretend there is no filter.
        return Ok(None);
    }
    if value_stack.len() != 1 {
        return Err(ierr(
            item,
            format!(
                "Expected one item in value stack, but got {}",
                value_stack.len()
            ),
        ));
    }
    Ok(value_stack.pop())
}
