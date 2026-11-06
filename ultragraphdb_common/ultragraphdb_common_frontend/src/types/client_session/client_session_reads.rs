/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_role::Answer;
use ultragraphdb_protocol_wire::{
    BodyBuilder, BodyView, Command, ComputeRead, DETAIL_TRANSACTION_NOT_HELD, ErrorCode,
    ErrorReply, FLAG_LAST, FrameView, Header, HeaderView, PatternSlot, QuadPattern, ReadLevel,
    ReadView, RequestTermView, ResultBatch, TermTable, TermTableView,
};

use super::client_session_bind::{command_of, error_of};
use super::{ClientSession, now};
use crate::traits::authorizer::Authorizer;
use crate::types::client_read::ClientRead;
use crate::types::client_transaction::{BindState, Queued};
use crate::types::link_event::LinkEvent;
use crate::types::route_key::RouteKey;
use crate::utils::iri_place::{IriPlace, iri_place};
use crate::utils::term_rules::{binds_nobody, check_pattern, check_table};

/// Where a Result frame's command header holds its `epoch`, and its `partition` (`PROTOCOL.md` §9.1).
const RESULT_EPOCH: usize = 8;
const RESULT_PARTITION: usize = 16;

/// The `epoch` and `partition` of the Result `frame`, read in place: the frontend relays the batch, and looks at nothing else.
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
        .count() as u32;
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
        self.take_read(
            stream,
            read.transaction(),
            read.terms(),
            read.pattern(),
            read.limit(),
            view.body().to_vec(),
        )
    }

    /// A read of `transaction`, checked and kept in order: an IRI nobody can hold answered here, binding nothing.
    pub(super) fn take_read(
        &mut self,
        stream: u64,
        transaction: u64,
        table: &TermTableView<'_>,
        pattern: &QuadPattern,
        limit: u32,
        body: Vec<u8>,
    ) -> Result<Answer, ErrorReply> {
        let Some(txn) = self.transactions.get(&transaction) else {
            return Err(ErrorReply::new(
                ErrorCode::Protocol,
                format!("no transaction is open on stream {transaction}"),
            ));
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
                body,
                pattern: *pattern,
                limit,
                subject: subject_of(table, pattern, tenant),
                credit,
                route: None,
                attempt: 0,
                tried: Vec::new(),
                frames: 0,
                last_frame: now(),
            },
        );
        if let Some(open) = self.transactions.get_mut(&transaction) {
            open.touched = true;
            open.queue.push_back(Queued::Read(stream));
        }
        self.pump(transaction);
        Ok(Answer::Open)
    }

    /// Sends the read at the queue's front to the next compute process. A gated transaction's first holds its later requests until
    /// its first frame or its Error.
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

    /// The ComputeRead of `read`, what compute needs of its transaction as of now.
    fn compute_read(&self, read: &ClientRead) -> Option<ComputeRead> {
        let txn = self.transactions.get(&read.transaction)?;
        let left = txn.deadline.saturating_duration_since(now());
        let left = u32::try_from(left.as_millis()).unwrap_or(u32::MAX);
        let body = ComputeRead::new(
            txn.tenant,
            txn.read_only,
            txn.level,
            TermTable::new(),
            read.pattern,
            Vec::new(),
        )
        .with_limit(read.limit)
        .with_principal(self.principal)
        .with_deadline_ms(left)
        .with_client_limits(self.max_frame, read.credit);
        Some(match (&txn.bind, txn.read_only) {
            (
                BindState::Bound {
                    partition,
                    read_version,
                    ..
                },
                false,
            ) => body
                .with_partition(*partition)
                .with_transaction(txn.identity)
                .with_read_version(*read_version),
            (BindState::None | BindState::Binding { .. } | BindState::Bound { .. }, _) => {
                match (txn.named, txn.level) {
                    (Some(version), _) => body.with_named_version(version),
                    (None, ReadLevel::Snapshot) => body
                        .with_read_version(txn.read_version)
                        .with_epoch(txn.epoch),
                    (None, ReadLevel::Latest) => body
                        .with_partition(txn.level_partition.unwrap_or(0))
                        .with_epoch(txn.epoch),
                    (None, ReadLevel::MonotonicFast) => body.with_epoch(txn.floor),
                }
            }
        })
    }

    /// Sends `read` to the next compute process it has not tried; with none left it is answered with code 17.
    fn dispatch_read(&mut self, read: u64) {
        let Some(state) = self.reads.get(&read) else {
            return;
        };
        let Some((address, link)) = self.core.links().next_compute(&state.tried) else {
            let refusal = ErrorReply::new(ErrorCode::Unavailable, "no compute process answers");
            self.refuse_read(read, &refusal);
            return;
        };
        let Some(body) = self.compute_read(state) else {
            return;
        };
        let stream = link.next_stream();
        let header = Header::new(self.cluster(), Command::ComputeRead)
            .with_stream(stream)
            .with_command_header(body.command_header());
        let frame = match header.encode_frame(&state.body) {
            Ok(frame) if frame.len() as u64 <= u64::from(link.max_frame()) => frame,
            Ok(_) | Err(_) => {
                let refusal = ErrorReply::new(
                    ErrorCode::TooLarge,
                    "the read's ComputeRead does not fit a frame",
                )
                .with_detail(u64::from(link.max_frame()));
                self.refuse_read(read, &refusal);
                return;
            }
        };
        let attempt = state.attempt + 1;
        let sink = self.sink(RouteKey::Read { read, attempt });
        let opened = link.open(frame, sink);
        if let Some(sent) = self.reads.get_mut(&read) {
            sent.attempt = attempt;
            sent.route = Some((link, opened));
            sent.tried.push(address);
            sent.last_frame = now();
        }
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

    /// What a read's ComputeRead stream said.
    pub(super) fn read_event(&mut self, read: u64, attempt: u32, event: LinkEvent) {
        if self
            .reads
            .get(&read)
            .is_none_or(|state| state.attempt != attempt)
        {
            return;
        }
        match event {
            LinkEvent::Closed => self.read_closed(read),
            LinkEvent::Frame(frame) if command_of(&frame) == Some(Command::Error) => {
                self.read_refused(read, &frame);
            }
            LinkEvent::Frame(frame) if command_of(&frame) == Some(Command::Result) => {
                self.relay_result(read, frame);
            }
            LinkEvent::Frame(_) => {}
        }
    }

    /// A role connection closing under `read`: before any frame reached the client, the next compute process; after, code 17.
    fn read_closed(&mut self, read: u64) {
        let Some(state) = self.reads.get_mut(&read) else {
            return;
        };
        state.route = None;
        if state.frames == 0 {
            self.dispatch_read(read);
            return;
        }
        let refusal = self.unavailable();
        self.refuse_read(read, &refusal);
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
        state.frames += 1;
        state.credit = state.credit.saturating_sub(1);
        state.last_frame = now();
        state.body = Vec::new();
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

    /// Compute refused `read`.
    fn read_refused(&mut self, read: u64, frame: &[u8]) {
        let Some(refusal) = error_of(self.cluster(), frame) else {
            let refusal = self.unavailable();
            self.refuse_read(read, &refusal);
            return;
        };
        let Some(state) = self.reads.get_mut(&read) else {
            return;
        };
        state.route = None;
        let transaction = state.transaction;
        let read_write = self
            .transactions
            .get(&transaction)
            .is_some_and(|txn| !txn.read_only);
        if read_write && is_not_held(&refusal) {
            // The stream's Error, or the code 23 that cancelled it, is the read's answer: it waits for the transaction's end.
            return;
        }
        if read_write && refusal.code() == ErrorCode::CrossPartition {
            self.read_crossed(read, transaction, &refusal);
            return;
        }
        if let Some(txn) = self.transactions.get_mut(&transaction)
            && txn.first_read == Some(read)
            && txn.read_only
        {
            // A first read refused leaves the next read first.
            txn.first_sent = false;
        }
        self.refuse_read(read, &refusal);
    }

    /// Compute answered `read` code 23: the Bind stream is cancelled, which aborts the transaction at its partition. A first read
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
        if let Some(state) = self.reads.get_mut(&read) {
            state.route = None;
            state.tried.clear();
        }
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
            if let Some(state) = self.reads.remove(&other) {
                if let Some((link, stream)) = &state.route {
                    link.cancel(*stream);
                }
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
        let Some(state) = self.reads.get(&read) else {
            return;
        };
        if let Some((link, stream)) = &state.route {
            link.cancel(*stream);
        }
        let transaction = state.transaction;
        if let Some(txn) = self.transactions.get_mut(&transaction) {
            txn.queue
                .retain(|queued| !matches!(queued, Queued::Read(held) if *held == read));
        }
        self.read_answered(read);
    }

    /// Cancels `read`'s ComputeRead and answers it with `refusal`.
    pub(super) fn read_cancelled_answering(&mut self, read: u64, refusal: &ErrorReply) {
        if let Some(state) = self.reads.get_mut(&read)
            && let Some((link, stream)) = state.route.take()
        {
            link.cancel(stream);
        }
        self.refuse_read(read, refusal);
    }

    /// The client granted `read` credit, relayed to its compute process; a stream that had none starts its hung timer over.
    pub(super) fn read_credited(&mut self, read: u64, frames: u32) {
        let Some(state) = self.reads.get_mut(&read) else {
            return;
        };
        if state.credit == 0 {
            state.last_frame = now();
        }
        state.credit = state.credit.saturating_add(frames);
        if let Some((link, stream)) = &state.route {
            link.credit(*stream, frames);
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
            if let Some(state) = self.reads.remove(&read)
                && let Some((link, stream)) = &state.route
            {
                link.cancel(*stream);
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

/// Compute's Error 1 of `detail` 1: a Scan named a transaction its partition no longer holds.
fn is_not_held(refusal: &ErrorReply) -> bool {
    refusal.code() == ErrorCode::Protocol && refusal.detail() == DETAIL_TRANSACTION_NOT_HELD
}
