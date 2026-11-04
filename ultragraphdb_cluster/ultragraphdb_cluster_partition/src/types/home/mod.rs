/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeMap;
use std::num::NonZeroU64;

use ultragraphdb_core_ids::{Bucket, Epoch, SystemId, Version};
use ultragraphdb_protocol_wire::{ErrorCode, RecordRegistration, WriteSetRecord};
use ultragraphdb_store_apply::{ChangesetError, ChangesetErrorEnum};
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_store_quads::{QuadError, QuadErrorEnum, ReferrerSet};
use ultragraphdb_traits_blocks::BlockSource;
use ultragraphdb_traits_log::OrderedLog;
use ultragraphdb_traits_store::{Executor, ListCache, Store};
use ultragraphdb_transactions_commit::{Pipeline, Reply};

use crate::errors::partition_error::PartitionError;
use crate::types::answer::Answer;
use crate::types::partition::Partition;
use crate::types::reference::Reference;
use crate::types::waiter::Waiter;

/// A partition as the home of its entities (term-dictionary, "A reference to another partition's entity registers at
/// its home"; partitions, "An entity's referrer set names where its inbound edges may be"): it gets or creates a
/// registered entity's ID, decides the referrer-set member by the generation rule, hands each decision to its
/// pipeline as a round entry of its own (a kind-2 record), and answers once that entry is durable, with the ID, the
/// member's generation and the latest close delivered below the epoch it was stamped at.
///
/// Decisions not yet applied are held in an overlay over the stored referrer sets, so registrations arriving before
/// earlier ones are durable decide against them: concurrent registrants of one entity get one ID and each its own
/// member, and a repeat waits for the entry that decided its generation. A repeat of a durable member is answered at
/// once, changing nothing.
#[derive(Debug, Default)]
pub struct Home {
    /// By namespace and entity: the referrer set as decided, and the pipeline ticket of its latest decision, until that
    /// decision is applied.
    overlay: BTreeMap<(u64, SystemId), (ReferrerSet, u64)>,
    /// By pipeline ticket: the registrations answered once its entry is durable.
    waiting: BTreeMap<u64, Vec<Waiter>>,
}

impl Home {
    /// A home with no registration in flight.
    pub fn new() -> Self {
        Self::default()
    }

    /// Accepts `reference`, a registration for an entity of `partition`, under `ticket` at `now_us`: the entity's ID
    /// got or created and the member decided, handed to `pipeline` when it changes the set or creates an ID. The answer
    /// at once for a repeat of a durable member; `None` while the answer waits for an entry to be durable, given by
    /// [`Home::settle`] under `ticket`.
    ///
    /// # Errors
    ///
    /// [`PartitionError::wrong_partition`] for an entity of a bucket the partition does not own; the applier's refusals
    /// of getting or creating the ID; the store's error reading the referrer set; the pipeline's error.
    pub fn accept<L, S, C, E, B>(
        &mut self,
        partition: &Partition<S, C, E, B>,
        pipeline: &mut Pipeline<'_, L, S, C, E, B>,
        ticket: u64,
        reference: &Reference,
        now_us: u64,
    ) -> Result<Option<Answer>, PartitionError>
    where
        L: OrderedLog,
        S: Store + Send + Sync + 'static,
        C: ListCache<Value = CachedList> + Send + Sync + 'static,
        B: BlockSource + Send + Sync + 'static,
        E: Executor,
    {
        let namespace = reference.namespace();
        let open = pipeline
            .delivered()
            .next()
            .map_err(|_| refused_at_the_last_epoch())?;
        let applier = partition.applier();
        let registering = applier
            .register(namespace, reference.entity_ref())
            .map_err(|e| not_owned(partition, open, e))?;
        let entity = registering.entity();
        if let Err(misrouted) = partition.route(entity.bucket(), open) {
            applier.release(&registering.into_executed());
            return Err(misrouted);
        }
        let Some(group) = reference.group() else {
            if !registering.holds_entries() {
                return Ok(Some(Answer::new(entity, None, pipeline.delivered())));
            }
            let record = WriteSetRecord::entity(namespace).encode();
            pipeline.registration(ticket, registering.finish_entity(), record, now_us)?;
            self.wait(ticket, ticket, entity, None);
            return Ok(None);
        };
        let key = (namespace, entity);
        let mut set = match self.overlay.get(&key) {
            Some((set, _)) => set.clone(),
            None => {
                let applied = applier.quads().lists().applied_ts();
                applier.quads().referrers(namespace, entity, applied)?
            }
        };
        let registered = set.register(group, reference.named(), open);
        let generation = registered.generation;
        if registered.changed {
            let record = WriteSetRecord::registration(
                namespace,
                RecordRegistration::new(entity, group, generation),
            )
            .encode();
            let executed = registering.finish(group, generation);
            pipeline.registration(ticket, executed, record, now_us)?;
            let _replaced = self.overlay.insert(key, (set, ticket));
            self.wait(ticket, ticket, entity, Some(generation));
            return Ok(None);
        }
        applier.release(&registering.into_executed());
        // A repeat of a member still on its way waits for the entry that decided it.
        if let Some((_, deciding)) = self.overlay.get(&key) {
            let deciding = *deciding;
            self.wait(deciding, ticket, entity, Some(generation));
            return Ok(None);
        }
        Ok(Some(Answer::new(
            entity,
            Some(generation),
            pipeline.delivered(),
        )))
    }

    /// Takes from `replies` those of the entries the home waits on, appending to `out` each answer they settle, under
    /// the ticket it was accepted under: the answer, or the pipeline's refusal of the entry. Replies of other tickets
    /// stay in `replies`.
    pub fn settle(
        &mut self,
        replies: &mut Vec<Reply>,
        out: &mut Vec<(u64, Result<Answer, PartitionError>)>,
    ) {
        replies.retain(|reply| {
            let Some(waiters) = self.waiting.remove(&reply.ticket()) else {
                return true;
            };
            self.overlay
                .retain(|_, (_, deciding)| *deciding != reply.ticket());
            for waiter in waiters {
                let answer = match reply.outcome() {
                    Ok(version) => {
                        // The epoch the entry was stamped at is one above the latest close delivered then; a commit
                        // version's epoch is at least 1.
                        let stamped = Version::from_u64(*version).epoch().get();
                        Epoch::new(stamped.saturating_sub(1))
                            .map(|close| Answer::new(waiter.entity, waiter.generation, close))
                            .map_err(|_| refused_at_the_last_epoch())
                    }
                    Err(refusal) => Err(refusal.clone().into()),
                };
                out.push((waiter.ticket, answer));
            }
            false
        });
    }

    /// Waits on the entry of pipeline ticket `entry` to answer the registration accepted under `ticket`.
    fn wait(&mut self, entry: u64, ticket: u64, entity: SystemId, generation: Option<NonZeroU64>) {
        self.waiting.entry(entry).or_default().push(Waiter {
            ticket,
            entity,
            generation,
        });
    }
}

/// The refusal of a registration past the last epoch, which no close follows: `Busy`, as a partition that cannot stamp
/// answers a commit.
fn refused_at_the_last_epoch() -> PartitionError {
    PartitionError::refused(ErrorCode::Busy.number())
}

/// A refusal of getting or creating an ID: an IRI of a bucket the partition does not own names that bucket's owner.
fn not_owned<S, C, E, B>(
    partition: &Partition<S, C, E, B>,
    epoch: Epoch,
    error: ChangesetError,
) -> PartitionError
where
    S: Store + Send + Sync + 'static,
    C: ListCache<Value = CachedList> + Send + Sync + 'static,
    B: BlockSource + Send + Sync + 'static,
    E: Executor,
{
    if let ChangesetErrorEnum::Quad(QuadError(QuadErrorEnum::NotOwned { bucket, .. })) = &error.0
        && let Ok(found) = Bucket::new(*bucket)
    {
        let owner = partition.placement().partition_of(found, epoch);
        return PartitionError::wrong_partition(*bucket, owner.get());
    }
    error.into()
}
