/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::types::command::Command;
use crate::types::key_values::KeyValues;
use crate::types::operation_result::OperationResult;
use crate::types::record_entry::RecordEntry;
use crate::types::record_operation::RecordOperation;
use crate::types::write_set_record::WriteSetRecord;
use crate::utils::constants::ABSENT;
use crate::utils::le::bytes_at;
use crate::utils::operation::read_operation;
use crate::utils::results::{RESULT_SIZE, check_results, result_at};
use crate::utils::sections::Sections;

/// The fixed part's size: namespace, read version, session, request and principal.
const FIXED_SIZE: usize = 40;

/// A received write-set record read in place; see [`WriteSetRecord`] for its layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteSetRecordView<'a> {
    bytes: &'a [u8],
    namespace: u64,
    read_version: u64,
    session: u64,
    request: u64,
    principal: u64,
    tables: [KeyValues<'a>; 8],
    commit_record: &'a [u8],
    results: &'a [u8],
    operation: Option<RecordOperation>,
}

impl<'a> WriteSetRecordView<'a> {
    /// The record `bytes`, checked once: the fixed part and section table, then each key-value section's entries,
    /// then the results, then the operation. Errors name the Round command and the record's own sections, offsets within the record; a
    /// conflict point with a value is [`ProtocolError::FieldValue`].
    pub fn new(bytes: &'a [u8]) -> Result<Self, ProtocolError> {
        let command = Command::Round.number();
        let sections = Sections::<11>::read(command, bytes, FIXED_SIZE)?;
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
        let [deltas, .., points, statements, ranges] = tables;
        if deltas.iter().any(|e| e.value().is_none()) {
            return Err(ProtocolError::FieldValue(
                command,
                "list delta",
                ABSENT.into(),
            ));
        }
        if let Some(entry) = points.iter().find(|e| e.value() != Some(&[][..])) {
            let value = entry.value().map_or(u64::from(ABSENT), |v| v.len() as u64);
            return Err(ProtocolError::FieldValue(command, "conflict point", value));
        }
        for (name, entries) in [
            ("conflict statement", statements),
            ("conflict range", ranges),
        ] {
            if entries.iter().any(|e| e.value().is_none()) {
                return Err(ProtocolError::FieldValue(command, name, ABSENT.into()));
            }
        }
        check_results(command, 9, sections.offset(9), sections.get(9))?;
        let operation = read_operation(command, 10, sections.offset(10), sections.get(10))?;
        Ok(Self {
            bytes,
            namespace: field(0),
            read_version: field(8),
            session: field(16),
            request: field(24),
            principal: field(32),
            tables,
            commit_record: sections.get(5),
            results: sections.get(9),
            operation,
        })
    }

    /// The record's bytes.
    pub fn bytes(&self) -> &'a [u8] {
        self.bytes
    }

    /// The namespace.
    pub fn namespace(&self) -> u64 {
        self.namespace
    }

    /// The snapshot version the transaction read at.
    pub fn read_version(&self) -> u64 {
        self.read_version
    }

    /// The client's session.
    pub fn session(&self) -> u64 {
        self.session
    }

    /// The request's number within its session.
    pub fn request(&self) -> u64 {
        self.request
    }

    /// The principal the transaction acted for; 0 without authentication.
    pub fn principal(&self) -> u64 {
        self.principal
    }

    /// The list deltas, in the posting format.
    pub fn list_deltas(&self) -> KeyValues<'a> {
        self.tables[0]
    }

    /// The dictionary entries.
    pub fn dictionary(&self) -> KeyValues<'a> {
        self.tables[1]
    }

    /// The statement records.
    pub fn statements(&self) -> KeyValues<'a> {
        self.tables[2]
    }

    /// The catalog entries.
    pub fn catalog(&self) -> KeyValues<'a> {
        self.tables[3]
    }

    /// The entries of the keyspace without versions.
    pub fn versionless(&self) -> KeyValues<'a> {
        self.tables[4]
    }

    /// The commit record the client attached.
    pub fn commit_record(&self) -> &'a [u8] {
        self.commit_record
    }

    /// The conflict points: entries whose value is empty.
    pub fn conflict_points(&self) -> KeyValues<'a> {
        self.tables[5]
    }

    /// The conflict statements: a list key and the object it changed.
    pub fn conflict_statements(&self) -> KeyValues<'a> {
        self.tables[6]
    }

    /// The conflict ranges: the first key and the key past the last.
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

    /// The record's operation, `None` for a transaction.
    pub fn operation(&self) -> Option<RecordOperation> {
        self.operation
    }

    /// The owned record of the view's values.
    pub fn to_record(&self) -> WriteSetRecord {
        let mut record = WriteSetRecord::new(
            self.namespace,
            self.read_version,
            self.session,
            self.request,
        );
        record.set_principal(self.principal);
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
        if let Some(operation) = self.operation {
            record.set_operation(operation);
        }
        record
    }
}
