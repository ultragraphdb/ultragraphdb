/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, chunker/json_parser.go (SetNquads, DeleteNquads). Changed for ultragraphdb:
 * translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// Whether parsed JSON N-Quads are to be added or deleted, Dgraph's `SetNquads` and `DeleteNquads`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JsonOp {
    /// The N-Quads are added.
    Set,
    /// The N-Quads are deleted: every object names its uid, a `null` value deletes the predicate's values, and an
    /// object holding only its uid deletes the node's every predicate.
    Delete,
}
