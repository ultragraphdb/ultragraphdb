/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, dql/math.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_types::{TypeId, Value};

use super::MathTree;
use crate::utils::go_strconv::format_float_upper_e;

impl MathTree {
    /// The tree as text, `(+ a (exp 1E+00))`: Go's `debugString`, for tests and debugging.
    ///
    /// # Panics
    ///
    /// On an inner node whose operator Go's `debugString` does not know, as Go's `x.Fatalf`.
    pub fn debug_string(&self) -> String {
        let mut buf = String::with_capacity(20);
        self.string_helper(&mut buf);
        buf
    }

    fn string_helper(&self, buf: &mut String) {
        if !self.var.is_empty() {
            // Leaf node.
            buf.push_str(&self.var);
            return;
        }
        if let Some(v) = self.const_val.value() {
            // Leaf node.
            match (self.const_val.tid(), v) {
                (TypeId::FLOAT, Value::Float(f)) => buf.push_str(&format_float_upper_e(*f)),
                (TypeId::INT, Value::Int(i)) => buf.push_str(&i.to_string()),
                _ => {}
            }
            return;
        }
        // Non-leaf node.
        buf.push('(');
        match self.fn_name.as_str() {
            "+" | "-" | "/" | "*" | "%" | "exp" | "ln" | "cond" | "min" | "sqrt" | "max" | "<"
            | ">" | "<=" | ">=" | "==" | "!=" | "u-" | "logbase" | "pow" | "dot" => {
                buf.push_str(&self.fn_name)
            }
            op => panic!("Unknown operator: {op:?}"),
        }
        for c in &self.child {
            buf.push(' ');
            c.string_helper(buf);
        }
        buf.push(')');
    }
}
