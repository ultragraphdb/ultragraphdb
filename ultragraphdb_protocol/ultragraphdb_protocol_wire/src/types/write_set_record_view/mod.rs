/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::types::command::Command;
use crate::types::key_values::KeyValues;
use crate::types::operation_result::OperationResult;
use crate::types::record_entry::RecordEntry;
use crate::types::write_set_record::WriteSetRecord;
use crate::utils::constants::ABSENT;
use crate::utils::le::bytes_at;
use crate::utils::results::{RESULT_SIZE, check_results, result_at};
use crate::utils::sections::Sections;

/// The fixed part's size: namespace, read version, session and request.
const FIXED_SIZE: usize = 32;

/// A received write-set record read in place; see [`WriteSetRecord`] for its layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteSetRecordView<'a> {
    bytes: &'a [u8],
    namespace: u64,
    read_version: u64,
    session: u64,
    request: u64,
    tables: [KeyValues<'a>; 8],
    commit_record: &'a [u8],
    results: &'a [u8],
}

impl<'a> WriteSetRecordView<'a> {
    /// The record `bytes`, checked once: the fixed part and section table, then each key-value section's entries,
    /// then the results. Errors name the Round command and the record's own sections, offsets within the record; a
    /// conflict point with a value is [`ProtocolError::FieldValue`].
    pub fn new(bytes: &'a [u8]) -> Result<Self, ProtocolError> {
        let command = Command::Round.number();
        let sections = Sections::<10>::read(command, bytes, FIXED_SIZE)?;
        let fixed = sections.fixed();
        let field = |at: usize| bytes_at(fixed, at).map_or(0, u64::from_le_bytes);
        let table = |i: usize| KeyValues::read(command, i, sections.offset(i), sections.get(i));
        let tables = [
            table(0)?,
            table(1)?,
            table(2)?,
            table(3)?,
            table(4)?,
            table(6)?,
            table(7)?,
            table(8)?,
        ];
        let [.., points, statements, ranges] = tables;
        if let Some(entry) = points.iter().find(|e| e.value() != Some(&[][..])) {
            let value = entry.value().map_or(u64::from(ABSENT), |v| v.len() as u64);
            return Err(ProtocolError::FieldValue(command, "conflict point", value));
        }
        for (field, entries) in [
            ("conflict statement", statements),
            ("conflict range", ranges),
        ] {
            if entries.iter().any(|e| e.value().is_none()) {
                return Err(ProtocolError::FieldValue(command, field, ABSENT.into()));
            }
        }
        check_results(command, 9, sections.offset(9), sections.get(9))?;
        Ok(Self {
            bytes,
            namespace: field(0),
            read_version: field(8),
            session: field(16),
            request: field(24),
            tables,
            commit_record: sections.get(5),
            results: sections.get(9),
        })
    }

    /// The record's bytes.
    pub fn bytes(&self) -> &'a [u8] {
        self.bytes
    }

    pub fn namespace(&self) -> u64 {
        self.namespace
    }

    pub fn read_version(&self) -> u64 {
        self.read_version
    }

    pub fn session(&self) -> u64 {
        self.session
    }

    pub fn request(&self) -> u64 {
        self.request
    }

    pub fn list_deltas(&self) -> KeyValues<'a> {
        self.tables[0]
    }

    pub fn dictionary(&self) -> KeyValues<'a> {
        self.tables[1]
    }

    pub fn statements(&self) -> KeyValues<'a> {
        self.tables[2]
    }

    pub fn catalog(&self) -> KeyValues<'a> {
        self.tables[3]
    }

    pub fn versionless(&self) -> KeyValues<'a> {
        self.tables[4]
    }

    pub fn commit_record(&self) -> &'a [u8] {
        self.commit_record
    }

    /// The conflict points: entries whose value is empty.
    pub fn conflict_points(&self) -> KeyValues<'a> {
        self.tables[5]
    }

    pub fn conflict_statements(&self) -> KeyValues<'a> {
        self.tables[6]
    }

    pub fn conflict_ranges(&self) -> KeyValues<'a> {
        self.tables[7]
    }

    /// The number of commit-time operations' results.
    pub fn result_count(&self) -> usize {
        self.results.len() / RESULT_SIZE
    }

    /// Result `index`, in the order the transaction asked its operations; `None` past the last.
    pub fn result(&self, index: usize) -> Option<OperationResult> {
        result_at(self.results, index)
    }

    /// The owned record of the view's values.
    pub fn to_record(&self) -> WriteSetRecord {
        let mut record = WriteSetRecord::new(
            self.namespace,
            self.read_version,
            self.session,
            self.request,
        );
        let owned = |e: RecordEntry<'_>| (e.key().to_vec(), e.value().map(<[u8]>::to_vec));
        for (key, value) in self.list_deltas().iter().map(owned) {
            record.push_list_delta(key, value.unwrap_or_default());
        }
        for (key, value) in self.dictionary().iter().map(owned) {
            record.push_dictionary(key, value);
        }
        for (key, value) in self.statements().iter().map(owned) {
            record.push_statement(key, value);
        }
        for (key, value) in self.catalog().iter().map(owned) {
            record.push_catalog(key, value);
        }
        for (key, value) in self.versionless().iter().map(owned) {
            record.push_versionless(key, value);
        }
        record.set_commit_record(self.commit_record.to_vec());
        for entry in self.conflict_points().iter() {
            record.push_conflict_point(entry.key().to_vec());
        }
        for (key, value) in self.conflict_statements().iter().map(owned) {
            record.push_conflict_statement(key, value.unwrap_or_default());
        }
        for (key, value) in self.conflict_ranges().iter().map(owned) {
            record.push_conflict_range(key, value.unwrap_or_default());
        }
        for result in (0..self.result_count()).filter_map(|i| self.result(i)) {
            record.push_result(result);
        }
        record
    }
}
