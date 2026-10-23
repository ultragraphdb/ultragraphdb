/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, dql/parser.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// The kind of a variable a query needs: Go's `AnyVar`, `UidVar`, `ValueVar` and `ListVar`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(i32)]
pub enum VarType {
    /// `AnyVar`.
    #[default]
    Any = 0,
    /// `UidVar`: a uid variable.
    Uid = 1,
    /// `ValueVar`: a value variable.
    Value = 2,
    /// `ListVar`: the predicates `expand(val(a))` lists.
    List = 3,
}

impl VarType {
    /// Go's integer constant.
    pub fn value(self) -> i32 {
        self as i32
    }
}
