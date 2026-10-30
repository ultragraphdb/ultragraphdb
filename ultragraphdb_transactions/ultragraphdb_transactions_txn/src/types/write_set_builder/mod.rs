/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_codec::PostingList;
use ultragraphdb_core_keys::Keyspace;
use ultragraphdb_store_apply::{BuildEnd, Executed};
use ultragraphdb_store_quads::{HASH_TO_STRING, IRI_TO_UID, UID_TO_IRI, posting_object};

use crate::errors::txn_error::TxnError;
use crate::types::txn_limits::TxnLimits;

/// The fixed part's size: namespace, read version, session, request and principal.
const FIXED: usize = 40;
/// The number of sections.
const SECTIONS: usize = 11;
/// Where section 0 starts: after the fixed part and the table of offsets and lengths.
const TABLE_END: usize = FIXED + 8 * SECTIONS;
/// A key-value entry's lengths, before its key.
const ENTRY_HEADER: usize = 8;
/// The value length of a removal.
const ABSENT: u32 = u32::MAX;
/// Where the tag of a key without versions sits, after its namespace.
const TAG_AT: usize = 8;

/// Writes an executed transaction's write-set record straight into a buffer, in `ultragraphdb_protocol_wire`'s
/// [`WriteSetRecord`](ultragraphdb_protocol_wire::WriteSetRecord) layout, which a
/// [`WriteSetRecordView`](ultragraphdb_protocol_wire::WriteSetRecordView) reads in place: the fixed part (namespace,
/// read version, session, request, and the principal, 0 here), the section table, then the sections, in one pass with no copy of the writes in
/// between.
///
/// | Section | What the transaction's writes put there |
/// |---|---|
/// | 0 | each list's delta, in the posting format, by key; each statistics delta, by its key |
/// | 1 | the dictionary entries (the keys without versions tagged IRI to UID, UID to IRI, hash to string) |
/// | 2 | statement records, a removal for a record deleted |
/// | 3 | catalog entries, a removal for an entry deleted |
/// | 4 | the other writes of the keyspace without versions |
/// | 5 | the commit record the client attached, unchanged |
/// | 6 | conflict points: the key of every write but the lists named object by object and a predicate's implicit default catalog entry, an insert-if-absent on its key and not a catalog change (decided 2026-10-27) |
/// | 7 | conflict statements: a list key and an object it changed, for each list changed by no more than the bound's objects and not cleared whole |
/// | 8 | conflict ranges: a drop's key ranges, each its first key and the key past its last; none for any other execution (7.7) |
/// | 9 | the commit-time operations' results, 16 bytes each, as the commit step writes them; empty for a worker's record (decided 2026-10-28) |
/// | 10 | the record's operation: a drop's predicate (code 1), a build's swap (2) or undo (3) with its record number; empty for any other execution (decided 2026-10-28) |
///
/// Only a forward list is named object by object, since only a forward list's statements are tested by a read; a list
/// of any other keyspace, a forward list changed by more objects than the bound, or one holding a posting that names
/// no object is a conflict point instead, which can only add conflicts. A transaction's execution writes no entry at
/// a version of its own, which only an index build's swap writes, so the record has no section for one.
#[derive(Debug, Clone, Copy)]
pub struct WriteSetBuilder<'e> {
    executed: &'e Executed,
    session: u64,
    request: u64,
    commit_record: &'e [u8],
    objects_per_list: usize,
    results: &'e [(u64, bool)],
}

/// The size of one commit-time operation's result: `uid` u64, `created` u8, 7 zero bytes.
const RESULT_SIZE: usize = 16;
/// The size of a record's operation: `operation` u16, 6 zero bytes, `argument` u64.
const OPERATION_SIZE: usize = 16;

/// The operation `executed` is, as section 10 names it: its code and argument.
fn operation_of(executed: &Executed) -> Option<(u16, u64)> {
    match (executed.dropped(), executed.build_end()) {
        (Some(uid), _) => Some((1, uid)),
        (None, Some(BuildEnd::Swap(record))) => Some((2, record)),
        (None, Some(BuildEnd::Undo(record))) => Some((3, record)),
        (None, None) => None,
    }
}

/// An entry's value as the builder writes it: bytes, a delta in the posting format, or a removal.
#[derive(Clone, Copy)]
enum Value<'v> {
    Bytes(&'v [u8]),
    Delta(&'v PostingList),
    Removal,
}

impl Value<'_> {
    fn len(self) -> usize {
        match self {
            Value::Bytes(b) => b.len(),
            Value::Delta(d) => d.encoded_len(),
            Value::Removal => 0,
        }
    }
}

/// `n` rounded up to a multiple of 8.
fn align8(n: usize) -> usize {
    n.div_ceil(8) * 8
}

/// The size of a key-value entry of `key` and `value`, its padding included.
fn entry_len(key: &[u8], value: Value<'_>) -> usize {
    align8(ENTRY_HEADER + key.len() + value.len())
}

/// The section of the keyspace without versions `key` belongs to: 1 for the dictionary, 4 otherwise.
fn versionless_section(key: &[u8]) -> usize {
    match key.get(TAG_AT) {
        Some(&IRI_TO_UID | &UID_TO_IRI | &HASH_TO_STRING) => 1,
        _ => 4,
    }
}

/// The section of an entry written at the commit version: 3 for the catalog, 2 for statement records, 0 for the
/// others, the statistics deltas.
fn entry_section(key: &[u8]) -> usize {
    match key.first().and_then(|b| Keyspace::from_byte(*b)) {
        Some((Keyspace::Catalog, _)) => 3,
        Some((Keyspace::Statements, _)) => 2,
        _ => 0,
    }
}

impl<'e> WriteSetBuilder<'e> {
    /// The record of `executed`, committed as request `request` of session `session` with `commit_record`, its
    /// lists named object by object up to the default bound of [`TxnLimits`].
    pub fn new(
        executed: &'e Executed,
        session: u64,
        request: u64,
        commit_record: &'e [u8],
    ) -> Self {
        Self {
            executed,
            session,
            request,
            commit_record,
            objects_per_list: TxnLimits::default().objects_per_list(),
            results: &[],
        }
    }

    /// The builder writing `results`, the commit-time operations' UIDs and whether each created its node, in the
    /// order the transaction asked them, into section 9: what the commit step's record of a transaction with
    /// operations holds (decided 2026-10-28).
    pub fn with_results(self, results: &'e [(u64, bool)]) -> Self {
        Self { results, ..self }
    }

    /// The builder naming at most `objects` objects of one list one by one.
    pub fn with_objects_per_list(self, objects: usize) -> Self {
        Self {
            objects_per_list: objects,
            ..self
        }
    }

    /// Whether the delta of `key` is named object by object: a forward list, within the bound, every posting naming
    /// an object.
    fn by_objects(&self, key: &[u8], delta: &PostingList) -> bool {
        matches!(
            key.first().and_then(|b| Keyspace::from_byte(*b)),
            Some((Keyspace::Forward, false))
        ) && delta.postings().len() <= self.objects_per_list
            && delta
                .postings()
                .iter()
                .all(|p| posting_object(key, p).is_ok())
    }

    /// Calls `f` with every key-value entry of section `section`, in order. Section 5 is no key-value section and
    /// gives none.
    fn visit(&self, section: usize, f: &mut impl FnMut(&[u8], Value<'_>)) {
        let writes = self.executed.writes();
        let empty = Value::Bytes(&[]);
        let entry_value = |e: &'e ultragraphdb_traits_store::Entry| {
            e.value().map_or(Value::Removal, Value::Bytes)
        };
        match section {
            0 => {
                for (key, delta) in writes.deltas() {
                    f(key, Value::Delta(delta));
                }
                for e in writes
                    .entries()
                    .iter()
                    .filter(|e| entry_section(e.key()) == 0)
                {
                    f(e.key(), entry_value(e));
                }
            }
            1 | 4 => {
                for e in writes
                    .versionless()
                    .iter()
                    .filter(|e| versionless_section(e.key()) == section)
                {
                    f(e.key(), e.value().map_or(Value::Removal, Value::Bytes));
                }
            }
            2 | 3 => {
                for e in writes
                    .entries()
                    .iter()
                    .filter(|e| entry_section(e.key()) == section)
                {
                    f(e.key(), entry_value(e));
                }
            }
            6 => {
                for (key, delta) in writes.deltas() {
                    if !self.by_objects(key, delta) {
                        f(key, empty);
                    }
                }
                // A predicate's implicit default entry is an insert-if-absent on its catalog key, no catalog change.
                let defaults = self.executed.defaults();
                for e in writes.entries() {
                    if defaults
                        .binary_search_by(|k| k.as_slice().cmp(e.key()))
                        .is_err()
                    {
                        f(e.key(), empty);
                    }
                }
                for e in writes.versionless() {
                    f(e.key(), empty);
                }
            }
            7 => {
                for (key, delta) in writes.deltas() {
                    if self.by_objects(key, delta) {
                        for p in delta.postings() {
                            if let Ok(object) = posting_object(key, p) {
                                f(key, Value::Bytes(object.as_bytes()));
                            }
                        }
                    }
                }
            }
            8 => {
                for (start, end) in self.executed.drop_ranges() {
                    f(start, Value::Bytes(end));
                }
            }
            _ => {}
        }
    }

    /// The size of section `section`.
    fn section_len(&self, section: usize) -> usize {
        if section == 5 {
            return self.commit_record.len();
        }
        if section == 9 {
            return RESULT_SIZE * self.results.len();
        }
        if section == 10 {
            return operation_of(self.executed).map_or(0, |_| OPERATION_SIZE);
        }
        let mut len = 0;
        self.visit(section, &mut |key, value| len += entry_len(key, value));
        len
    }

    /// The record's size in bytes, computed without writing it.
    pub fn len(&self) -> usize {
        (0..SECTIONS).fold(TABLE_END, |end, section| {
            align8(end) + self.section_len(section)
        })
    }

    /// Whether the record is empty, which it never is: its fixed part and table are always there.
    pub fn is_empty(&self) -> bool {
        false
    }

    /// Clears `out` and writes the record into it, reserving its exact size once: the record's size.
    ///
    /// # Errors
    ///
    /// [`TxnError::WriteSetTooLarge`] when the record is larger than `limit` bytes, or than `u32::MAX`; `out` is then
    /// left empty, its capacity kept, and nothing is written.
    pub fn write_into(&self, out: &mut Vec<u8>, limit: usize) -> Result<usize, TxnError> {
        out.clear();
        let len = self.len();
        let limit = limit.min(u32::MAX as usize);
        if len > limit {
            return Err(TxnError::WriteSetTooLarge(len, limit));
        }
        out.reserve_exact(len);
        let fixed = [
            self.executed.namespace(),
            self.executed.read_ts(),
            self.session,
            self.request,
        ];
        fixed
            .iter()
            .for_each(|v| out.extend_from_slice(&v.to_le_bytes()));
        out.resize(TABLE_END, 0);
        for section in 0..SECTIONS {
            out.resize(align8(out.len()), 0);
            let start = out.len();
            if section == 5 {
                out.extend_from_slice(self.commit_record);
            } else if section == 9 {
                for (uid, created) in self.results {
                    out.extend_from_slice(&uid.to_le_bytes());
                    out.push(u8::from(*created));
                    out.extend_from_slice(&[0; 7]);
                }
            } else if section == 10 {
                if let Some((code, argument)) = operation_of(self.executed) {
                    out.extend_from_slice(&code.to_le_bytes());
                    out.extend_from_slice(&[0; 6]);
                    out.extend_from_slice(&argument.to_le_bytes());
                }
            } else {
                self.visit(section, &mut |key, value| write_entry(out, key, value));
            }
            let at = FIXED + 8 * section;
            // The record is at most u32::MAX bytes, checked above.
            let table = [start as u32, (out.len() - start) as u32];
            if let Some(slot) = out.get_mut(at..at + 8) {
                slot[..4].copy_from_slice(&table[0].to_le_bytes());
                slot[4..].copy_from_slice(&table[1].to_le_bytes());
            }
        }
        Ok(out.len())
    }
}

/// Appends the key-value entry of `key` and `value` to `out`, padded to 8 bytes.
fn write_entry(out: &mut Vec<u8>, key: &[u8], value: Value<'_>) {
    let value_len = match value {
        Value::Removal => ABSENT,
        v => v.len() as u32,
    };
    out.extend_from_slice(&(key.len() as u32).to_le_bytes());
    out.extend_from_slice(&value_len.to_le_bytes());
    out.extend_from_slice(key);
    match value {
        Value::Bytes(b) => out.extend_from_slice(b),
        Value::Delta(d) => d.encode_into(out),
        Value::Removal => {}
    }
    out.resize(align8(out.len()), 0);
}
