/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod partition_host_bound;
mod partition_host_read;

use std::collections::BTreeMap;

use ultragraphdb_cluster_placement::PartitionId;
use ultragraphdb_core_ids::Epoch;
use ultragraphdb_core_mapping::Changeset;
use ultragraphdb_store_posting::CachedList;
use ultragraphdb_traits_blocks::BlockSource;
use ultragraphdb_traits_log::OrderedLog;
use ultragraphdb_traits_store::{Executor, ListCache, Store};
use ultragraphdb_transactions_commit::Pipeline;
use ultragraphdb_transactions_txn::{Commit, Mode, TxnError, TxnLimits};

use crate::errors::partition_error::PartitionError;
use crate::types::answer::Answer;
use crate::types::bound_transaction::{BoundKey, BoundTransaction};
use crate::types::home::Home;
use crate::types::parked_write::ParkedWrite;
use crate::types::partition::Partition;
use crate::types::partition_mailbox::PartitionMailbox;
use crate::types::partition_message::PartitionMessage;
use crate::types::partition_outcome::PartitionOutcome;
use crate::types::reference::Reference;

/// How long a write's registrations go unanswered before they are sent again: a home that crashed lost what it had not
/// made durable, and a registration is idempotent.
pub(crate) const RESEND_US: u64 = 1_000_000;

/// One partition's loop, on whichever runtime its caller runs it (the storage process's compio runtime per partition, a
/// simulation's single thread): its pipeline, its home, the writes waiting for answers and its mailbox. A write
/// ([`PartitionHost::submit`]) registers what it references at their homes, batched per home, and goes on waiting while
/// the host serves everything else; its answers adopted, it is written and handed to the commit step, which holds it
/// until the closes its answers named are delivered. A turn ([`PartitionHost::turn`]) takes what the mailbox holds,
/// syncs what was decided, and hands out what that made durable.
///
/// The host keeps what a crash loses and nothing else: a restarted host begins with no waiting write, and an answer to a
/// write it no longer holds is dropped.
#[derive(Debug)]
pub struct PartitionHost<'a, L, S, C, E, B>
where
    S: Store + Send + Sync + 'static,
    C: ListCache<Value = CachedList> + Send + Sync + 'static,
    B: BlockSource + Send + Sync + 'static,
    E: Executor,
{
    partition: &'a Partition<S, C, E, B>,
    pipeline: Pipeline<'a, L, S, C, E, B>,
    home: Home,
    mailbox: PartitionMailbox,
    limits: TxnLimits,
    next_ticket: u64,
    last_ticket: u64,
    /// The writes waiting for answers, by ticket.
    parked: BTreeMap<u64, ParkedWrite<'a, S, C, E, B>>,
    /// The registrations the home answers once durable, by the home's ticket: who asked, for which write and index.
    asked: BTreeMap<u64, (Option<PartitionId>, u64, usize)>,
    /// How many registrations and how long each write in the commit step waited, by ticket.
    waited: BTreeMap<u64, (usize, u64)>,
    outcomes: Vec<PartitionOutcome>,
    resends: u64,
    /// The transactions bound to the partition, by the connection and stream that bound them.
    bound: BTreeMap<BoundKey, BoundTransaction<'a, S, C, E, B>>,
    /// The connection and stream of each bound transaction, by the identity its Bind named.
    identities: BTreeMap<[u8; 16], BoundKey>,
    /// The bound transactions whose references were all answered since the last look.
    bound_ready: Vec<BoundKey>,
    /// The bound transactions a home's refusal or a failed adoption ended since the last look, each with why.
    bound_refused: Vec<(BoundKey, PartitionError)>,
}

impl<'a, L, S, C, E, B> PartitionHost<'a, L, S, C, E, B>
where
    L: OrderedLog,
    S: Store + Send + Sync + 'static,
    C: ListCache<Value = CachedList> + Send + Sync + 'static,
    B: BlockSource + Send + Sync + 'static,
    E: Executor,
{
    /// The host of `partition` on `pipeline`, over `partition`'s applier, exchanging through `mailbox`.
    pub fn new(
        partition: &'a Partition<S, C, E, B>,
        pipeline: Pipeline<'a, L, S, C, E, B>,
        mailbox: PartitionMailbox,
    ) -> Self {
        Self {
            partition,
            pipeline,
            home: Home::new(),
            mailbox,
            limits: TxnLimits::default(),
            next_ticket: 1,
            last_ticket: 0,
            parked: BTreeMap::new(),
            asked: BTreeMap::new(),
            waited: BTreeMap::new(),
            outcomes: Vec::new(),
            resends: 0,
            bound: BTreeMap::new(),
            identities: BTreeMap::new(),
            bound_ready: Vec::new(),
            bound_refused: Vec::new(),
        }
    }

    /// The host numbering its tickets from `first`: a restarted host starts above every ticket its predecessor gave, so
    /// an answer sent to an earlier write cannot be taken for a later one's.
    pub fn with_first_ticket(mut self, first: u64) -> Self {
        self.next_ticket = first;
        self
    }

    /// The ticket [`PartitionHost::submit`] gave its last write.
    pub fn last_ticket(&self) -> u64 {
        self.last_ticket
    }

    /// The partition.
    pub fn partition(&self) -> &'a Partition<S, C, E, B> {
        self.partition
    }

    /// The mailbox, whose senders reach every partition.
    pub fn mailbox(&self) -> &PartitionMailbox {
        &self.mailbox
    }

    /// The latest close delivered to the partition.
    pub fn delivered(&self) -> Epoch {
        self.pipeline.delivered()
    }

    /// Whether what was decided waits for a sync.
    pub fn needs_sync(&self) -> bool {
        self.pipeline.needs_sync()
    }

    /// Whether the pipeline stopped: a failed sync or a refused round, after which recovery is the only next step.
    pub fn is_stopped(&self) -> bool {
        self.pipeline.is_stopped()
    }

    /// How many times a write's unanswered registrations went again: what a home's crash or a slow home costs.
    pub fn resends(&self) -> u64 {
        self.resends
    }

    /// How many writes wait for answers.
    pub fn parked(&self) -> usize {
        self.parked.len()
    }

    /// The mailbox, given back as a restart keeps it, and the log, to be opened again.
    pub fn into_parts(self) -> (PartitionMailbox, L) {
        (self.mailbox, self.pipeline.into_log())
    }

    /// The outcomes of the writes ended since the last call.
    pub fn take_outcomes(&mut self) -> Vec<PartitionOutcome> {
        std::mem::take(&mut self.outcomes)
    }

    /// Takes the write `changeset` at `now_us`: its ticket, under which its outcome comes. Registers what it references
    /// and waits for the answers, or goes to the commit step at once when it references nothing to register.
    ///
    /// # Errors
    ///
    /// The pipeline's error, which stops it. A refusal of the write itself is its outcome.
    pub fn submit(&mut self, changeset: Changeset, now_us: u64) -> Result<u64, PartitionError> {
        let ticket = self.ticket();
        self.last_ticket = ticket;
        let buffers = self.pipeline.buffers_mut().pop().unwrap_or_default();
        let begun = self.partition.begin(
            changeset.tenant(),
            Mode::ReadWrite,
            0,
            now_us,
            self.limits,
            buffers,
        );
        let mut txn = match begun {
            Ok(txn) => txn,
            Err(refusal) => {
                self.outcomes
                    .push(PartitionOutcome::new(ticket, Err(refusal), 0, 0));
                return Ok(ticket);
            }
        };
        let mut references = Vec::new();
        // Past the last epoch no close follows; the commit step refuses what it holds.
        let open = self.pipeline.delivered().next().unwrap_or(Epoch::MAX);
        if let Err(refusal) =
            self.partition
                .references(&mut txn, &changeset, open, now_us, &mut references)
        {
            self.outcomes
                .push(PartitionOutcome::new(ticket, Err(refusal), 0, 0));
            return Ok(ticket);
        }
        let parked = ParkedWrite {
            txn,
            changeset,
            references: references.into_iter().map(|r| (r, None)).collect(),
            began_us: now_us,
            sent_us: now_us,
        };
        self.send_unanswered(ticket, &parked);
        let _previous = self.parked.insert(ticket, parked);
        self.finish_if_answered(ticket, now_us)?;
        Ok(ticket)
    }

    /// One turn at `now_us`: every message the mailbox holds, unanswered registrations sent again, every decided round
    /// synced and applied, and what that made durable handed out.
    ///
    /// # Errors
    ///
    /// The pipeline's error, after which it is stopped and recovery is the only next step.
    pub async fn turn(&mut self, now_us: u64) -> Result<(), PartitionError> {
        // Ends when the mailbox is empty: a message taken is gone, and what a turn sends goes to other partitions' or
        // waits for the next turn.
        while let Some(message) = self.mailbox.next() {
            self.handle(message, now_us)?;
        }
        self.resend(now_us);
        self.resend_bound(now_us);
        self.pipeline.tick(now_us);
        // Ends when nothing decided is left unsynced: a sync takes every round decided, and a turn adds none during it.
        while self.pipeline.needs_sync() {
            self.pipeline.sync(now_us).await?;
        }
        self.hand_out();
        Ok(())
    }

    /// Acts on `message`.
    fn handle(&mut self, message: PartitionMessage, now_us: u64) -> Result<(), PartitionError> {
        match message {
            PartitionMessage::Register { from, write, batch } => {
                for (index, reference) in batch {
                    self.accept(from, write, index, &reference, now_us)?;
                }
            }
            PartitionMessage::Answered {
                write,
                index,
                answer,
            } => self.answered(write, index, answer, now_us)?,
            PartitionMessage::Close(close) => self.pipeline.close(close, now_us)?,
        }
        Ok(())
    }

    /// Accepts a registration as the home; its answer goes back at once for a repeat of a durable member, and otherwise
    /// once its entry is durable. A refusal goes back as its answer, unless the pipeline stopped.
    fn accept(
        &mut self,
        from: Option<PartitionId>,
        write: u64,
        index: usize,
        reference: &Reference,
        now_us: u64,
    ) -> Result<(), PartitionError> {
        let ticket = self.ticket();
        let accepted = self.home.accept(
            self.partition,
            &mut self.pipeline,
            ticket,
            reference,
            now_us,
        );
        if self.pipeline.is_stopped() {
            return accepted.map(|_| ());
        }
        match accepted {
            Ok(None) => {
                let _previous = self.asked.insert(ticket, (from, write, index));
            }
            Ok(Some(answer)) => self.answer(from, write, index, Ok(answer)),
            Err(refusal) => self.answer(from, write, index, Err(refusal)),
        }
        Ok(())
    }

    /// Sends `answer` to `from`, itself included; to the shell for an asking from another process.
    fn answer(
        &self,
        from: Option<PartitionId>,
        write: u64,
        index: usize,
        answer: Result<Answer, PartitionError>,
    ) {
        let Some(from) = from else {
            self.mailbox.answer_remote(write, index, answer);
            return;
        };
        self.mailbox.send(
            from,
            PartitionMessage::Answered {
                write,
                index,
                answer,
            },
        );
    }

    /// Takes the answer to registration `index` of the write `write`; one for a write the host no longer holds, gone in
    /// a restart or already ended, is dropped. A refusal ends the write.
    fn answered(
        &mut self,
        write: u64,
        index: usize,
        answer: Result<Answer, PartitionError>,
        now_us: u64,
    ) -> Result<(), PartitionError> {
        if self.bound_answered(write, index, &answer, now_us) {
            return Ok(());
        }
        let Some(parked) = self.parked.get_mut(&write) else {
            return Ok(());
        };
        match answer {
            Ok(answer) => {
                if let Some((_, slot)) = parked.references.get_mut(index) {
                    *slot = Some(answer);
                }
                self.finish_if_answered(write, now_us)
            }
            Err(refusal) => {
                let registrations = parked.references.len();
                let waited = now_us.saturating_sub(parked.began_us);
                let _ended = self.parked.remove(&write);
                self.outcomes.push(PartitionOutcome::new(
                    write,
                    Err(refusal),
                    registrations,
                    waited,
                ));
                Ok(())
            }
        }
    }

    /// Hands the write `write` to the commit step once every reference is answered: the answers adopted, its changes
    /// written. A refusal is its outcome.
    fn finish_if_answered(&mut self, write: u64, now_us: u64) -> Result<(), PartitionError> {
        let answered = self
            .parked
            .get(&write)
            .is_some_and(|p| p.references.iter().all(|(_, answer)| answer.is_some()));
        if !answered {
            return Ok(());
        }
        let Some(mut parked) = self.parked.remove(&write) else {
            return Ok(());
        };
        let registrations = parked.references.len();
        let waited = now_us.saturating_sub(parked.began_us);
        let prepared = (|| -> Result<_, PartitionError> {
            for (reference, answer) in &parked.references {
                if let Some(answer) = answer {
                    self.partition
                        .adopt(&mut parked.txn, reference, answer, now_us)?;
                }
            }
            parked.txn.write(&parked.changeset, now_us)?;
            Ok(parked.txn.commit(0, 0, &[], now_us)?)
        })();
        match prepared {
            Ok(Commit::ReadWrite(prepared)) => {
                let _previous = self.waited.insert(write, (registrations, waited));
                self.pipeline.commit(write, prepared, now_us)?;
            }
            Ok(Commit::ReadOnly(_)) => {}
            Err(refusal) => {
                self.outcomes.push(PartitionOutcome::new(
                    write,
                    Err(refusal),
                    registrations,
                    waited,
                ));
            }
        }
        Ok(())
    }

    /// Sends again the unanswered registrations of each write waiting past [`RESEND_US`], and ends each past its limit.
    fn resend(&mut self, now_us: u64) {
        let limit_us = u64::try_from(self.limits.read_write().as_micros()).unwrap_or(u64::MAX);
        let tickets: Vec<u64> = self.parked.keys().copied().collect();
        for ticket in tickets {
            let Some(parked) = self.parked.get(&ticket) else {
                continue;
            };
            if now_us.saturating_sub(parked.began_us) > limit_us {
                let registrations = parked.references.len();
                let waited = now_us.saturating_sub(parked.began_us);
                let _ended = self.parked.remove(&ticket);
                let refusal = PartitionError::from(TxnError::past_limit(limit_us / 1_000));
                self.outcomes.push(PartitionOutcome::new(
                    ticket,
                    Err(refusal),
                    registrations,
                    waited,
                ));
            } else if now_us.saturating_sub(parked.sent_us) >= RESEND_US {
                self.send_unanswered(ticket, parked);
                self.resends += 1;
                if let Some(resent) = self.parked.get_mut(&ticket) {
                    resent.sent_us = now_us;
                }
            }
        }
    }

    /// Sends `parked`'s unanswered references to their homes, one message per home.
    fn send_unanswered(&self, ticket: u64, parked: &ParkedWrite<'a, S, C, E, B>) {
        self.send_references(ticket, &parked.references);
    }

    /// Sends the unanswered ones of `references` to their homes, one message per home.
    fn send_references(&self, ticket: u64, references: &[(Reference, Option<Answer>)]) {
        let mut batches: BTreeMap<PartitionId, Vec<(usize, Reference)>> = BTreeMap::new();
        for (index, (reference, answer)) in references.iter().enumerate() {
            if answer.is_none() {
                batches
                    .entry(reference.home())
                    .or_default()
                    .push((index, reference.clone()));
            }
        }
        for (home, batch) in batches {
            self.mailbox.send(
                home,
                PartitionMessage::Register {
                    from: Some(self.partition.id()),
                    write: ticket,
                    batch,
                },
            );
        }
    }

    /// Hands out what the last sync made durable: the home's answers to who asked, and the writes' outcomes.
    fn hand_out(&mut self) {
        let mut settled = Vec::new();
        self.home.settle(self.pipeline.replies_mut(), &mut settled);
        for (ticket, answer) in settled {
            if let Some((from, write, index)) = self.asked.remove(&ticket) {
                self.answer(from, write, index, answer);
            }
        }
        let replies = std::mem::take(self.pipeline.replies_mut());
        for reply in replies {
            let (registrations, waited) = self.waited.remove(&reply.ticket()).unwrap_or_default();
            let outcome = reply.outcome().clone().map_err(PartitionError::from);
            self.outcomes.push(
                PartitionOutcome::new(reply.ticket(), outcome, registrations, waited)
                    .with_reply(reply),
            );
        }
    }

    /// The next ticket, above no ticket the pipeline reserves.
    fn ticket(&mut self) -> u64 {
        let ticket = self.next_ticket;
        self.next_ticket += 1;
        ticket
    }
}
