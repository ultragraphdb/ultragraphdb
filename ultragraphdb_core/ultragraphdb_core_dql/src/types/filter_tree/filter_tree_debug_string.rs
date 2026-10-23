/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, dql/parser.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use super::FilterTree;

impl FilterTree {
    /// The tree as text, `(AND (eq name "a") (NOT (has age)))`: Go's `debugString`, for tests and debugging.
    ///
    /// # Panics
    ///
    /// On an inner node whose operator is not `and`, `or` or `not`, as Go's `x.Fatalf`.
    pub fn debug_string(&self) -> String {
        let mut buf = String::with_capacity(20);
        self.string_helper(&mut buf);
        buf
    }

    fn string_helper(&self, buf: &mut String) {
        if let Some(f) = self.func.as_ref().filter(|f| !f.name.is_empty()) {
            // Leaf node.
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
            return;
        }
        // Non-leaf node.
        buf.push('(');
        match self.op.as_str() {
            "and" => buf.push_str("AND"),
            "or" => buf.push_str("OR"),
            "not" => buf.push_str("NOT"),
            op => panic!("Unknown operator: {op:?}"),
        }
        for c in &self.child {
            buf.push(' ');
            c.string_helper(buf);
        }
        buf.push(')');
    }
}
