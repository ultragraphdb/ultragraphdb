/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod round_body_body_builder;

use std::collections::BTreeMap;
use std::num::NonZeroU32;
use ultragraphdb_core_body::as_u32;

use crate::types::decision::Decision;
use crate::types::error_code::ErrorCode;
use crate::types::round_header::RoundHeader;

/// The size of a decision.
pub(crate) const DECISION_SIZE: usize = 16;

/// The size of a record table entry.
pub(crate) const RECORD_ENTRY_SIZE: usize = 8;

/// The Round frame's body (`PROTOCOL.md` §12.1), the log record of one partition's round: each entry's decision in
/// round order, the write-set record of each committed one, and the dictionary entries the round creates. An entry is
/// a transaction, a Drop, a registration the partition accepted as an entity's home, or an index build's start or
/// completion. Committed entry k of the round (counting committed ones from 0) has commit version `first_version +
/// 2k`; the round's entries carry one epoch, `first_version >> 22`. The command header is [`RoundHeader`], its
/// decision count the number of decisions.
///
/// | Section | Contents |
/// |---|---|
/// | 0 | decisions, 16 bytes each: `outcome` u16 (0 committed, else the [`crate::ErrorCode`] of the abort), 2 zero bytes, `record` u32 (the committed entry's record index from 0; [`crate::ABSENT`] for an abort), `detail` u64 (0 for a commit) |
/// | 1 | the record table: `offset` u32 and `len` u32 of each record within section 2, record 0 at 0 and each later one at the end of the one before rounded up to 8, the padding zero |
/// | 2 | the records, each a [`crate::WriteSetRecord`] |
/// | 3 | the dictionary entries the round creates, in the key-value layout of a record's sections: each a key of the dictionary keyspace and its value, each once, in ascending key order |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoundBody {
    position: u64,
    parent: u64,
    incarnation: u64,
    first_version: u64,
    derivation: NonZeroU32,
    decisions: Vec<Decision>,
    records: Vec<Vec<u8>>,
    dictionary: BTreeMap<Vec<u8>, Vec<u8>>,
}

impl RoundBody {
    /// An empty round at `position` of the log of `incarnation`, after the round whose checksum is `parent`, its first
    /// commit version `first_version`, its quads turned into keys by `derivation`.
    #[must_use]
    pub fn new(
        position: u64,
        parent: u64,
        incarnation: u64,
        first_version: u64,
        derivation: NonZeroU32,
    ) -> Self {
        Self {
            position,
            parent,
            incarnation,
            first_version,
            derivation,
            decisions: Vec::new(),
            records: Vec::new(),
            dictionary: BTreeMap::new(),
        }
    }

    /// Appends a committed entry and its encoded write-set record.
    pub fn commit(&mut self, record: Vec<u8>) {
        self.decisions.push(Decision::Committed);
        self.records.push(record);
    }

    /// Appends an aborted entry with its reason.
    pub fn abort(&mut self, code: ErrorCode, detail: u64) {
        self.decisions.push(Decision::Aborted { code, detail });
    }

    /// Adds the dictionary entry of `key` holding `value`, which the round creates; the entries are kept in ascending
    /// key order, each key once, a later value for a key replacing the earlier.
    pub fn insert_dictionary(&mut self, key: Vec<u8>, value: Vec<u8>) {
        let _replaced: Option<Vec<u8>> = self.dictionary.insert(key, value);
    }

    /// The command header: the round's fields and its decision count.
    pub fn header(&self) -> RoundHeader {
        RoundHeader::new(
            self.position,
            self.parent,
            self.incarnation,
            self.first_version,
            as_u32(self.decisions.len()),
            self.derivation,
        )
    }

    /// Each entry's decision, in order.
    pub fn decisions(&self) -> &[Decision] {
        &self.decisions
    }

    /// The encoded write-set records of the committed entries, in order.
    pub fn records(&self) -> &[Vec<u8>] {
        &self.records
    }

    /// The dictionary entries the round creates, in ascending key order.
    #[must_use]
    pub fn dictionary(&self) -> &BTreeMap<Vec<u8>, Vec<u8>> {
        &self.dictionary
    }
}
