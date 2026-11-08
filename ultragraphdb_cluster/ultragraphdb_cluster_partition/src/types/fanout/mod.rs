/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeMap;
use std::num::NonZeroUsize;

use ultragraphdb_cluster_placement::PartitionId;
use ultragraphdb_core_ids::{Epoch, SystemId, Tenant, Version};
use ultragraphdb_core_keys::{CountDirection, Graph, Keyspace, QuadKey};
use ultragraphdb_core_terms::EncodedTerm;
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_store_quads::{KernelScan, QuadError, ReferrerMember};
use ultragraphdb_traits_blocks::BlockSource;
use ultragraphdb_traits_store::{Executor, ListCache, Store};

use crate::errors::partition_error::PartitionError;
use crate::types::partition::Partition;
use crate::types::reverse_query::ReverseQuery;

/// How many lists one scan of a reverse count's keys reads.
const COUNT_ROWS: NonZeroUsize = NonZeroUsize::MIN.saturating_add(255);

/// Reverse reads over the partitions of one process at a closed epoch (quad-reads, "A reverse read asks the object's
/// referrers"): the object's referrer set read at its home, its bucket groups resolved to partitions through the
/// placement map in effect at the epoch, and only those partitions asked for their own reverse entries. The caller
/// reads at an epoch every partition has closed; stage 8's compute waits for it.
#[derive(Debug)]
pub struct Fanout<'p, S, C, E, B> {
    partitions: Vec<&'p Partition<S, C, E, B>>,
    /// The latest epoch each partition has closed, by position; every one when empty.
    closed: Vec<Epoch>,
}

impl<'p, S, C, E, B> Fanout<'p, S, C, E, B>
where
    S: Store + Send + Sync + 'static,
    C: ListCache<Value = CachedList> + Send + Sync + 'static,
    B: BlockSource + Send + Sync + 'static,
    E: Executor,
{
    /// The reads over `partitions`.
    pub fn new(partitions: &'p [Partition<S, C, E, B>]) -> Self {
        Self::over(partitions.iter().collect())
    }

    /// The reads over `partitions`, wherever each is held.
    #[must_use]
    pub fn over(partitions: Vec<&'p Partition<S, C, E, B>>) -> Self {
        Self {
            partitions,
            closed: Vec::new(),
        }
    }

    /// The reads with `closed`, the latest epoch each partition has closed, by position: a read asking a partition
    /// that has not closed its epoch fails naming it, as a snapshot read does at its deadline.
    #[must_use]
    pub fn with_closed(mut self, closed: Vec<Epoch>) -> Self {
        self.closed = closed;
        self
    }

    /// The subjects holding `query`'s predicate to its object at its epoch, into `out`, cleared first, sorted; and the
    /// partitions asked for their reverse entries, in order: exactly those the object's referrer set names at the epoch.
    ///
    /// # Errors
    ///
    /// [`PartitionError::timed_out`] naming the first partition asked, the home first, that has not closed the query's
    /// epoch, with no row of the others; [`PartitionError::wrong_partition`] when the query's home or a partition the
    /// set names is not among the partitions, naming the first bucket of the group that resolved to it; the store's
    /// error.
    pub fn subjects(
        &self,
        query: &ReverseQuery,
        out: &mut Vec<SystemId>,
    ) -> Result<Vec<PartitionId>, PartitionError> {
        out.clear();
        let asked = self.asked(query)?;
        let object = EncodedTerm::id(query.object());
        let mut gathered = Vec::new();
        for partition in &asked {
            gathered.extend(partition.applier().quads().subjects(
                query.tenant(),
                query.graph(),
                query.predicate(),
                &object,
                read_version(partition, query.epoch()),
            )?);
        }
        gathered.sort_unstable();
        out.append(&mut gathered);
        Ok(asked.into_iter().map(Partition::id).collect())
    }

    /// The partitions a reverse read asks, in order, all of which have closed its epoch: those the object's referrer set
    /// names at the epoch, through the placement map then in effect.
    fn asked(
        &self,
        query: &ReverseQuery,
    ) -> Result<Vec<&'p Partition<S, C, E, B>>, PartitionError> {
        let epoch = query.epoch();
        let home = self.closed_partition(query.home(), epoch, query.object().bucket().get())?;
        let set = home.applier().quads().referrers(
            query.tenant(),
            query.object(),
            read_version(home, epoch),
        )?;
        home.placement()
            .holders_of_groups(set.at(epoch).map(ReferrerMember::group), epoch)
            .into_iter()
            .map(|(holder, bucket)| self.closed_partition(holder, epoch, bucket))
            .collect()
    }

    /// The partition `id`, once it has closed `epoch`; `bucket` is what resolved to it.
    fn closed_partition(
        &self,
        id: PartitionId,
        epoch: Epoch,
        bucket: u32,
    ) -> Result<&'p Partition<S, C, E, B>, PartitionError> {
        let at = self.partitions.iter().position(|p| p.id() == id);
        let Some((at, partition)) = at.and_then(|at| Some((at, self.partitions.get(at).copied()?)))
        else {
            return Err(PartitionError::wrong_partition(bucket, id.get()));
        };
        match self.closed.get(at) {
            Some(closed) if *closed < epoch => Err(PartitionError::timed_out(id.get())),
            _ => Ok(partition),
        }
    }

    /// How many subjects hold `query`'s predicate to its object at its epoch: the reverse counts of the partitions the
    /// object's referrer set names, summed (index-maintenance, "Count indexes").
    ///
    /// # Errors
    ///
    /// As [`Fanout::subjects`].
    pub fn count(&self, query: &ReverseQuery) -> Result<u64, PartitionError> {
        let object = EncodedTerm::id(query.object());
        self.asked(query)?
            .into_iter()
            .try_fold(0u64, |sum, partition| {
                let count = partition.applier().quads().reverse_count(
                    query.tenant(),
                    query.graph(),
                    query.predicate(),
                    &object,
                    read_version(partition, query.epoch()),
                )?;
                Ok(sum.saturating_add(count))
            })
    }

    /// The objects whose reverse count of `predicate` in `graph` of `tenant` at `epoch` is exactly `count`, summed
    /// over every partition of the tenant's run, since each lists an object under its own subjects' count alone
    /// (index-maintenance, "A reverse count sums the referrers' partitions"), sorted; into `out`, cleared first.
    ///
    /// # Errors
    ///
    /// [`PartitionError::timed_out`] for a partition that has not closed `epoch`; the store's error.
    pub fn reverse_counted(
        &self,
        tenant: u64,
        graph: Graph,
        predicate: SystemId,
        count: u64,
        epoch: Epoch,
        out: &mut Vec<EncodedTerm>,
    ) -> Result<(), PartitionError> {
        out.clear();
        let key = |listed| {
            QuadKey::CountIndex {
                tenant,
                graph,
                predicate,
                direction: CountDirection::Reverse,
                count: listed,
            }
            .to_bytes()
        };
        let (first, past) = (key(0), key(u32::MAX));
        let mut counts: BTreeMap<Vec<u8>, u64> = BTreeMap::new();
        let mut rows = Vec::new();
        for partition in self.of_run(tenant, epoch)? {
            let mut from = first.clone();
            // Ends: each scan resumes past the lists it read, and the range holds finitely many.
            loop {
                let scan = KernelScan::new(
                    Keyspace::CountIndex,
                    tenant,
                    from,
                    read_version(partition, epoch),
                    COUNT_ROWS,
                )
                .to(past.clone());
                rows.clear();
                let resume = partition.applier().quads().scan(&scan, &mut rows)?;
                for row in &rows {
                    let (parsed, _) = QuadKey::parse(row.key()).map_err(QuadError::from)?;
                    let QuadKey::CountIndex { count: listed, .. } = parsed else {
                        continue;
                    };
                    for (object, _) in row.objects() {
                        let sum = counts.entry(object.as_bytes().to_vec()).or_insert(0);
                        *sum = sum.saturating_add(u64::from(listed));
                    }
                }
                let Some(next) = resume else {
                    break;
                };
                from = next;
            }
        }
        for (bytes, sum) in counts {
            if sum == count {
                out.push(EncodedTerm::from_bytes(&bytes).map_err(QuadError::from)?);
            }
        }
        Ok(())
    }

    /// The partitions holding a bucket of `tenant`'s run at `epoch`, all of which have closed it.
    fn of_run(
        &self,
        tenant: u64,
        epoch: Epoch,
    ) -> Result<Vec<&'p Partition<S, C, E, B>>, PartitionError> {
        let Some(any) = self.partitions.first().copied() else {
            return Ok(Vec::new());
        };
        any.placement()
            .holders_of_run(Tenant::new(tenant), epoch)
            .into_iter()
            .map(|(owner, first)| self.closed_partition(owner, epoch, first))
            .collect()
    }
}

/// The version a read of `partition`, which has closed `epoch`, reads at: the epoch's highest, every commit stamped at or
/// below it and the rollups of those, or the partition's watermark below that, since it has applied all of them and a
/// read at a version above the watermark waits for it.
fn read_version<S, C, E, B>(partition: &Partition<S, C, E, B>, epoch: Epoch) -> u64
where
    S: Store + Send + Sync + 'static,
    C: ListCache<Value = CachedList> + Send + Sync + 'static,
    B: BlockSource + Send + Sync + 'static,
    E: Executor,
{
    Version::bound_of(epoch).min(partition.applier().quads().lists().applied_ts())
}
