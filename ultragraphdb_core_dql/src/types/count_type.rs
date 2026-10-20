/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, dql/parser.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// Where the parser is in a `count(...)` among a block's children.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum CountType {
    /// The default.
    #[default]
    NotSeen,
    /// The `count` keyword was seen.
    Seen,
    /// A predicate was seen within count.
    SeenWithPred,
}
