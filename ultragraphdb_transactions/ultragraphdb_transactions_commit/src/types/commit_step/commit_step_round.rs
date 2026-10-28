/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{ABSENT, ChunkLayout, HEADER_SIZE, WriteSetRecordView};

use crate::errors::commit_error::CommitError;
use crate::traits::proposal::Proposal;
use crate::types::commit_step::CommitStep;
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

impl<P: Proposal> CommitStep<P> {
    /// Closes a round over the requests waiting, within the bounds, decides it and writes its body into `out`.
    pub(crate) fn close(&mut self, now_us: u64, out: &mut StepOutput<P>) {
        let Some(first) = self.sequencer.open_round(now_us) else {
            self.stop(out);
            return;
        };
        self.admitted.clear();
        self.decided.clear();
        // The requests the round takes, every one counted as committed: the sum of their records' padded lengths
        // before the last, and the last one's own.
        let (mut count, mut padded) = (0usize, 0usize);
        while let Some(next) = self.waiting.front() {
            let len = next.proposal().record().len();
            if count == self.bounds.transactions() as usize
                || frame_len(count + 1, count + 1, padded + len) > self.bounds.bytes() as usize
            {
                break;
            }
            count += 1;
            padded += align8(len);
            if let Some(request) = self.waiting.pop_front() {
                let (ticket, proposal) = request.into_parts();
                match self.decide(&proposal) {
                    Ok(version) => {
                        self.admitted.push((version, proposal));
                        self.decided.push((ticket, Ok(version)));
                    }
                    Err(refusal) => self.decided.push((ticket, Err(refusal))),
                }
            }
        }
        let record = RoundRecord::new(first, count as u32, self.admitted.len() as u32);
        self.write_body(&mut out.body);
        out.append = Some(record);
        self.in_flight = Some(record);
    }

    /// Decides one transaction of the round: a record that reads, then the resolver's checks, then a version, then its
    /// writes added to the history.
    fn decide(&mut self, proposal: &P) -> Result<u64, CommitError> {
        let record = WriteSetRecordView::new(proposal.record()).map_err(CommitError::Malformed)?;
        self.resolver.check(proposal.read_set())?;
        let Some(version) = self.sequencer.take() else {
            self.stopped = true;
            return Err(CommitError::Unavailable());
        };
        self.resolver.admit(version, &record);
        Ok(version)
    }

    /// Writes the round's body into `body` in the Round layout: the section table, the decisions, the record table
    /// and the admitted records, each section at the end of the one before rounded up to 8.
    fn write_body(&self, body: &mut Vec<u8>) {
        body.clear();
        let decisions = DECISION * self.decided.len();
        let table = RECORD_ENTRY * self.admitted.len();
        let records = self
            .admitted
            .iter()
            .map(|(_, p)| p.record().len())
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
        for (_, p) in &self.admitted {
            let at = align8(end);
            body.extend_from_slice(&(at as u32).to_le_bytes());
            body.extend_from_slice(&(p.record().len() as u32).to_le_bytes());
            end = at + p.record().len();
        }
        let start = body.len();
        for (_, p) in &self.admitted {
            body.resize(start + align8(body.len() - start), 0);
            body.extend_from_slice(p.record());
        }
    }

    /// Stops the step: the round in flight and every request waiting are answered unavailable, and so is every later
    /// request.
    pub(crate) fn stop(&mut self, out: &mut StepOutput<P>) {
        self.stopped = true;
        self.in_flight = None;
        self.admitted.clear();
        let unavailable = |ticket| Reply::new(ticket, Err(CommitError::Unavailable()));
        out.replies.extend(
            self.decided
                .drain(..)
                .map(|(ticket, _)| unavailable(ticket)),
        );
        out.replies.extend(
            self.waiting
                .drain(..)
                .map(|request| unavailable(request.ticket())),
        );
    }
}
