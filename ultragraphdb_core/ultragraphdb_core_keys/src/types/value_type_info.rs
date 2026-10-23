/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, x/values.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// The type of a value inside a DirectedEdge, Posting or N-Quad, with Go's `ValueTypeInfo` discriminants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(i32)]
pub enum ValueTypeInfo {
    /// An unknown type of value.
    #[default]
    Unknown = 0,
    /// A value with no UID and no value.
    Empty = 1,
    /// A value with an UID.
    Uid = 2,
    /// A plain old value without defined language tag.
    Plain = 3,
    /// A value which is part of a multi-value posting list (like language).
    Multi = 4,
}
