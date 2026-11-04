/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod partition_referrer;

use ultragraphdb_cluster_placement::{NamespaceRun, PartitionId, PlacementMap};
use ultragraphdb_core_ids::{Bucket, Epoch, Namespace};
use ultragraphdb_store_apply::ChangesetApplier;
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_store_quads::OwnedBuckets;
use ultragraphdb_traits_blocks::BlockSource;
use ultragraphdb_traits_store::{Executor, ListCache, Store};
use ultragraphdb_transactions_txn::{Mode, Transaction, TxnBuffers, TxnLimits};

use crate::errors::partition_error::PartitionError;

/// One partition of a storage process (partitions, "A storage process hosts partitions over bucket ranges"): the
/// bucket ranges the placement map gives it, its changeset applier over its own store, drawing its blank nodes among
/// its buckets, and the placement map it routes by. Its log and commit step are a [`ultragraphdb_transactions_commit::
/// Pipeline`] over its applier; the code is the same whether a process hosts one partition or many.
///
/// Every request is checked against the partition's buckets before it reaches the commit step: a key outside them is
/// refused naming its owner ([`Partition::route`]). A read-write transaction begun here is bound to the partition
/// ([`Partition::begin`]): whatever it reads or writes beyond the partition aborts it, cross-partition, and its
/// references to other partitions' entities register at their homes first ([`Partition::references`],
/// [`Partition::adopt`]).
#[derive(Debug)]
pub struct Partition<S, C, E, B> {
    id: PartitionId,
    placement: PlacementMap,
    applier: ChangesetApplier<S, C, E, B>,
}

impl<S, C, E, B> Partition<S, C, E, B>
where
    S: Store + Send + Sync + 'static,
    C: ListCache<Value = CachedList> + Send + Sync + 'static,
    B: BlockSource + Send + Sync + 'static,
    E: Executor,
{
    /// The partition `id` of `placement`, writing through `applier`, whose quad store owns the partition's buckets
    /// ([`Partition::owned`]).
    pub fn new(
        id: PartitionId,
        placement: PlacementMap,
        applier: ChangesetApplier<S, C, E, B>,
    ) -> Self {
        Self {
            id,
            placement,
            applier,
        }
    }

    /// The buckets `placement` gives partition `id` at `epoch`, which its quad store owns; `None` for a layout whose
    /// ranges are not sorted and disjoint, which a placement map never holds.
    pub fn owned(id: PartitionId, placement: &PlacementMap, epoch: Epoch) -> Option<OwnedBuckets> {
        let ranges = placement
            .ranges_at(epoch)
            .into_iter()
            .filter(|(_, owner)| *owner == id)
            .map(|(range, _)| {
                // A range holds at least one bucket, every one below 2^24.
                Some((
                    Bucket::new(range.start()).ok()?,
                    Bucket::new(range.end().checked_sub(1)?).ok()?,
                ))
            })
            .collect::<Option<Vec<(Bucket, Bucket)>>>()?;
        OwnedBuckets::new(&ranges)
    }

    /// The partition's ID.
    pub fn id(&self) -> PartitionId {
        self.id
    }

    /// The placement map the partition routes by.
    pub fn placement(&self) -> &PlacementMap {
        &self.placement
    }

    /// The partition's changeset applier.
    pub fn applier(&self) -> &ChangesetApplier<S, C, E, B> {
        &self.applier
    }

    /// Checks that `bucket` is the partition's at `epoch`.
    ///
    /// # Errors
    ///
    /// [`PartitionError::wrong_partition`] naming the bucket and its owner (partitions, "Requests reach the partition
    /// of their bucket").
    pub fn route(&self, bucket: Bucket, epoch: Epoch) -> Result<(), PartitionError> {
        let owner = self.placement.partition_of(bucket, epoch);
        if owner == self.id {
            Ok(())
        } else {
            Err(PartitionError::wrong_partition(bucket.get(), owner.get()))
        }
    }

    /// Whether the partition owns every bucket of `namespace`'s run at `epoch`, as it always does alone.
    pub fn owns_run(&self, namespace: u64, epoch: Epoch) -> bool {
        self.lowest_other(Some(namespace), epoch).is_none()
    }

    /// The lowest other partition holding a bucket of `namespace`'s run at `epoch`, or of any run for `None`.
    fn lowest_other(&self, namespace: Option<u64>, epoch: Epoch) -> Option<PartitionId> {
        let run = namespace.map(|n| NamespaceRun::of(Namespace::new(n)).range());
        self.placement
            .ranges_at(epoch)
            .into_iter()
            .filter(|(range, _)| {
                run.is_none_or(|run| range.start() < run.end() && run.start() < range.end())
            })
            .map(|(_, owner)| owner)
            .filter(|owner| *owner != self.id)
            .min()
    }

    /// Begins a transaction of `namespace` on the partition at `now_us`, as [`Transaction::begin`] does, bound to the
    /// partition at its read epoch.
    ///
    /// # Errors
    ///
    /// The transaction's refusals of [`Transaction::begin`].
    pub fn begin(
        &self,
        namespace: u64,
        mode: Mode,
        read_version: u64,
        now_us: u64,
        limits: TxnLimits,
        buffers: TxnBuffers,
    ) -> Result<Transaction<'_, S, C, E, B>, PartitionError> {
        let begun = Transaction::begin(
            &self.applier,
            namespace,
            mode,
            read_version,
            now_us,
            limits,
            buffers,
        )?;
        Ok(begun.bind(self.id, &self.placement))
    }

    /// Checks a Drop at `epoch`: of `namespace`'s data, a predicate or a graph of it, or, for `None`, of everything.
    ///
    /// # Errors
    ///
    /// [`PartitionError::cross_partition`] naming the lowest other partition the Drop reaches: one holding a bucket of
    /// the namespace's run, or any other partition for a Drop of everything (partitions, "A read-write transaction stays
    /// within one partition in this stage").
    pub fn check_drop(&self, namespace: Option<u64>, epoch: Epoch) -> Result<(), PartitionError> {
        match self.lowest_other(namespace, epoch) {
            Some(other) => Err(PartitionError::cross_partition(self.id.get(), other.get())),
            None => Ok(()),
        }
    }
}
