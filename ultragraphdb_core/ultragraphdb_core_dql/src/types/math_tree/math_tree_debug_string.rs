/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, dql/math.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_common_utils::go_strconv;
use ultragraphdb_core_types::{TypeId, Value};

use crate::errors::dql_error::DqlError;

use super::MathTree;

impl MathTree {
    /// The tree as text, `(+ a (exp 1E+00))`: Go's `debugString`, for tests and debugging.
    ///
    /// # Errors
    ///
    /// [`DqlErrorEnum::UnknownOperator`](crate::DqlErrorEnum::UnknownOperator) on an inner node whose operator Go's
    /// `debugString` does not know, among them `floor`, `ceil` and `since`, which parse; Go's `x.Fatalf` exits.
    pub fn debug_string(&self) -> Result<String, DqlError> {
        /// A node to write, or text to write after a node's children.
        enum Piece<'t> {
            Node(&'t MathTree),
            Text(&'static str),
        }
        let mut buf = String::with_capacity(20);
        // Go's recursive stringHelper, with the nodes and closing text waiting on a stack.
        let mut stack = vec![Piece::Node(self)];
        while let Some(piece) = stack.pop() {
            match piece {
                Piece::Text(text) => buf.push_str(text),
                Piece::Node(node) => {
                    if node.write_leaf(&mut buf) {
                        continue;
                    }
                    // Non-leaf node.
                    buf.push('(');
                    match node.fn_name.as_str() {
                        "+" | "-" | "/" | "*" | "%" | "exp" | "ln" | "cond" | "min" | "sqrt"
                        | "max" | "<" | ">" | "<=" | ">=" | "==" | "!=" | "u-" | "logbase"
                        | "pow" | "dot" => buf.push_str(&node.fn_name),
                        op => return Err(DqlError::UnknownOperator(op.to_string())),
                    }
                    stack.push(Piece::Text(")"));
                    for c in node.child.iter().rev() {
                        stack.push(Piece::Node(c));
                        stack.push(Piece::Text(" "));
                    }
                }
            }
        }
        Ok(buf)
    }

    /// Writes the node when it is a leaf, a variable or a constant; whether it was.
    fn write_leaf(&self, buf: &mut String) -> bool {
        if !self.var.is_empty() {
            buf.push_str(&self.var);
            return true;
        }
        let Some(v) = self.const_val.value() else {
            return false;
        };
        match (self.const_val.tid(), v) {
            (TypeId::FLOAT, Value::Float(f)) => {
                buf.push_str(&go_strconv::format_float(*f, b'E', 64))
            }
            (TypeId::INT, Value::Int(i)) => buf.push_str(&i.to_string()),
            _ => {}
        }
        true
    }
}
