/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod applier_operator_operator;

use ultragraphdb_cluster_placement::{PartitionId, PlacementMap};
use ultragraphdb_store_apply::ChangesetApplier;
use ultragraphdb_store_posting::Batch;

/// The operator of a node's commit step: commit-time operations run on the changeset applier (7.5), each against the
/// store at the watermark with an overlay of the decided but unapplied proposals' writes above it, the round in flight's
/// and the current round's, in round order. The overlay is the posting lists' stacked layers ([`Batch::stack`]), built
/// when a round's first operation runs and dropped when the next round begins, so a round without operations stacks
/// nothing and allocates nothing for them.
///
/// **Claims.** An insert-if-absent or upsert-and-link claims its value at the partition owning the value's bucket
/// (`ultragraphdb_core_ids::Bucket::of_value`); bound to a partition ([`ApplierOperator::bound`]), the operator refuses a
/// claim another partition owns as cross-partition, which aborts the transaction, writing nothing. Unbound, it claims
/// every value locally, as one partition does.
#[derive(Debug)]
pub struct ApplierOperator<'a, S, C, E, B> {
    applier: &'a ChangesetApplier<S, C, E, B>,
    overlay: Option<Batch<'a, S, C>>,
    binding: Option<(PartitionId, &'a PlacementMap)>,
}

impl<'a, S, C, E, B> ApplierOperator<'a, S, C, E, B> {
    /// An operator over `applier`, with no overlay built, claiming every value locally.
    pub fn new(applier: &'a ChangesetApplier<S, C, E, B>) -> Self {
        Self {
            applier,
            overlay: None,
            binding: None,
        }
    }

    /// The operator of `partition`, which owns the buckets `placement` gives it: a claim of a value whose bucket another
    /// partition owns is refused, cross-partition.
    pub fn bound(self, partition: PartitionId, placement: &'a PlacementMap) -> Self {
        Self {
            binding: Some((partition, placement)),
            ..self
        }
    }

    /// The changeset applier the operations read and write through.
    pub fn applier(&self) -> &'a ChangesetApplier<S, C, E, B> {
        self.applier
    }

    /// Whether the round's overlay is built: an operation ran in it.
    pub fn has_overlay(&self) -> bool {
        self.overlay.is_some()
    }
}
