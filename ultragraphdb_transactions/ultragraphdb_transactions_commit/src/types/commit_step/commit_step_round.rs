/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{
    ABSENT, ChunkLayout, HEADER_SIZE, OperationResult, WriteSetRecord, WriteSetRecordView,
};

use crate::errors::commit_error::CommitError;
use crate::traits::operator::Operator;
use crate::traits::proposal::Proposal;
use crate::types::commit_request::CommitRequest;
use crate::types::commit_step::{CommitStep, Source, Update, Waiting};
use crate::types::reply::Reply;
use crate::types::round_record::RoundRecord;
use crate::types::step_output::StepOutput;

/// The Round body's section table: three sections of offset and length.
const TABLE: usize = 24;
/// A decision's size.
const DECISION: usize = 16;
/// A record table entry's size.
const RECORD_ENTRY: usize = 8;

/// `n` rounded up to a multiple of 8.
fn align8(n: usize) -> usize {
    n.div_ceil(8) * 8
}

/// The frame of a Round body of `decisions` decisions and `committed` records whose bytes, padding between them
/// included, are `records`: the header, the body and its chunk table.
pub(crate) fn frame_len(decisions: usize, committed: usize, records: usize) -> usize {
    let body = TABLE + DECISION * decisions + RECORD_ENTRY * committed + records;
    HEADER_SIZE + ChunkLayout::of_data(body).body_len()
}

/// The record a registration of `session` logs: a write-set record of that session, request 0 and no writes.
pub(crate) fn registration_record(session: u64) -> Vec<u8> {
    WriteSetRecord::new(0, 0, session, 0).encode()
}

/// The length of every registration's record.
fn registration_len() -> usize {
    registration_record(0).len()
}

/// How the round decided one transaction.
enum Decided {
    /// Admitted at the version, with its commit-time operations' results when it asked any.
    Admitted(u64, Option<Vec<OperationResult>>),
    /// Its commit-time operations left a record that does not fit the round: it waits for the next, decided again
    /// there from the start.
    Deferred,
    /// A retry of its session's last committed request: that commit's version and results, nothing committed.
    Retry(u64, Vec<OperationResult>),
    /// Refused.
    Refused(CommitError),
}

impl<P> Waiting<P> {
    fn ticket(&self) -> u64 {
        match self {
            Waiting::Commit(request) => request.ticket(),
            Waiting::Register(ticket) => *ticket,
        }
    }
}

impl<P: Proposal> CommitStep<P> {
    /// Closes a round over the requests waiting, within the bounds, decides it and writes its body into `out`.
    pub(crate) fn close<O: Operator<P>>(
        &mut self,
        now_us: u64,
        out: &mut StepOutput<P>,
        operator: &mut O,
    ) {
        let Some(first) = self.sequencer.open_round(now_us) else {
            self.stop(out);
            return;
        };
        operator.begin_round();
        self.admitted.clear();
        self.decided.clear();
        self.sources.clear();
        self.registrations.clear();
        self.updates.clear();
        // The requests the round takes, every one counted as committed: the sum of their records' padded lengths
        // before the last, and the last one's own.
        let (mut count, mut padded) = (0usize, 0usize);
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
                Waiting::Register(_) => registration_len(),
            };
            let limit = self.bounds.bytes() as usize;
            if count == self.bounds.transactions() as usize
                || frame_len(count + 1, count + 1, padded + len) > limit
            {
                break;
            }
            let fits = |len: usize| frame_len(count + 1, count + 1, padded + len) <= limit;
            match self.waiting.pop_front() {
                Some(Waiting::Commit(request)) => {
                    let (ticket, mut proposal) = request.into_parts();
                    let decided = self.decide(&mut proposal, &out.apply, operator, fits);
                    if !matches!(decided, Decided::Deferred) {
                        count += 1;
                        padded += align8(proposal.record().len());
                    }
                    match decided {
                        Decided::Admitted(version, results) => {
                            if let Some(results) = results {
                                self.results.push((ticket, results));
                            }
                            self.building |= proposal.starts_build();
                            let ends = proposal.ends_build();
                            if ends {
                                self.building = false;
                                while let Some(set_aside) = self.deferred.pop_back() {
                                    self.waiting.push_front(set_aside);
                                }
                            }
                            let drop = proposal.is_drop() || ends;
                            self.sources.push(Source::Txn(self.admitted.len()));
                            self.admitted.push((version, proposal));
                            self.decided.push((ticket, Ok(version)));
                            if drop {
                                break;
                            }
                        }
                        Decided::Deferred => {
                            self.waiting
                                .push_front(Waiting::Commit(CommitRequest::new(ticket, proposal)));
                            break;
                        }
                        Decided::Retry(version, results) => {
                            out.replies
                                .push(Reply::new(ticket, Ok(version)).with_results(results));
                            out.released.push((ticket, proposal));
                        }
                        Decided::Refused(refusal) => {
                            self.decided.push((ticket, Err(refusal)));
                            out.released.push((ticket, proposal));
                        }
                    }
                }
                Some(Waiting::Register(ticket)) => {
                    count += 1;
                    padded += align8(len);
                    match self.sequencer.take() {
                        Some(session) => {
                            self.sources
                                .push(Source::Registration(self.registrations.len()));
                            self.registrations.push(registration_record(session));
                            self.updates.push(Update::Registered(session));
                            self.decided.push((ticket, Ok(session)));
                        }
                        None => {
                            self.stopped = true;
                            self.decided.push((ticket, Err(CommitError::Unavailable())));
                        }
                    }
                }
                None => break,
            }
        }
        let record = RoundRecord::new(first, self.decided.len() as u32, self.sources.len() as u32);
        self.write_body(&mut out.body);
        out.append = Some(record);
        self.in_flight = Some(record);
    }

    /// Decides one transaction of the round, in the Lean model's order: a record that reads, then its session, then
    /// the resolver's checks, then its commit-time operations through `operator` over `apply`, the round handed over
    /// and not yet applied, and the round's admitted proposals, the record they leave fitting the round by `fits`;
    /// then a version, then its writes added to the history, given to the operator and its session marked busy.
    fn decide<O: Operator<P>>(
        &mut self,
        proposal: &mut P,
        apply: &[(u64, P)],
        operator: &mut O,
        fits: impl Fn(usize) -> bool,
    ) -> Decided {
        let record = match WriteSetRecordView::new(proposal.record()) {
            Ok(record) => record,
            Err(cause) => return Decided::Refused(CommitError::Malformed(cause)),
        };
        let (session, request) = (record.session(), record.request());
        match self.sessions.check(session, request) {
            Ok(Some((version, results))) => return Decided::Retry(version, results.to_vec()),
            Ok(None) => {}
            Err(refusal) => return Decided::Refused(refusal),
        }
        if let Err(refusal) = self.resolver.check(proposal.read_set()) {
            return Decided::Refused(refusal);
        }
        if let Err(refusal) = self
            .resolver
            .check_writes(proposal.read_set().read_version(), &record)
        {
            return Decided::Refused(refusal);
        }
        let (version, results) = if proposal.has_operations() {
            let mut results = Vec::new();
            let unapplied = apply.iter().chain(self.admitted.iter()).map(|(_, p)| p);
            let staged = match operator.operate(proposal, unapplied, &mut results) {
                Ok(staged) => staged,
                Err(refusal) => return Decided::Refused(refusal),
            };
            let operated = match WriteSetRecordView::new(operator.staged_record(&staged)) {
                Ok(operated) => operated,
                Err(cause) => return Decided::Refused(CommitError::Malformed(cause)),
            };
            if !fits(operated.bytes().len()) {
                return Decided::Deferred;
            }
            let Some(version) = self.sequencer.take() else {
                self.stopped = true;
                return Decided::Refused(CommitError::Unavailable());
            };
            self.resolver.admit(version, &operated);
            operator.adopt(proposal, staged);
            (version, Some(results))
        } else {
            let Some(version) = self.sequencer.take() else {
                self.stopped = true;
                return Decided::Refused(CommitError::Unavailable());
            };
            self.resolver.admit(version, &record);
            (version, None)
        };
        operator.admitted(proposal);
        if session != 0 {
            self.sessions.begin(session);
            self.updates.push(Update::Committed {
                session,
                request,
                version,
                results: results.clone().unwrap_or_default(),
            });
        }
        Decided::Admitted(version, results)
    }

    /// The record of the round's committed decision from `source`.
    fn record_of(&self, source: Source) -> &[u8] {
        match source {
            Source::Txn(i) => self.admitted.get(i).map_or(&[], |(_, p)| p.record()),
            Source::Registration(i) => self.registrations.get(i).map_or(&[], Vec::as_slice),
        }
    }

    /// Writes the round's body into `body` in the Round layout: the section table, the decisions, the record table
    /// and the committed records, each section at the end of the one before rounded up to 8.
    fn write_body(&self, body: &mut Vec<u8>) {
        body.clear();
        let decisions = DECISION * self.decided.len();
        let table = RECORD_ENTRY * self.sources.len();
        let records = self
            .sources
            .iter()
            .map(|s| self.record_of(*s).len())
            .fold(0, |end, len| align8(end) + len);
        body.reserve(TABLE + decisions + table + records);
        let sections = [
            (TABLE, decisions),
            (TABLE + decisions, table),
            (TABLE + decisions + table, records),
        ];
        for (offset, len) in sections {
            // A round is at most the frame, far below u32 offsets.
            body.extend_from_slice(&(offset as u32).to_le_bytes());
            body.extend_from_slice(&(len as u32).to_le_bytes());
        }
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
        let start = body.len();
        for source in &self.sources {
            body.resize(start + align8(body.len() - start), 0);
            body.extend_from_slice(self.record_of(*source));
        }
    }

    /// Stops the step: the round in flight and every request waiting are answered unavailable, and so is every later
    /// request.
    pub(crate) fn stop(&mut self, out: &mut StepOutput<P>) {
        self.stopped = true;
        self.in_flight = None;
        self.updates.clear();
        self.results.clear();
        let unavailable = |ticket| Reply::new(ticket, Err(CommitError::Unavailable()));
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
        self.sources.clear();
        out.replies.extend(
            self.decided
                .drain(..)
                .map(|(ticket, _)| unavailable(ticket)),
        );
        for waiting in self.deferred.drain(..).chain(self.waiting.drain(..)) {
            out.replies.push(unavailable(waiting.ticket()));
            if let Waiting::Commit(request) = waiting {
                out.released.push(request.into_parts());
            }
        }
    }
}
