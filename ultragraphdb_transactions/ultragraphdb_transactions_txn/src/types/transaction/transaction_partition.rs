/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroU64;

use oxrdf::{GraphNameRef, NamedNodeRef, NamedOrBlankNode, NamedOrBlankNodeRef, TermRef};
use ultragraphdb_cluster_placement::TenantRun;
use ultragraphdb_core_ids::{BucketGroup, Epoch, SystemId, Tenant, Version};
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_traits_blocks::BlockSource;
use ultragraphdb_traits_store::{Executor, ListCache, Store};

use crate::errors::txn_error::TxnError;
use crate::types::transaction::Transaction;

/// What a transaction does as one partition's (partitions, "A read-write transaction stays within one partition in
/// this stage"; term-dictionary, "A reference to another partition's entity registers at its home"): its reverse reads
/// and index lookups, allowed only while its partition owns the tenant's whole run, and the IDs and marks of other
/// partitions' entities their homes answered, each holding its commit until the close the reply named is delivered.
impl<S, C, E, B> Transaction<'_, S, C, E, B>
where
    S: Store + Send + Sync + 'static,
    C: ListCache<Value = CachedList> + Send + Sync + 'static,
    B: BlockSource + Send + Sync + 'static,
    E: Executor,
{
    /// The subjects holding `predicate` to `object` in `graph` at `now_us`, as the transaction sees them, into `out`,
    /// cleared first; recorded in its read set. A bound transaction reads its partition's own reverse entries, at its
    /// latest state, only while the partition owns the tenant's whole run at its read epoch.
    ///
    /// # Errors
    ///
    /// [`TxnError::ended`]; [`TxnError::past_limit`], ending it; [`TxnError::cross_partition`] naming the lowest other
    /// partition of the tenant's run when the bound partition does not own all of it, ending it;
    /// [`TxnError::changeset`] for a store that failed, ending it.
    pub fn subjects_into(
        &mut self,
        graph: GraphNameRef<'_>,
        predicate: NamedNodeRef<'_>,
        object: TermRef<'_>,
        now_us: u64,
        out: &mut Vec<NamedOrBlankNode>,
    ) -> Result<(), TxnError> {
        let foreign = self.foreign_run();
        let execution = self.live(now_us)?;
        if let Some((partition, other)) = foreign {
            self.end();
            return Err(TxnError::cross_partition(partition, other));
        }
        if let Err(e) = execution.subjects_into(graph, predicate, object, out) {
            self.end();
            return Err(e.into());
        }
        Ok(())
    }

    /// The subjects whose `predicate` in `graph` holds `value` by its `tokenizer` index at `now_us`, into `out`,
    /// cleared first; recorded in its read set. A fan-out read: allowed to a bound transaction only while its
    /// partition owns the tenant's whole run at its read epoch.
    ///
    /// # Errors
    ///
    /// As [`Transaction::subjects_into`], and the execution's refusal of an index the catalog does not declare,
    /// ending it.
    pub fn lookup_into(
        &mut self,
        graph: GraphNameRef<'_>,
        predicate: NamedNodeRef<'_>,
        tokenizer: &str,
        value: TermRef<'_>,
        now_us: u64,
        out: &mut Vec<NamedOrBlankNode>,
    ) -> Result<(), TxnError> {
        let foreign = self.foreign_run();
        let execution = self.live(now_us)?;
        if let Some((partition, other)) = foreign {
            self.end();
            return Err(TxnError::cross_partition(partition, other));
        }
        if let Err(e) = execution.lookup_into(graph, predicate, tokenizer, value, out) {
            self.end();
            return Err(e.into());
        }
        Ok(())
    }

    /// The ID the transaction's execution gives the blank node labelled by `node` at `now_us`, the same for every later
    /// change naming the label: its bucket group is where the node's edges register at their objects' homes.
    ///
    /// # Errors
    ///
    /// [`TxnError::ended`]; [`TxnError::past_limit`], ending it; [`TxnError::changeset`] for a refusal of the ID or a
    /// store that failed, ending it.
    pub fn node_id(
        &mut self,
        node: NamedOrBlankNodeRef<'_>,
        now_us: u64,
    ) -> Result<SystemId, TxnError> {
        let execution = self.live(now_us)?;
        match execution.node_id(node) {
            Ok(id) => Ok(id),
            Err(e) => {
                self.end();
                Err(e.into())
            }
        }
    }

    /// Takes `iri` as named by `id`, which its home answered with the latest close `close` delivered to it: the
    /// transaction resolves `iri` to `id`, its commit copies `iri`'s dictionary entries into the partition, and the
    /// commit is held until close `close` is delivered to the partition, so it is stamped at `close + 1` or later.
    ///
    /// # Errors
    ///
    /// [`TxnError::ended`]; [`TxnError::past_limit`], ending it.
    pub fn adopt(
        &mut self,
        iri: &str,
        id: SystemId,
        close: Epoch,
        now_us: u64,
    ) -> Result<(), TxnError> {
        self.live(now_us)?.adopt(iri, id);
        self.raise_floor(close);
        Ok(())
    }

    /// The generation of the partition's mark of `(entity, group)` at `now_us`, as the transaction sees it; recorded
    /// in its read set. `None` when the partition holds none, so the reference registers at the entity's home.
    ///
    /// # Errors
    ///
    /// [`TxnError::ended`]; [`TxnError::past_limit`], ending it; [`TxnError::changeset`] for a store that failed,
    /// ending it.
    pub fn read_mark(
        &mut self,
        entity: SystemId,
        group: BucketGroup,
        now_us: u64,
    ) -> Result<Option<NonZeroU64>, TxnError> {
        let execution = self.live(now_us)?;
        match execution.read_mark(entity, group) {
            Ok(generation) => Ok(generation),
            Err(e) => {
                self.end();
                Err(e.into())
            }
        }
    }

    /// Marks `(entity, group)` at `generation`, as the entity's home answered a registration with the latest close
    /// `close` delivered to it: the mark's key is in the transaction's read set, its commit writes the mark and lists
    /// the registration in its record (section 4), and the commit is held until close `close` is delivered.
    ///
    /// # Errors
    ///
    /// [`TxnError::ended`]; [`TxnError::past_limit`], ending it; [`TxnError::changeset`] for a store that failed,
    /// ending it.
    pub fn mark(
        &mut self,
        entity: SystemId,
        group: BucketGroup,
        generation: NonZeroU64,
        close: Epoch,
        now_us: u64,
    ) -> Result<(), TxnError> {
        let execution = self.live(now_us)?;
        if let Err(e) = execution.mark(entity, group, generation) {
            self.end();
            return Err(e.into());
        }
        self.raise_floor(close);
        Ok(())
    }

    /// The epoch the transaction's commit may be stamped at, at the earliest: one above the latest close a home's reply
    /// named, `None` before any reply.
    #[must_use]
    pub fn epoch_floor(&self) -> Option<Epoch> {
        self.floor
    }

    /// Raises the epoch floor to one above `close`, the latest close a home's reply named. A close of the last epoch
    /// leaves the floor at it: no epoch follows, and the commit step stops before stamping past it.
    fn raise_floor(&mut self, close: Epoch) {
        let next = close.next().unwrap_or(Epoch::MAX);
        self.floor = Some(self.floor.map_or(next, |floor| floor.max(next)));
    }

    /// The bound partition and the lowest other partition holding buckets of the tenant's run at the transaction's
    /// read epoch, when the bound partition does not own the whole run; `None` when unbound or it does.
    fn foreign_run(&self) -> Option<(u32, u32)> {
        let (partition, placement) = self.binding?;
        let epoch = Version::from_u64(self.read_version).epoch();
        let run = TenantRun::of(Tenant::new(self.tenant)).range();
        placement
            .ranges_at(epoch)
            .into_iter()
            .filter(|(range, owner)| {
                *owner != partition && range.start() < run.end() && run.start() < range.end()
            })
            .map(|(_, owner)| owner.get())
            .min()
            .map(|other| (partition.get(), other))
    }
}
