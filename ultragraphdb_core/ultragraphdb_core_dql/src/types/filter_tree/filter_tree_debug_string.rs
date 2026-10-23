/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, dql/parser.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::errors::dql_error::DqlError;

use super::FilterTree;

impl FilterTree {
    /// The tree as text, `(AND (eq name "a") (NOT (has age)))`: Go's `debugString`, for tests and debugging.
    ///
    /// # Errors
    ///
    /// [`DqlErrorEnum::UnknownOperator`](crate::DqlErrorEnum::UnknownOperator) on an inner node whose operator is
    /// not `and`, `or` or `not`, where Go's `x.Fatalf` exits.
    pub fn debug_string(&self) -> Result<String, DqlError> {
        /// A node to write, or text to write after a node's children.
        enum Piece<'t> {
            Node(&'t FilterTree),
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
                    match node.op.as_str() {
                        "and" => buf.push_str("AND"),
                        "or" => buf.push_str("OR"),
                        "not" => buf.push_str("NOT"),
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

    /// Writes the node when it is a leaf, a function; whether it was.
    fn write_leaf(&self, buf: &mut String) -> bool {
        let Some(f) = self.func.as_ref().filter(|f| !f.name.is_empty()) else {
            return false;
        };
        buf.push('(');
        buf.push_str(&f.name);
        if !f.attr.is_empty() {
            buf.push(' ');
            if f.is_count {
                buf.push_str("count(");
            } else if f.is_value_var {
                buf.push_str("val(");
            } else if f.is_len_var {
                buf.push_str("len(");
            }
            buf.push_str(&f.attr);
            if f.is_count || f.is_value_var || f.is_len_var {
                buf.push(')');
            }
            if !f.lang.is_empty() {
                buf.push('@');
                buf.push_str(&f.lang);
            }
            for arg in &f.args {
                buf.push_str(if arg.is_value_var { " val(" } else { " \"" });
                buf.push_str(&arg.value);
                buf.push(if arg.is_value_var { ')' } else { '"' });
            }
        }
        buf.push(')');
        true
    }
}
