/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_role::Answer;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Command, DETAIL_TRANSACTION_NOT_HELD, ErrorCode, ErrorReply,
    ErrorReplyView, FLAG_LAST, FrameView, Header, HeaderView, MAX_FRAME_SIZE, PatternSlot,
    QuadPattern, ReadLevel, ReadView, RequestTermView, ResultBatch, TermTableView,
};

use super::client_session_bind::command_of;
use super::{ClientSession, now};
use crate::traits::authorizer::Authorizer;
use crate::types::bind_state::BindState;
use crate::types::client_read::ClientRead;
use crate::types::queued::Queued;
use crate::types::read_pattern::ReadPattern;
use crate::types::read_request::ReadRequest;
use crate::utils::iri_place::{IriPlace, iri_place};
use crate::utils::term_rules::{binds_nobody, check_pattern, check_table};

/// Where a Result frame's command header holds its `epoch`, and its `partition` (`PROTOCOL.md` §9.1).
const RESULT_EPOCH: usize = 8;
const RESULT_PARTITION: usize = 16;

/// The `epoch` and `partition` of the Result `frame`, read in place: the session passes the batch on, and looks at nothing else.
fn read_at(frame: &[u8]) -> Option<(u64, u32)> {
    let header = HeaderView::new(frame).ok()?;
    let fields = header.command_header();
    let epoch = fields.get(RESULT_EPOCH..RESULT_EPOCH + 8)?;
    let partition = fields.get(RESULT_PARTITION..RESULT_PARTITION + 4)?;
    Some((
        u64::from_le_bytes(epoch.try_into().ok()?),
        u32::from_le_bytes(partition.try_into().ok()?),
    ))
}

/// The columns of `pattern`'s batch: its unbound slots and the reifier.
fn columns(pattern: &QuadPattern) -> u32 {
    let unbound = pattern
        .slots()
        .into_iter()
        .filter(|slot| matches!(slot, PatternSlot::Unbound))
        .map(|_| 1_u32)
        .sum::<u32>(); // a pattern's slots, at most four
    unbound + 1
}

/// The bucket of the subject `pattern` binds, by which a read routes.
fn subject_of(
    table: &TermTableView<'_>,
    pattern: &QuadPattern,
    tenant: u64,
) -> Option<ultragraphdb_core_ids::Bucket> {
    let PatternSlot::Bound(index) = pattern.subject() else {
        return None;
    };
    let RequestTermView::Iri(iri) = table.get(index)? else {
        return None;
    };
    match iri_place(tenant, iri) {
        IriPlace::Ordinary(bucket) | IriPlace::Skolem(bucket) => Some(bucket),
        IriPlace::Nobody | IriPlace::TooLong => None,
    }
}

impl<A: Authorizer + Clone> ClientSession<A> {
    /// A Read on `stream`.
    pub(super) fn read(&mut self, stream: u64, view: &FrameView<'_>) -> Result<Answer, ErrorReply> {
        let read = ReadView::from_frame(view).map_err(|rule| ErrorReply::protocol(&rule))?;
        let request = ReadPattern::of_read(&read)?;
        self.take_read(
            stream,
            read.transaction(),
            (read.terms(), read.pattern()),
            request,
        )
    }

    /// A read of `transaction`, checked and kept in order: an IRI nobody can hold answered here, binding nothing.
    pub(super) fn take_read(
        &mut self,
        stream: u64,
        transaction: u64,
        (table, pattern): (&TermTableView<'_>, &QuadPattern),
        request: ReadPattern,
    ) -> Result<Answer, ErrorReply> {
        let Some(txn) = self.transactions.get(&transaction) else {
            return self.not_open(stream, transaction);
        };
        if let Some(refusal) = txn.ended.clone() {
            return Err(refusal);
        }
        let tenant = txn.tenant;
        check_table(table)?;
        check_pattern(table, pattern)?;
        if !txn.declared.is_empty() {
            let refusal = ErrorReply::new(
                ErrorCode::Refused,
                "a transaction holding declarations holds no read",
            );
            self.push_error(stream, &refusal);
            self.end_transaction(transaction, &refusal);
            return Ok(Answer::Open);
        }
        if binds_nobody(table, pattern, tenant) {
            self.push_last(
                stream,
                &ResultBatch::new(columns(pattern)).with_read_at(0, 0),
            );
            return Ok(Answer::Open);
        }
        let credit = self.core.limits().initial_credit();
        let _none = self.reads.insert(
            stream,
            ClientRead {
                transaction,
                request,
                subject: subject_of(table, pattern, tenant),
                credit,
            },
        );
        if let Some(open) = self.transactions.get_mut(&transaction) {
            open.touched = true;
            open.queue.push_back(Queued::Read(stream));
        }
        self.pump(transaction);
        Ok(Answer::Open)
    }

    /// A read on `stream` naming `transaction`, which holds no transaction open. When the transaction's end still waits to leave,
    /// the client sent the read before it could see that end: the read is answered with the transaction's Error, as a read still
    /// open is, or with Error 1 when a Commit's reply ends it (`PROTOCOL.md` §11.2), queued ahead of that end, since a read's stream
    /// closes with its transaction's. Otherwise Error 1, at once.
    pub(super) fn not_open(&mut self, stream: u64, transaction: u64) -> Result<Answer, ErrorReply> {
        let not_open = ErrorReply::new(
            ErrorCode::Protocol,
            format!("no transaction is open on stream {transaction}"),
        );
        let Some(at) = self.out.iter().position(|(on, frame)| {
            *on == transaction
                && HeaderView::new(frame).is_ok_and(|header| header.flags() & FLAG_LAST != 0)
        }) else {
            return Err(not_open);
        };
        let ended = self
            .out
            .get(at)
            .filter(|(_, frame)| command_of(frame) == Some(Command::Error))
            .map(|(_, frame)| frame.clone());
        let answer = match ended {
            Some(mut error) => {
                // A frame this session built holds its header.
                drop(Header::stamp_stream(&mut error, stream));
                error
            }
            None => {
                let header = Header::new(self.cluster(), Command::Error)
                    .with_stream(stream)
                    .with_flags(FLAG_LAST);
                not_open
                    .frame(header)
                    .map_err(|rule| ErrorReply::protocol(&rule))?
            }
        };
        self.out.insert(at, (stream, answer));
        Ok(Answer::Open)
    }

    /// Starts the read at the queue's front. A gated transaction's first holds its later requests until its first frame or its
    /// Error.
    pub(super) fn send_next_read(&mut self, transaction: u64) {
        let Some(txn) = self.transactions.get_mut(&transaction) else {
            return;
        };
        let Some(Queued::Read(read)) = txn.queue.pop_front() else {
            return;
        };
        let gated = !txn.read_only || txn.level == ReadLevel::Latest;
        if gated && !txn.first_sent {
            txn.first_sent = true;
            txn.first_open = true;
            txn.first_read = Some(read);
        }
        let _new = txn.reads_out.insert(read);
        self.dispatch_read(read);
    }

    /// The read `read` asks, with what it takes of its transaction as of now (`PROTOCOL.md` §11.12, *Reads*).
    fn read_request(&self, read: &ClientRead) -> Option<ReadRequest> {
        let txn = self.transactions.get(&read.transaction)?;
        let left = txn.deadline.saturating_duration_since(now());
        let left = u32::try_from(left.as_millis()).unwrap_or(u32::MAX);
        let request = ReadRequest::new(read.request.clone(), txn.tenant, txn.read_only, txn.level)
            .with_principal(self.principal)
            .with_deadline_ms(left)
            .with_max_frame(self.max_frame);
        Some(match (&txn.bind, txn.read_only) {
            (
                BindState::Bound {
                    partition,
                    read_version,
                    ..
                },
                false,
            ) => request
                .with_partition(*partition)
                .with_transaction(txn.identity)
                .with_read_version(*read_version),
            (BindState::None | BindState::Binding { .. } | BindState::Bound { .. }, _) => {
                match (txn.named, txn.level) {
                    (Some(version), _) => request.with_named_version(version),
                    (None, ReadLevel::Snapshot) => request
                        .with_read_version(txn.read_version)
                        .with_epoch(txn.epoch),
                    (None, ReadLevel::Latest) => request
                        .with_partition(txn.level_partition.unwrap_or(0))
                        .with_epoch(txn.epoch),
                    (None, ReadLevel::MonotonicFast) => request.with_epoch(txn.floor),
                }
            }
        })
    }

    /// Hands `read` to the core's executor, which runs it on this core (design D3); a core executing no read answers it code 17.
    fn dispatch_read(&mut self, read: u64) {
        let Some(request) = self
            .reads
            .get(&read)
            .and_then(|state| self.read_request(state))
        else {
            return;
        };
        let Some(executor) = self.reads_run.as_mut() else {
            let refusal = ErrorReply::new(
                ErrorCode::Unavailable,
                "this endpoint core executes no read",
            );
            self.refuse_read(read, &refusal);
            return;
        };
        executor.start(read, request);
    }

    /// Ends what the executor runs of `read`; nothing for a read it does not run.
    pub(super) fn stop_read(&mut self, read: u64) {
        if let Some(executor) = self.reads_run.as_mut() {
            executor.cancel(read);
        }
    }

    /// A frame the executor gave of `read`: a Result batch, or, flagged `LAST`, its refusal as an Error frame.
    pub(super) fn read_frame(&mut self, read: u64, frame: Vec<u8>) {
        if command_of(&frame) != Some(Command::Error) {
            self.relay_result(read, frame);
            return;
        }
        let refusal = FrameView::validate(&frame, self.cluster(), MAX_FRAME_SIZE)
            .and_then(|view| ErrorReplyView::from_frame(&view).and_then(|error| error.to_body()))
            .unwrap_or_else(|rule| {
                ErrorReply::new(
                    ErrorCode::Unavailable,
                    format!("a read's refusal did not read back: {rule}"),
                )
            });
        self.read_refused(read, &refusal);
    }

    /// Answers `read` with `refusal`, which ends it.
    fn refuse_read(&mut self, read: u64, refusal: &ErrorReply) {
        self.push_error(read, refusal);
        self.read_answered(read);
    }

    /// `read` was answered: what its order held back goes on.
    fn read_answered(&mut self, read: u64) {
        let Some(state) = self.reads.remove(&read) else {
            return;
        };
        if let Some(txn) = self.transactions.get_mut(&state.transaction) {
            let _was = txn.reads_out.remove(&read);
            if txn.first_read == Some(read) {
                txn.first_read = None;
                txn.first_open = false;
                // A first read ending before any Result named its partition binds nothing: the next read is first, as after a
                // refusal.
                if txn.read_only {
                    txn.first_sent = false;
                }
            }
        }
        self.pump(state.transaction);
    }

    /// A batch for the client, on its stream: its first frame answers a first read, and its `LAST` the read.
    fn relay_result(&mut self, read: u64, mut frame: Vec<u8>) {
        let last = HeaderView::new(&frame).is_ok_and(|header| header.flags() & FLAG_LAST != 0);
        let at = read_at(&frame);
        // A frame the framer took holds its header.
        drop(Header::stamp_stream(&mut frame, read));
        let Some(state) = self.reads.get_mut(&read) else {
            return;
        };
        let transaction = state.transaction;
        state.credit = state.credit.saturating_sub(1);
        self.push(read, frame);
        if let (Some(txn), Some((epoch, partition))) = (self.transactions.get_mut(&transaction), at)
        {
            txn.floor = txn.floor.max(epoch);
            if txn.first_read == Some(read) {
                txn.first_read = None;
                txn.first_open = false;
                if txn.level == ReadLevel::Latest && partition != 0 {
                    txn.level_partition = Some(partition);
                }
            }
        }
        if last {
            self.read_answered(read);
        } else {
            self.pump(transaction);
        }
    }

    /// The executor refused `read`.
    fn read_refused(&mut self, read: u64, refusal: &ErrorReply) {
        let Some(state) = self.reads.get(&read) else {
            return;
        };
        let transaction = state.transaction;
        let read_write = self
            .transactions
            .get(&transaction)
            .is_some_and(|txn| !txn.read_only);
        if read_write && is_not_held(refusal) {
            // The stream's Error, or the code 23 that cancelled it, is the read's answer: it waits for the transaction's end.
            return;
        }
        if read_write && refusal.code() == ErrorCode::CrossPartition {
            self.read_crossed(read, transaction, refusal);
            return;
        }
        if let Some(txn) = self.transactions.get_mut(&transaction)
            && txn.first_read == Some(read)
            && txn.read_only
        {
            // A first read refused leaves the next read first.
            txn.first_sent = false;
        }
        self.refuse_read(read, refusal);
    }

    /// The executor answered `read` code 23: the Bind stream is cancelled, which aborts the transaction at its partition. A first read
    /// waits for a refreshed map, which may move its subject; any other ends the transaction.
    fn read_crossed(&mut self, read: u64, transaction: u64, refusal: &ErrorReply) {
        let Some(txn) = self.transactions.get_mut(&transaction) else {
            return;
        };
        if let BindState::Binding { link, stream, .. } | BindState::Bound { link, stream, .. } =
            &txn.bind
        {
            link.cancel(*stream);
        }
        let first = txn.first_read == Some(read) && !txn.resent;
        if !first {
            self.cross_ended(read, transaction, refusal);
            return;
        }
        txn.resent = true;
        txn.crossed = Some((read, refusal.clone()));
        self.refresh_map(transaction);
    }

    /// The map a first read's code 23 asked for is here: its subject moved, or the transaction ends with the read's code 23.
    pub(super) fn after_refresh(&mut self, transaction: u64) {
        let Some(txn) = self.transactions.get_mut(&transaction) else {
            return;
        };
        let Some((read, refusal)) = txn.crossed.take() else {
            return;
        };
        let old = match &txn.bind {
            BindState::Binding { partition, .. } | BindState::Bound { partition, .. } => {
                Some(*partition)
            }
            BindState::None => None,
        };
        let moved = self
            .reads
            .get(&read)
            .and_then(|state| state.subject)
            .and_then(|bucket| self.core.links().owner_of(bucket))
            .filter(|owner| Some(*owner) != old);
        let Some(owner) = moved else {
            self.cross_ended(read, transaction, &refusal);
            return;
        };
        let Some(again) = self.transactions.get_mut(&transaction) else {
            return;
        };
        again.bind = BindState::None;
        again.forced = Some(owner);
        again.first_open = false;
        again.first_sent = false;
        again.first_read = None;
        let _was = again.reads_out.remove(&read);
        again.queue.push_front(Queued::Read(read));
    }

    /// A read's code 23 ends its transaction while its stream stays open (`PROTOCOL.md` §11.12): relayed on the read's stream, the
    /// held writes dropped, the other reads answered with it, and its next request and every later read too.
    fn cross_ended(&mut self, read: u64, transaction: u64, refusal: &ErrorReply) {
        self.push_error(read, refusal);
        let _answered = self.reads.remove(&read);
        let others: Vec<u64> = self
            .reads
            .iter()
            .filter(|(_, state)| state.transaction == transaction)
            .map(|(other, _)| *other)
            .collect();
        for other in others {
            if self.reads.remove(&other).is_some() {
                self.stop_read(other);
                self.push_error(other, refusal);
            }
        }
        let Some(txn) = self.transactions.get_mut(&transaction) else {
            return;
        };
        let committing = txn
            .queue
            .iter()
            .any(|queued| matches!(queued, Queued::Commit { .. }));
        let behind: u64 = txn
            .queue
            .drain(..)
            .map(|queued| match queued {
                Queued::Write { behind, .. } => behind,
                Queued::Read(_) | Queued::Commit { .. } => 0,
            })
            .sum();
        txn.reads_out.clear();
        txn.first_read = None;
        txn.first_open = false;
        txn.write_out = None;
        txn.bind = BindState::None;
        txn.ended = Some(refusal.clone());
        self.held = self.held.saturating_sub(behind);
        // A Commit taken meanwhile is the stream's next request, which the 23 answers, ending the transaction.
        if committing {
            self.end_transaction(transaction, refusal);
        }
    }

    /// The client cancelled `read`: one held is dropped unsent, one sent is cancelled and counts as answered.
    pub(super) fn read_cancelled(&mut self, read: u64) {
        let Some(transaction) = self.reads.get(&read).map(|state| state.transaction) else {
            return;
        };
        self.stop_read(read);
        if let Some(txn) = self.transactions.get_mut(&transaction) {
            txn.queue
                .retain(|queued| !matches!(queued, Queued::Read(held) if *held == read));
        }
        self.read_answered(read);
    }

    /// The client granted `read` credit, which its connection counts and tells the executor of as room for its next batch.
    pub(super) fn read_credited(&mut self, read: u64, frames: u32) {
        if let Some(state) = self.reads.get_mut(&read) {
            state.credit = state.credit.saturating_add(frames);
        }
    }

    /// Cancels the reads of `transaction` still open and drops those held: what a Commit and an Abort do, waiting for none.
    pub(super) fn end_reads(&mut self, transaction: u64) {
        let reads: Vec<u64> = self
            .reads
            .iter()
            .filter(|(_, state)| state.transaction == transaction)
            .map(|(read, _)| *read)
            .collect();
        for read in reads {
            if self.reads.remove(&read).is_some() {
                self.stop_read(read);
            }
        }
        if let Some(txn) = self.transactions.get_mut(&transaction) {
            txn.reads_out.clear();
            // A first read cancelled counts as answered; a first Write still waits on its own answer.
            if txn.first_read.take().is_some() {
                txn.first_open = false;
            }
            txn.queue
                .retain(|queued| !matches!(queued, Queued::Read(_)));
        }
    }
}

/// A partition's Error 1 of `detail` 1, passed on by the executor: a Scan named a transaction its partition no longer holds.
fn is_not_held(refusal: &ErrorReply) -> bool {
    refusal.code() == ErrorCode::Protocol && refusal.detail() == DETAIL_TRANSACTION_NOT_HELD
}
