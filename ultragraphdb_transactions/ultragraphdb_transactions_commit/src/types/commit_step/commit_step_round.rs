/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_cluster_sequencer::Close;
use ultragraphdb_core_ids::{SessionToken, Version};
use ultragraphdb_protocol_wire::{
    ABSENT, ChunkLayout, ConflictKind, HEADER_SIZE, OperationResult, RecordKind, TermTable,
    WriteSetRecordView,
};
use ultragraphdb_traits_store::VersionlessEntry;

use crate::errors::commit_error::{CommitError, CommitErrorEnum};
use crate::traits::operator::Operator;
use crate::traits::proposal::Proposal;
use crate::types::commit_request::CommitRequest;
use crate::types::commit_step::{CommitStep, Source, Update, Waiting};
use crate::types::replay::Replay;
use crate::types::reply::Reply;
use crate::types::round_record::RoundRecord;
use crate::types::step_output::StepOutput;
use crate::types::stored_reply::StoredReply;

/// The Round body's section table: four sections of offset and length.
const TABLE: usize = 32;
/// A decision's size.
const DECISION: usize = 16;
/// A record table entry's size.
const RECORD_ENTRY: usize = 8;
/// A key-value entry's lengths, before its key.
const ENTRY_HEADER: usize = 8;

/// `n` rounded up to a multiple of 8.
fn align8(n: usize) -> usize {
    n.div_ceil(8) * 8
}

/// The frame of a Round body of `decisions` decisions and `committed` records whose bytes, padding between them
/// included, are `records`, and no dictionary entry: the header, the body and its chunk table.
pub(crate) fn frame_len(decisions: usize, committed: usize, records: usize) -> usize {
    frame_with(decisions, committed, records, 0)
}

/// The same, with `dictionary` bytes of dictionary entries.
fn frame_with(decisions: usize, committed: usize, records: usize, dictionary: usize) -> usize {
    let body =
        TABLE + DECISION * decisions + RECORD_ENTRY * committed + align8(records) + dictionary;
    HEADER_SIZE + ChunkLayout::of_data(body).body_len()
}

/// The bytes the key-value entry of `key` and `value` takes, its padding included.
fn entry_len(key: &[u8], value: &[u8]) -> usize {
    align8(ENTRY_HEADER + key.len() + value.len())
}

/// How the round decided one entry.
enum Decided {
    /// Admitted at the version, with its commit-time operations' results and their terms when it asked any.
    Admitted(u64, Option<(Vec<OperationResult>, TermTable)>),
    /// It waits for the next round: its operations left a record this round has no room for, or the open epoch filled.
    Deferred,
    /// A retry of its session's last committed request: that commit's stored reply, nothing committed.
    Retry(StoredReply),
    /// Refused.
    Refused(CommitError),
}

impl<P> Waiting<P> {
    fn ticket(&self) -> u64 {
        match self {
            Waiting::Commit(request) => request.ticket(),
            Waiting::Registration { ticket, .. } => *ticket,
        }
    }

    /// The size of the entry's record.
    fn record_len(&self) -> usize
    where
        P: Proposal,
    {
        match self {
            Waiting::Commit(request) => request.proposal().record().len(),
            Waiting::Registration { record, .. } => record.len(),
        }
    }

    /// The size of the entry's write set: its record and the dictionary entries its round holds for it.
    fn write_set_len(&self) -> usize
    where
        P: Proposal,
    {
        let dictionary: usize = match self {
            Waiting::Commit(request) => request
                .proposal()
                .dictionary()
                .map(|(k, v)| entry_len(k, v))
                .sum(),
            Waiting::Registration { dictionary, .. } => {
                dictionary.iter().map(|(k, v)| entry_len(k, v)).sum()
            }
        };
        self.record_len().saturating_add(dictionary)
    }
}

impl<P: Proposal> CommitStep<P> {
    /// An entry arrives: refused when the step is stopped, its record is over the largest a round holds, the
    /// sequencer counts as unreachable or the queue is full; held when it carries a session token of an epoch not yet
    /// open; else it waits, and a round closes at once when none is in flight.
    pub(crate) fn arrive<O: Operator<P>>(
        &mut self,
        entry: Waiting<P>,
        now_us: u64,
        out: &mut StepOutput<P>,
        operator: &mut O,
    ) {
        self.expire(now_us, out);
        let size = entry.write_set_len();
        let limit = self.bounds.max_write_set();
        let refusal = if self.stopped {
            Some(CommitError::unavailable())
        } else if size > limit {
            Some(CommitError::too_large(size, limit))
        } else if self.unreachable(now_us)
            || self.waiting.len() + self.deferred.len() + self.held.len() >= self.queue
        {
            Some(CommitError::busy(self.queue))
        } else {
            None
        };
        if let Some(refusal) = refusal {
            refuse(entry, refusal, out);
            return;
        }
        match entry {
            Waiting::Commit(request) if self.waits(request.proposal()) => {
                self.held.push_back(request);
            }
            entry @ (Waiting::Commit(_) | Waiting::Registration { .. }) => {
                self.waiting.push_back(entry);
                if self.in_flight.is_none() {
                    self.close(now_us, out, operator);
                }
            }
        }
    }

    /// Whether `proposal` may not be stamped at the open epoch: the larger of its session token's epoch and its epoch
    /// floor is above it, so it is held until the close before that epoch.
    fn waits(&self, proposal: &P) -> bool {
        let earliest = proposal
            .token()
            .map(SessionToken::epoch)
            .max(proposal.epoch_floor());
        match (earliest, self.stamp.open()) {
            (Some(earliest), Some(open)) => earliest > open,
            (Some(_) | None, None) | (None, Some(_)) => false,
        }
    }

    /// Whether the close the step asked for has gone unanswered for `close_request_ms` at `now_us`.
    fn unreachable(&self, now_us: u64) -> bool {
        self.asked_us
            .is_some_and(|asked| now_us.saturating_sub(asked) >= self.close_request.micros())
    }

    /// Refuses busy every entry waiting for the close the step asked for, and every commit held for its token or its
    /// epoch floor, which wait for later closes of the same sequencer, once the request has gone unanswered for
    /// `close_request_ms`.
    pub(crate) fn expire(&mut self, now_us: u64, out: &mut StepOutput<P>) {
        if !self.unreachable(now_us) || !self.stamp.is_full() {
            return;
        }
        let busy = CommitError::busy(self.queue);
        // Each turn takes one entry off the queue.
        while let Some(entry) = self.waiting.pop_front() {
            refuse(entry, busy.clone(), out);
        }
        // Each turn takes one commit off the held queue.
        while let Some(request) = self.held.pop_front() {
            refuse(Waiting::Commit(request), busy.clone(), out);
        }
    }

    /// A close is delivered: the epoch after it opens, its catalog versions take effect, the sessions it evicts are
    /// purged, the commits held for it wait, and a round closes over them when none is in flight. A close past a gap
    /// stops the step.
    pub(crate) fn deliver<O: Operator<P>>(
        &mut self,
        close: &Close,
        now_us: u64,
        out: &mut StepOutput<P>,
        operator: &mut O,
    ) {
        if self.stopped {
            return;
        }
        let delivered = self.stamp.delivered();
        match self.stamp.deliver(close.epoch()) {
            None => {
                self.fault = Some(CommitError::close_out_of_order(
                    delivered.get(),
                    close.epoch().get(),
                ));
                self.stop(out);
                return;
            }
            Some(false) => return,
            Some(true) => {}
        }
        for version in close.catalog() {
            let tenant = version.tenant().get();
            self.catalog.retain(|(n, _)| *n != tenant);
            self.catalog.push((tenant, version.version()));
        }
        self.sessions.purge(close.evictions());
        self.asked_us = None;
        let held = std::mem::take(&mut self.held);
        for request in held {
            if self.waits(request.proposal()) {
                self.held.push_back(request);
            } else {
                self.waiting.push_back(Waiting::Commit(request));
            }
        }
        if self.stamp.open().is_none() {
            self.stop(out);
        } else if self.in_flight.is_none() && !self.waiting.is_empty() {
            self.close(now_us, out, operator);
        }
    }

    /// The round in flight is synced: it is handed over to apply with its replies, each commit's with its session
    /// token, the session table takes its commits, and the next round closes over the entries waiting.
    pub(crate) fn synced<O: Operator<P>>(
        &mut self,
        now_us: u64,
        out: &mut StepOutput<P>,
        operator: &mut O,
    ) {
        let Some(record) = self.in_flight.take() else {
            return;
        };
        out.applied = Some(record);
        std::mem::swap(&mut out.apply, &mut self.admitted);
        if let Some(last) = record.last_version() {
            self.watermark = last;
        }
        let position = self.in_flight_position;
        let token = |version: u64| {
            SessionToken::new(self.partition, position, Version::from_u64(version).epoch())
        };
        for update in self.updates.drain(..) {
            let reply = StoredReply::new(
                update.version,
                token(update.version),
                update.results,
                update.terms,
            );
            self.sessions
                .committed(update.session, update.request, reply);
        }
        self.sessions.drain_entries(&mut out.system);
        out.system.push(VersionlessEntry::put(
            Replay::APPLIED_POSITION_KEY.to_vec(),
            position.to_le_bytes().to_vec(),
        ));
        // The round's committed registrations, in round order, at their versions: their homes' writes apply with it.
        let committed = self
            .decided
            .iter()
            .filter_map(|(ticket, outcome)| Some((*ticket, *outcome.as_ref().ok()?)));
        out.registered.extend(
            committed
                .zip(self.sources.iter())
                .filter(|(_, source)| matches!(source, Source::Registration(_)))
                .map(|(decision, _)| decision),
        );
        let results = &mut self.results;
        for (ticket, outcome) in self.decided.drain(..) {
            let reply = match &outcome {
                Ok(version) => Reply::new(ticket, outcome.clone()).with_token(token(*version)),
                Err(_) => Reply::new(ticket, outcome),
            };
            let reply = match results.iter().position(|(t, _, _)| *t == ticket) {
                Some(at) => {
                    let (_, found, terms) = results.swap_remove(at);
                    reply.with_results(found, terms)
                }
                None => reply,
            };
            out.replies.push(reply);
        }
        results.clear();
        if !self.waiting.is_empty() {
            self.close(now_us, out, operator);
        }
    }

    /// Closes a round over the entries waiting, within the bounds and the open epoch's sequences, decides it and
    /// writes its body into `out`. With the open epoch full it closes none and asks the sequencer for its close; with
    /// no epoch left it stops the step.
    pub(crate) fn close<O: Operator<P>>(
        &mut self,
        now_us: u64,
        out: &mut StepOutput<P>,
        operator: &mut O,
    ) {
        let Some(open) = self.stamp.open() else {
            self.stop(out);
            return;
        };
        if self.stamp.is_full() {
            self.ask(open, now_us, out);
            return;
        }
        let first = self.stamp.peek().map_or(0, Version::to_u64);
        operator.begin_round();
        self.admitted.clear();
        self.decided.clear();
        self.sources.clear();
        self.registrations.clear();
        self.dictionary.clear();
        self.updates.clear();
        self.gather(out, operator);
        if self.stamp.is_full() && !self.waiting.is_empty() {
            self.ask(open, now_us, out);
        }
        if self.decided.is_empty() {
            return;
        }
        let record = RoundRecord::new(first, self.decided.len() as u32, self.sources.len() as u32);
        debug_assert_eq!(
            self.sources.len(),
            self.decided.iter().filter(|(_, o)| o.is_ok()).count()
        );
        self.position = self.position.saturating_add(1);
        self.in_flight_position = self.position;
        self.write_body(&mut out.body);
        out.append = Some(record);
        out.position = self.position;
        self.in_flight = Some(record);
    }

    /// Asks the sequencer for the close of the full `open` epoch, once.
    fn ask(&mut self, open: ultragraphdb_core_ids::Epoch, now_us: u64, out: &mut StepOutput<P>) {
        if self.asked_us.is_none() {
            self.asked_us = Some(now_us);
            out.close_request = Some(open);
        }
    }

    /// Takes the entries waiting into the round, in arrival order, while its frame stays within the bounds' bytes and
    /// its decisions within their count, each deciding as it is taken.
    fn gather<O: Operator<P>>(&mut self, out: &mut StepOutput<P>, operator: &mut O) {
        let (mut count, mut padded, mut dictionary) = (0usize, 0usize, 0usize);
        let limit = self.bounds.bytes() as usize;
        // Each turn takes the entry at the queue's head into the round or sets it aside, or ends the round, so it ends
        // within the entries waiting.
        while let Some(next) = self.waiting.front() {
            let len = match next {
                Waiting::Commit(request) if self.building && request.proposal().starts_build() => {
                    if let Some(set_aside) = self.waiting.pop_front() {
                        self.deferred.push_back(set_aside);
                    }
                    continue;
                }
                // A build's end is a round of its own.
                Waiting::Commit(request) if request.proposal().ends_build() && count > 0 => break,
                Waiting::Commit(request) => request.proposal().record().len(),
                Waiting::Registration { record, .. } => record.len(),
            };
            let entries: usize = match next {
                Waiting::Commit(request) => request
                    .proposal()
                    .dictionary()
                    .map(|(k, v)| entry_len(k, v))
                    .sum(),
                Waiting::Registration {
                    dictionary: entries,
                    ..
                } => entries.iter().map(|(k, v)| entry_len(k, v)).sum(),
            };
            if count == self.bounds.transactions() as usize
                || frame_with(count + 1, count + 1, padded + len, dictionary + entries) > limit
            {
                break;
            }
            // An entry's commit-time operations may leave a longer record and more dictionary entries: the round is
            // checked again with what they leave.
            let fits = |record_len: usize, entries_len: usize| {
                frame_with(
                    count + 1,
                    count + 1,
                    padded + record_len,
                    dictionary + entries_len,
                ) <= limit
            };
            let Some(entry) = self.waiting.pop_front() else {
                break;
            };
            let (taken, record_len, entries_len, more) =
                self.take_entry(entry, out, operator, fits);
            if taken {
                count += 1;
                padded += align8(record_len);
                dictionary += entries_len;
            }
            if !more {
                break;
            }
        }
    }

    /// Decides `entry` into the round: whether it took a decision, its record's length and the bytes of its
    /// dictionary entries as decided, what its operations left included, and whether the round takes more after it.
    fn take_entry<O: Operator<P>>(
        &mut self,
        entry: Waiting<P>,
        out: &mut StepOutput<P>,
        operator: &mut O,
        fits: impl Fn(usize, usize) -> bool,
    ) -> (bool, usize, usize, bool) {
        match entry {
            Waiting::Commit(request) => {
                let (ticket, mut proposal) = request.into_parts();
                let decided = self.decide(&mut proposal, &out.apply, operator, fits);
                let len = proposal.record().len();
                let entries = proposal.dictionary().map(|(k, v)| entry_len(k, v)).sum();
                let taken = matches!(decided, Decided::Admitted(..) | Decided::Refused(_));
                let more = self.record_decision(ticket, proposal, decided, out);
                (taken, len, entries, more)
            }
            Waiting::Registration {
                ticket,
                record,
                dictionary,
            } => {
                let len = record.len();
                let entries: usize = dictionary.iter().map(|(k, v)| entry_len(k, v)).sum();
                let more = self.admit_registration(ticket, record, dictionary, out);
                (more, len, if more { entries } else { 0 }, more)
            }
        }
    }

    /// Records how the round decided the transaction of `ticket`: admitted, set back to wait, answered from its
    /// session or refused and released. `false` when the round takes nothing after it.
    fn record_decision(
        &mut self,
        ticket: u64,
        proposal: P,
        decided: Decided,
        out: &mut StepOutput<P>,
    ) -> bool {
        match decided {
            Decided::Admitted(version, results) => {
                if let Some((results, terms)) = results {
                    self.results.push((ticket, results, terms));
                }
                self.building |= proposal.starts_build();
                let ends = proposal.ends_build();
                if ends {
                    self.building = false;
                    // Each turn moves one proposal set aside back to the queue.
                    while let Some(set_aside) = self.deferred.pop_back() {
                        self.waiting.push_front(set_aside);
                    }
                }
                let drop = proposal.is_drop() || ends;
                for (key, value) in proposal.dictionary() {
                    let _previous: Option<Vec<u8>> =
                        self.dictionary.insert(key.to_vec(), value.to_vec());
                }
                self.sources.push(Source::Txn(self.admitted.len()));
                self.admitted.push((version, proposal));
                self.decided.push((ticket, Ok(version)));
                !drop
            }
            Decided::Deferred => {
                self.waiting
                    .push_front(Waiting::Commit(CommitRequest::new(ticket, proposal)));
                false
            }
            Decided::Retry(reply) => {
                let answer = Reply::new(ticket, Ok(reply.version()))
                    .with_token(reply.token().clone())
                    .with_results(reply.results().to_vec(), reply.terms().clone());
                out.replies.push(answer);
                out.released.push((ticket, proposal));
                true
            }
            Decided::Refused(refusal) => {
                self.decided.push((ticket, Err(refusal)));
                out.released.push((ticket, proposal));
                true
            }
        }
    }

    /// Admits the registration a home accepted under `ticket` with its own commit version, or refuses one whose record
    /// is no registration's; with the open epoch full it waits. `false` when the round takes nothing after it.
    fn admit_registration(
        &mut self,
        ticket: u64,
        record: Vec<u8>,
        dictionary: Vec<(Vec<u8>, Vec<u8>)>,
        out: &mut StepOutput<P>,
    ) -> bool {
        let kind = WriteSetRecordView::new(&record).map(|view| view.kind());
        match kind {
            Ok(RecordKind::Registration) => {}
            Ok(other @ (RecordKind::Transaction | RecordKind::Build)) => {
                let cause = ultragraphdb_error_protocol::ProtocolError::field_value(
                    17,
                    "record kind",
                    u64::from(other.code()),
                );
                self.decided
                    .push((ticket, Err(CommitError::malformed(cause))));
                out.dismissed.push(ticket);
                return true;
            }
            Err(cause) => {
                self.decided
                    .push((ticket, Err(CommitError::malformed(cause))));
                out.dismissed.push(ticket);
                return true;
            }
        }
        let Some(version) = self.stamp.take() else {
            self.waiting.push_front(Waiting::Registration {
                ticket,
                record,
                dictionary,
            });
            return false;
        };
        self.sources
            .push(Source::Registration(self.registrations.len()));
        self.registrations.push(record);
        self.dictionary.extend(dictionary);
        self.decided.push((ticket, Ok(version.to_u64())));
        true
    }

    /// Decides one transaction of the round, in the Lean model's order: a record that reads, then its session, then the
    /// resolver's checks with the catalog version's between the reads' and a drop's, then its commit-time operations
    /// through `operator` over `apply` and the round's admitted proposals, the record and dictionary entries they leave
    /// fitting the round by `fits`; then a version, then its writes added to the history and its session marked busy.
    fn decide<O: Operator<P>>(
        &mut self,
        proposal: &mut P,
        apply: &[(u64, P)],
        operator: &mut O,
        fits: impl Fn(usize, usize) -> bool,
    ) -> Decided {
        let record = match WriteSetRecordView::new(proposal.record()) {
            Ok(record) => record,
            Err(cause) => return Decided::Refused(CommitError::malformed(cause)),
        };
        let (session, request) = (u128::from_be_bytes(record.session()), record.request());
        match self.sessions.check(session, request) {
            Ok(Some(reply)) => return Decided::Retry(reply.clone()),
            Ok(None) => {}
            Err(refusal) => return Decided::Refused(refusal),
        }
        if let Err(refusal) = self.validate(proposal, &record) {
            return Decided::Refused(refusal);
        }
        let decided = if proposal.has_operations() {
            self.operate(proposal, apply, operator, fits)
        } else {
            match self.stamp.take() {
                Some(version) => {
                    self.resolver.admit(version.to_u64(), &record);
                    Decided::Admitted(version.to_u64(), None)
                }
                None => Decided::Deferred,
            }
        };
        if let Decided::Admitted(version, results) = &decided {
            operator.admitted(proposal);
            if session != 0 {
                self.sessions.begin(session);
                let (found, terms) = results.clone().unwrap_or_default();
                self.updates.push(Update {
                    session,
                    request,
                    version: *version,
                    results: found,
                    terms,
                });
            }
        }
        decided
    }

    /// The resolver's checks of `proposal`, the catalog version's placed after the reads' and before a drop's.
    fn validate(&self, proposal: &P, record: &WriteSetRecordView<'_>) -> Result<(), CommitError> {
        let stale = record.kind() == RecordKind::Transaction
            && record.operation_count() == 0
            && record.catalog() != self.catalog_version(record.tenant());
        match self.resolver.check(proposal.read_set()) {
            Err(CommitError(CommitErrorEnum::Conflict {
                kind: ConflictKind::Drop,
            })) if stale => Err(CommitError::conflict(ConflictKind::Catalog)),
            Err(refusal) => Err(refusal),
            Ok(()) if stale => Err(CommitError::conflict(ConflictKind::Catalog)),
            Ok(()) => self
                .resolver
                .check_writes(proposal.read_set().read_version(), record),
        }
    }

    /// Runs `proposal`'s operations and admits what they leave at the next version, with the operations' results and
    /// terms; refused, or set back to wait for a round with room or the next epoch. The round must hold what they leave,
    /// the record and the dictionary entries both, which the operations may have added to. What the operations leave is
    /// adopted only when admitted; otherwise the operator discards it, releasing what the operations hold.
    fn operate<O: Operator<P>>(
        &mut self,
        proposal: &mut P,
        apply: &[(u64, P)],
        operator: &mut O,
        fits: impl Fn(usize, usize) -> bool,
    ) -> Decided {
        let mut results = Vec::new();
        let unapplied = apply.iter().chain(self.admitted.iter()).map(|(_, p)| p);
        let staged = match operator.operate(proposal, unapplied, &mut results) {
            Ok(staged) => staged,
            Err(refusal) => return Decided::Refused(refusal),
        };
        let operated = match WriteSetRecordView::new(operator.staged_record(&staged)) {
            Ok(operated) => operated,
            Err(cause) => {
                operator.discard(proposal, staged);
                return Decided::Refused(CommitError::malformed(cause));
            }
        };
        let size = operated.bytes().len();
        let entries: usize = operator
            .staged_dictionary(&staged)
            .map(|(k, v)| entry_len(k, v))
            .sum();
        if !fits(size, entries) {
            operator.discard(proposal, staged);
            // A write set no round holds on its own would wait at the head of every round: refused instead. Entries
            // are padded to 8 bytes, so a round of it alone holds it exactly when the record and its entries together
            // are at most the largest record.
            let (write_set, limit) = (size.saturating_add(entries), self.bounds.max_write_set());
            if write_set > limit {
                return Decided::Refused(CommitError::too_large(write_set, limit));
            }
            return Decided::Deferred;
        }
        let Some(version) = self.stamp.take() else {
            operator.discard(proposal, staged);
            return Decided::Deferred;
        };
        let terms = operated.result_terms().to_table();
        self.resolver.admit(version.to_u64(), &operated);
        operator.adopt(proposal, staged);
        Decided::Admitted(version.to_u64(), Some((results, terms)))
    }

    /// The record of the round's committed decision from `source`.
    fn record_of(&self, source: Source) -> &[u8] {
        match source {
            Source::Txn(i) => self.admitted.get(i).map_or(&[], |(_, p)| p.record()),
            Source::Registration(i) => self.registrations.get(i).map_or(&[], Vec::as_slice),
        }
    }

    /// Writes the round's body into `body` in the Round layout: the section table, the decisions, the record table,
    /// the committed records and the dictionary entries, each section at the end of the one before rounded up to 8.
    fn write_body(&self, body: &mut Vec<u8>) {
        body.clear();
        let decisions = DECISION * self.decided.len();
        let table = RECORD_ENTRY * self.sources.len();
        let records = self
            .sources
            .iter()
            .map(|s| self.record_of(*s).len())
            .fold(0, |end, len| align8(end) + len);
        let dictionary: usize = self.dictionary.iter().map(|(k, v)| entry_len(k, v)).sum();
        let at_dictionary = align8(TABLE + decisions + table + records);
        body.reserve(at_dictionary + dictionary);
        let sections = [
            (TABLE, decisions),
            (TABLE + decisions, table),
            (TABLE + decisions + table, records),
            (at_dictionary, dictionary),
        ];
        for (offset, len) in sections {
            // A round is at most the frame, far below u32 offsets.
            body.extend_from_slice(&(offset as u32).to_le_bytes());
            body.extend_from_slice(&(len as u32).to_le_bytes());
        }
        self.write_decisions(body);
        let start = body.len();
        for source in &self.sources {
            body.resize(start + align8(body.len() - start), 0);
            body.extend_from_slice(self.record_of(*source));
        }
        body.resize(at_dictionary, 0);
        for (key, value) in &self.dictionary {
            body.extend_from_slice(&(key.len() as u32).to_le_bytes());
            body.extend_from_slice(&(value.len() as u32).to_le_bytes());
            body.extend_from_slice(key);
            body.extend_from_slice(value);
            body.resize(align8(body.len()), 0);
        }
        // The body is exactly the sections its table names.
        debug_assert_eq!(start, TABLE + decisions + table);
        debug_assert_eq!(body.len(), at_dictionary + dictionary);
    }

    /// Writes the round's decisions and its record table into `body`.
    fn write_decisions(&self, body: &mut Vec<u8>) {
        let mut committed = 0u32;
        for (_, outcome) in &self.decided {
            let (code, record, detail) = match outcome {
                Ok(_) => {
                    committed += 1;
                    (0u16, committed - 1, 0u64)
                }
                Err(refusal) => (refusal.code().number(), ABSENT, refusal.detail()),
            };
            body.extend_from_slice(&code.to_le_bytes());
            body.extend_from_slice(&[0, 0]);
            body.extend_from_slice(&record.to_le_bytes());
            body.extend_from_slice(&detail.to_le_bytes());
        }
        let mut end = 0usize;
        for source in &self.sources {
            let len = self.record_of(*source).len();
            let at = align8(end);
            body.extend_from_slice(&(at as u32).to_le_bytes());
            body.extend_from_slice(&(len as u32).to_le_bytes());
            end = at + len;
        }
    }

    /// Stops the step: the round in flight and every entry waiting or held are answered unavailable, and so is every
    /// later one.
    pub(crate) fn stop(&mut self, out: &mut StepOutput<P>) {
        self.stopped = true;
        self.in_flight = None;
        self.updates.clear();
        self.results.clear();
        let unavailable = |ticket| Reply::new(ticket, Err(CommitError::unavailable()));
        // The round's admitted proposals follow its committed transactions in round order: each is released under the
        // ticket of its decision. Its aborts and retries were released when the round closed.
        let tickets = self
            .decided
            .iter()
            .filter(|(_, outcome)| outcome.is_ok())
            .map(|(ticket, _)| *ticket)
            .zip(self.sources.iter())
            .filter(|(_, source)| matches!(source, Source::Txn(_)))
            .map(|(ticket, _)| ticket);
        out.released.extend(
            tickets
                .zip(self.admitted.drain(..))
                .map(|(ticket, (_, proposal))| (ticket, proposal)),
        );
        // Its registrations are dismissed under their tickets the same way.
        let registrations = self
            .decided
            .iter()
            .filter(|(_, outcome)| outcome.is_ok())
            .map(|(ticket, _)| *ticket)
            .zip(self.sources.iter())
            .filter(|(_, source)| matches!(source, Source::Registration(_)))
            .map(|(ticket, _)| ticket);
        out.dismissed.extend(registrations);
        self.sources.clear();
        out.replies.extend(
            self.decided
                .drain(..)
                .map(|(ticket, _)| unavailable(ticket)),
        );
        let held = self.held.drain(..).map(Waiting::Commit);
        let waiting: Vec<Waiting<P>> = self
            .deferred
            .drain(..)
            .chain(self.waiting.drain(..))
            .chain(held)
            .collect();
        for entry in waiting {
            refuse(entry, CommitError::unavailable(), out);
        }
    }
}

/// Answers `entry` with `refusal`, releasing a commit's proposal and dismissing a registration.
fn refuse<P>(entry: Waiting<P>, refusal: CommitError, out: &mut StepOutput<P>) {
    out.replies.push(Reply::new(entry.ticket(), Err(refusal)));
    match entry {
        Waiting::Commit(request) => out.released.push(request.into_parts()),
        Waiting::Registration { ticket, .. } => out.dismissed.push(ticket),
    }
}
