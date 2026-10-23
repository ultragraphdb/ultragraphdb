/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/index.go (indexOp). Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// What a schema change does to one of a predicate's indexes: Dgraph's `indexOp`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IndexOp {
    /// The index is left alone.
    Noop,
    /// The index is deleted.
    Delete,
    /// The index is deleted and built again.
    Rebuild,
}
