/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_cluster::{ClusterError, ClusterErrorEnum};
use ultragraphdb_traits_log::Position;

use crate::types::reader::Reader;
use crate::types::sequencer_record::SequencerRecord;
use crate::types::sequencer_snapshot::{Part, SequencerSnapshot, first_disorder};

/// The format byte of the encoding.
const FORMAT: u8 = 1;

/// The most records of either part, the most the settings can retain, 2^32 − 1.
/// The cast is lossless on every target the node builds for, whose `usize` holds 32 bits or more.
const RECORDS_MAX: usize = u32::MAX as usize;

impl SequencerSnapshot {
    /// Appends the snapshot's encoding to `out`, big-endian throughout: the format byte 1 and the position as 8 bytes;
    /// the closes' count as 4 bytes and each close as its record's encoding ([`crate::SequencerRecord::encode`]) after
    /// its length as 4; the transactions' count as 4 and each transaction's position as 8 and its record's encoding
    /// after its length as 4. A snapshot has one encoding.
    pub fn encode(&self, out: &mut Vec<u8>) {
        out.push(FORMAT);
        out.extend_from_slice(&self.position().get().to_be_bytes());
        out.extend_from_slice(&count_u32(self.closes().len()).to_be_bytes());
        let mut record = Vec::new();
        for close in self.closes() {
            record.clear();
            SequencerRecord::Close(close.clone()).encode(&mut record);
            put_framed(out, &record);
        }
        out.extend_from_slice(&count_u32(self.transactions().len()).to_be_bytes());
        for (at, transaction) in self.transactions() {
            out.extend_from_slice(&at.get().to_be_bytes());
            record.clear();
            SequencerRecord::Transaction(transaction.clone()).encode(&mut record);
            put_framed(out, &record);
        }
    }

    /// The snapshot `bytes` encode, exactly as [`SequencerSnapshot::encode`] writes it.
    ///
    /// # Errors
    ///
    /// [`ClusterError::malformed`] naming the first value that is not as the encoding writes it, and its offset: a
    /// format byte other than 1, a record that is not the close or the transaction its place holds, a value out of the
    /// order [`SequencerSnapshot::new`] checks (at the offset of its entry), a truncated value, bytes after the end; the
    /// refusals of [`crate::SequencerRecord::decode`] for a record's bytes, a malformed one's offset counted from the
    /// snapshot's first byte.
    pub fn decode(bytes: &[u8]) -> Result<Self, ClusterError> {
        let mut reader = Reader::new(bytes);
        if reader.u8("the format byte 1")? != FORMAT {
            return Err(ClusterError::malformed("the format byte 1", 0));
        }
        let position = Position::new(reader.u64("a snapshot's position")?);
        // Each count and length below is a `u32`, held whole by a `usize` of 32 bits or more.
        let closes_count = reader.u32("the closes' count")? as usize;
        let mut closes = Vec::new();
        let mut close_at = Vec::new();
        for _ in 0..closes_count.min(RECORDS_MAX) {
            close_at.push(reader.offset());
            let (at, record) = read_framed(&mut reader)?;
            let SequencerRecord::Close(close) = record else {
                return Err(ClusterError::malformed("a close", at));
            };
            closes.push(close);
        }
        let transactions_count = reader.u32("the transactions' count")? as usize; // a `u32`, lossless
        let mut transactions = Vec::new();
        let mut transaction_at = Vec::new();
        for _ in 0..transactions_count.min(RECORDS_MAX) {
            transaction_at.push(reader.offset());
            let placed = Position::new(reader.u64("a transaction's position")?);
            let (at, record) = read_framed(&mut reader)?;
            let SequencerRecord::Transaction(transaction) = record else {
                return Err(ClusterError::malformed("a transaction", at));
            };
            transactions.push((placed, transaction));
        }
        reader.finish_as("the end of the snapshot")?;
        if let Some((part, index, what)) = first_disorder(position, &closes, &transactions) {
            let offsets = match part {
                Part::Closes => &close_at,
                Part::Transactions => &transaction_at,
            };
            return Err(ClusterError::malformed(
                what,
                offsets.get(index).copied().unwrap_or(0),
            ));
        }
        Self::new(position, closes, transactions)
    }
}

/// `count`, at most the settings' limit, as its 4-byte field.
fn count_u32(count: usize) -> u32 {
    u32::try_from(count).unwrap_or(u32::MAX)
}

/// Appends `record` after its length as 4 bytes.
fn put_framed(out: &mut Vec<u8>, record: &[u8]) {
    out.extend_from_slice(&count_u32(record.len()).to_be_bytes());
    out.extend_from_slice(record);
}

/// The record framed at the reader, with the offset its bytes begin at.
fn read_framed(reader: &mut Reader<'_>) -> Result<(usize, SequencerRecord), ClusterError> {
    let len = reader.u32("a record's length")? as usize; // a `u32`, lossless
    let at = reader.offset();
    let body = reader.bytes(len, "a record")?;
    let record = SequencerRecord::decode(body).map_err(|refusal| {
        if let ClusterErrorEnum::Malformed { what, offset } = &refusal.0 {
            ClusterError::malformed(what, at + *offset)
        } else {
            refusal
        }
    })?;
    Ok((at, record))
}
