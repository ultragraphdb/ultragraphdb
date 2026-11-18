/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::Bucket;

use crate::types::claim::Claim;

/// Where a request of a transaction goes, as its subjects say.
#[derive(Debug, Clone)]
pub(crate) enum Destination {
    /// It names these subjects' buckets, in order.
    Named(Vec<Bucket>),
    /// It names only new blank nodes: a partition in turn, which draws them from its own buckets.
    InTurn,
    /// It claims a value, naming no subject but new blank nodes: the claim's partition, once known.
    Claim(Claim),
    /// It names no subject: the partition owning the tenant's whole run.
    Run,
}
