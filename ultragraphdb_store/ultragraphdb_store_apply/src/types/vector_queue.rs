/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// What the vector indexer of one predicate has before it: the vectors the rounds applied since its last pass wrote or
/// removed, as their commit version, graph and node, in commit order, and the version its last pass indexed through.
#[derive(Debug, Default)]
pub(crate) struct VectorQueue {
    pub pending: Vec<(u64, u64, u64)>,
    pub indexed: u64,
}
