/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_body::Sections;
use ultragraphdb_error_protocol::ProtocolError;

use crate::types::command::Command;
use crate::types::key_values::KeyValues;
use crate::types::operation_result::OperationResult;
use crate::types::record_kind::RecordKind;
use crate::types::record_operation::RecordOperation;
use crate::types::record_registration::RecordRegistration;
use crate::types::term_table::{TERM_RECORD_SIZE, kind};
use crate::types::term_table_view::TermTableView;
use crate::types::write_set_record::WriteSetRecord;
use crate::utils::body::read_sections;
use crate::utils::constants::{
    ABSENT, OPERATION_RESULT_SIZE, RECORD_FIXED_SIZE, RECORD_OPERATION_SIZE,
    RECORD_REGISTRATION_SIZE, RECORD_SECTIONS,
};
use crate::utils::le::bytes_at;
use crate::utils::operation::{
    check_operations, check_registrations, operation_at, registration_at,
};
use crate::utils::results::{check_results, result_at};

/// The fixed part's fields, read and checked.
#[derive(Debug, Clone, Copy)]
struct Fixed {
    kind: RecordKind,
    tenant: u64,
    catalog: u64,
    read_version: u64,
    session: [u8; 16],
    request: u64,
    principal: u64,
}

// A copy moves at most one cache line (Ultra rule 9).
const _: () = assert!(size_of::<Fixed>() <= 64);

/// The fixed part `fixed`, of 64 bytes, checked in order: the kind, its 7 zero bytes, the fields the kind leaves 0.
fn read_fixed(command: u16, fixed: &[u8]) -> Result<Fixed, ProtocolError> {
    let code = fixed.first().copied().unwrap_or(0);
    let kind = RecordKind::from_code(code)
        .ok_or_else(|| ProtocolError::field_value(command, "record kind", code.into()))?;
    if let Some(j) = fixed
        .get(1..8)
        .and_then(|zeros| zeros.iter().position(|b| *b != 0))
    {
        return Err(ProtocolError::padding_not_zero(command, 1 + j));
    }
    let u64_at = |at: usize| bytes_at(fixed, at).map_or(0, u64::from_le_bytes);
    let session: [u8; 16] = bytes_at(fixed, 32).unwrap_or([0; 16]);
    // The fields each kind leaves 0, by their offsets: a registration all but its tenant, a build all but its
    // tenant and catalog.
    let zero: &[usize] = match kind {
        RecordKind::Transaction => &[],
        RecordKind::Registration => &[16, 24, 32, 48, 56],
        RecordKind::Build => &[24, 32, 48, 56],
    };
    let set = |at: usize| match at {
        32 => session != [0; 16],
        _ => u64_at(at) != 0,
    };
    if let Some(at) = zero.iter().find(|at| set(**at)) {
        return Err(ProtocolError::field_value(command, "record", *at as u64)); // lossless: a `usize` is at most 64 bits
    }
    Ok(Fixed {
        kind,
        tenant: u64_at(8),
        catalog: u64_at(16),
        read_version: u64_at(24),
        session,
        request: u64_at(48),
        principal: u64_at(56),
    })
}

/// The key-value sections of `sections`, each entry framed, then each section's values as it takes them.
fn read_tables<'a>(
    command: u16,
    sections: &Sections<'a, RECORD_SECTIONS>,
) -> Result<[KeyValues<'a>; 6], ProtocolError> {
    let table = |i: usize| KeyValues::read(command, i, sections.offset(i), sections.get(i));
    let tables = [
        table(0)?,
        table(1)?,
        table(2)?,
        table(6)?,
        table(7)?,
        table(8)?,
    ];
    let [inserted, removed, _, points, statements, ranges] = tables;
    // Lossless: a `usize` is at most 64 bits.
    let length = |value: Option<&[u8]>| value.map_or(u64::from(ABSENT), |v| v.len() as u64);
    for entries in [inserted, removed] {
        if let Some(e) = entries
            .iter()
            .find(|e| !matches!(e.value().map(<[u8]>::len), Some(0 | 16)))
        {
            return Err(ProtocolError::field_value(
                command,
                "statement id",
                length(e.value()),
            ));
        }
    }
    if let Some(e) = points.iter().find(|e| e.value() != Some(&[][..])) {
        return Err(ProtocolError::field_value(
            command,
            "conflict point",
            length(e.value()),
        ));
    }
    for (name, entries) in [
        ("conflict statement", statements),
        ("conflict range", ranges),
    ] {
        if entries.iter().any(|e| e.value().is_none()) {
            return Err(ProtocolError::field_value(command, name, ABSENT.into()));
        }
    }
    Ok(tables)
}

/// The results' term table, sections 10 and 11, checked as a request's, then each entry an IRI.
fn read_result_terms<'a>(
    command: u16,
    sections: &Sections<'a, RECORD_SECTIONS>,
) -> Result<TermTableView<'a>, ProtocolError> {
    let records = sections.get(10);
    let terms = TermTableView::read(
        command,
        (10, 11),
        sections.offset(10),
        records,
        sections.get(11),
    )?;
    match records
        .as_chunks::<TERM_RECORD_SIZE>()
        .0
        .iter()
        .find_map(|record| record.first().copied().filter(|k| *k != kind::IRI))
    {
        Some(found) => Err(ProtocolError::field_value(
            command,
            "node term",
            found.into(),
        )),
        None => Ok(terms),
    }
}

/// A received write-set record read in place; see [`WriteSetRecord`] for its layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteSetRecordView<'a> {
    bytes: &'a [u8],
    kind: RecordKind,
    tenant: u64,
    catalog: u64,
    read_version: u64,
    session: [u8; 16],
    request: u64,
    principal: u64,
    tables: [KeyValues<'a>; 6],
    text_len: usize,
    operations: &'a [u8],
    registrations: &'a [u8],
    commit_record: &'a [u8],
    results: &'a [u8],
    result_terms: TermTableView<'a>,
}

impl<'a> WriteSetRecordView<'a> {
    /// The record `bytes`, checked once in the order of `PROTOCOL.md` §12.2; errors name the Round command and
    /// offsets within the record.
    ///
    /// # Errors
    ///
    /// In order: the fixed part and section table (§4.1's refusals); [`ProtocolError::field_value`] `"record kind"` for
    /// a kind not 1 to 3; [`ProtocolError::padding_not_zero`] for a nonzero byte among bytes 1 to 7;
    /// [`ProtocolError::field_value`] `"record"`, naming the field's offset, for a fixed field the kind leaves 0 that is
    /// not; sections 0, 1, 2, 6, 7 and 8 in turn, each entry inside its section
    /// ([`ProtocolError::entry_out_of_bounds`]) with zero padding ([`ProtocolError::padding_not_zero`]);
    /// [`ProtocolError::field_value`] `"statement id"` for a value of section 0 or 1 absent or neither empty nor 16
    /// bytes, `"conflict point"` for a conflict point's value absent or not empty, `"conflict statement"` and
    /// `"conflict range"` for an absent value; [`ProtocolError::section_length`] for section 3 not a whole number of
    /// 24-byte operations, [`ProtocolError::field_value`] `"operation"` for a code not 1 to 4, 6 or 7,
    /// [`ProtocolError::padding_not_zero`] for its bytes 2 to 7, `"argument"`, naming the operation's index, for an
    /// argument other than its code takes; [`ProtocolError::section_length`] for section 4 not a whole number of 32-byte
    /// registrations, [`ProtocolError::field_value`] `"registration"`, naming its index, for an invalid entity, a group
    /// of 2¹⁸ or more, nonzero bytes 20 to 23 or a generation of 0; the results' term table as a request's, then each
    /// entry an IRI ([`ProtocolError::field_value`] `"node term"`), section 9 a whole number of 16-byte results, each
    /// result's bytes 13 to 15 zero ([`ProtocolError::padding_not_zero`]), `created` at most 1
    /// ([`ProtocolError::field_value`] `"created"`), `node` [`crate::ABSENT`] or below the table's length
    /// ([`ProtocolError::index_out_of_range`]), and `value` 0 where a node is named and `created` 0 where none is
    /// ([`ProtocolError::field_value`] `"result"`); then the sections the kind holds
    /// ([`ProtocolError::count_mismatch`] `"record sections"`, the offending section's count and the count the kind
    /// takes; [`ProtocolError::field_value`] `"operation"` for an operation the kind does not take, and `"record"` for a
    /// Drop's nonzero read version).
    pub fn new(bytes: &'a [u8]) -> Result<Self, ProtocolError> {
        let command = Command::Round.number();
        let sections = read_sections::<RECORD_SECTIONS>(command, bytes, RECORD_FIXED_SIZE)?;
        let fixed = read_fixed(command, sections.fixed())?;
        let tables = read_tables(command, &sections)?;
        check_operations(command, 3, sections.offset(3), sections.get(3))?;
        check_registrations(command, 4, sections.get(4))?;
        let result_terms = read_result_terms(command, &sections)?;
        check_results(
            command,
            9,
            sections.offset(9),
            sections.get(9),
            Some(result_terms.len()),
        )?;
        let view = Self {
            bytes,
            kind: fixed.kind,
            tenant: fixed.tenant,
            catalog: fixed.catalog,
            read_version: fixed.read_version,
            session: fixed.session,
            request: fixed.request,
            principal: fixed.principal,
            tables,
            text_len: sections.get(11).len(),
            operations: sections.get(3),
            registrations: sections.get(4),
            commit_record: sections.get(5),
            results: sections.get(9),
            result_terms,
        };
        view.check_kind()?;
        Ok(view)
    }

    /// The record's bytes.
    pub fn bytes(&self) -> &'a [u8] {
        self.bytes
    }

    /// The record's kind.
    #[must_use]
    pub fn kind(&self) -> RecordKind {
        self.kind
    }

    /// The tenant.
    pub fn tenant(&self) -> u64 {
        self.tenant
    }

    /// The tenant's catalog version the entry was validated against; 0 for a registration.
    #[must_use]
    pub fn catalog(&self) -> u64 {
        self.catalog
    }

    /// The version the transaction read at; 0 for a Drop, a registration and a build.
    pub fn read_version(&self) -> u64 {
        self.read_version
    }

    /// The session committing through the transaction; zero for none.
    #[must_use]
    pub fn session(&self) -> [u8; 16] {
        self.session
    }

    /// The session's request number; 0 for none.
    pub fn request(&self) -> u64 {
        self.request
    }

    /// The principal the transaction acted for.
    pub fn principal(&self) -> u64 {
        self.principal
    }

    /// The statements inserted: each key a quad, its value the statement ID or empty.
    #[must_use]
    pub fn inserted(&self) -> KeyValues<'a> {
        self.tables[0]
    }

    /// The statements removed: each key a quad, its value the statement ID or empty.
    #[must_use]
    pub fn removed(&self) -> KeyValues<'a> {
        self.tables[1]
    }

    /// The statement records: each key a graph and a statement ID, its value the record or a removal.
    #[must_use]
    pub fn statement_records(&self) -> KeyValues<'a> {
        self.tables[2]
    }

    /// The number of operations.
    #[must_use]
    pub fn operation_count(&self) -> usize {
        self.operations.len() / RECORD_OPERATION_SIZE
    }

    /// Operation `index`; `None` past the last.
    #[must_use]
    pub fn operation(&self, index: usize) -> Option<RecordOperation> {
        operation_at(self.operations, index)
    }

    /// The number of registrations.
    #[must_use]
    pub fn registration_count(&self) -> usize {
        self.registrations.len() / RECORD_REGISTRATION_SIZE
    }

    /// Registration `index`; `None` past the last.
    #[must_use]
    pub fn registration(&self, index: usize) -> Option<RecordRegistration> {
        registration_at(self.registrations, index)
    }

    /// The commit record the client attached.
    pub fn commit_record(&self) -> &'a [u8] {
        self.commit_record
    }

    /// The conflict points: entries whose value is empty.
    pub fn conflict_points(&self) -> KeyValues<'a> {
        self.tables[3]
    }

    /// The conflict statements: a list key and the object it changed.
    pub fn conflict_statements(&self) -> KeyValues<'a> {
        self.tables[4]
    }

    /// The conflict ranges: the first key and the key past the last.
    pub fn conflict_ranges(&self) -> KeyValues<'a> {
        self.tables[5]
    }

    /// The number of commit-time operations' results.
    pub fn result_count(&self) -> usize {
        self.results.len() / OPERATION_RESULT_SIZE
    }

    /// Result `index`, in the order the transaction asked its operations; `None` past the last.
    pub fn result(&self, index: usize) -> Option<OperationResult> {
        result_at(self.results, index)
    }

    /// The results' term table: each node by its IRI or skolem IRI.
    #[must_use]
    pub fn result_terms(&self) -> TermTableView<'a> {
        self.result_terms
    }

    /// The owned record of the view's values.
    pub fn to_record(&self) -> WriteSetRecord {
        let mut record = WriteSetRecord::empty(self.kind, self.tenant);
        record.set_catalog(self.catalog);
        record.set_read_version(self.read_version);
        record.set_session(self.session, self.request);
        record.set_principal(self.principal);
        let id = |value: Option<&[u8]>| value.and_then(|v| <[u8; 16]>::try_from(v).ok());
        for entry in self.inserted().iter() {
            record.push_inserted(entry.key().to_vec(), id(entry.value()));
        }
        for entry in self.removed().iter() {
            record.push_removed(entry.key().to_vec(), id(entry.value()));
        }
        for entry in self.statement_records().iter() {
            record.push_statement_record(entry.key().to_vec(), entry.value().map(<[u8]>::to_vec));
        }
        (0..self.operation_count())
            .filter_map(|i| self.operation(i))
            .for_each(|operation| record.push_operation(operation));
        (0..self.registration_count())
            .filter_map(|i| self.registration(i))
            .for_each(|registration| record.push_registration(registration));
        record.set_commit_record(self.commit_record.to_vec());
        for entry in self.conflict_points().iter() {
            record.push_conflict_point(entry.key().to_vec());
        }
        let owned = |value: Option<&[u8]>| value.map_or_else(Vec::new, <[u8]>::to_vec);
        for entry in self.conflict_statements().iter() {
            record.push_conflict_statement(entry.key().to_vec(), owned(entry.value()));
        }
        for entry in self.conflict_ranges().iter() {
            record.push_conflict_range(entry.key().to_vec(), owned(entry.value()));
        }
        (0..self.result_count())
            .filter_map(|i| self.result(i))
            .for_each(|result| record.push_result(result));
        record.set_result_terms(self.result_terms.to_table());
        record
    }

    /// The number of items section `index` holds: its entries, operations, registrations, results or terms, or the
    /// bytes of the commit record and the term text.
    fn count(&self, index: usize) -> u64 {
        let table = |i: usize| self.tables.get(i).map_or(0, KeyValues::len);
        let count = match index {
            0..=2 => table(index),
            3 => self.operation_count(),
            4 => self.registration_count(),
            5 => self.commit_record.len(),
            6..=8 => table(index - 3),
            9 => self.result_count(),
            10 => self.result_terms.len(),
            _ => self.text_len,
        };
        count as u64 // lossless: a `usize` is at most 64 bits
    }

    /// The sections the record's kind holds (`PROTOCOL.md` §12.2): a transaction's writes and no operation, a Drop's
    /// one drop and key ranges and a read version of 0, a registration's one registration, a build's one build
    /// operation.
    fn check_kind(&self) -> Result<(), ProtocolError> {
        let command = Command::Round.number();
        // Per section, the count the kind takes; `None` for any.
        let (expected, drop): ([Option<u64>; RECORD_SECTIONS], bool) = match self.kind {
            RecordKind::Transaction if self.operation_count() == 0 => return Ok(()),
            RecordKind::Transaction => {
                let mut takes = [Some(0); RECORD_SECTIONS];
                takes[3] = Some(1);
                takes[8] = None;
                (takes, true)
            }
            RecordKind::Registration => {
                // One registration, or none for an entity got or created with no bucket group, whose dictionary
                // entries the round carries.
                let registrations = self.count(4);
                if registrations > 1 {
                    return Err(ProtocolError::count_mismatch(
                        command,
                        "record sections",
                        registrations,
                        1,
                    ));
                }
                let mut takes = [Some(0); RECORD_SECTIONS];
                takes[4] = None;
                (takes, false)
            }
            RecordKind::Build => {
                let mut takes = [Some(0); RECORD_SECTIONS];
                takes[3] = Some(1);
                (takes, false)
            }
        };
        for (index, takes) in expected.into_iter().enumerate() {
            let count = self.count(index);
            if let Some(takes) = takes.filter(|takes| *takes != count) {
                return Err(ProtocolError::count_mismatch(
                    command,
                    "record sections",
                    count,
                    takes,
                ));
            }
        }
        if let Some(operation) = self.operation(0).filter(|o| o.is_drop() != drop) {
            return Err(ProtocolError::field_value(
                command,
                "operation",
                operation.code().into(),
            ));
        }
        if drop && self.read_version != 0 {
            return Err(ProtocolError::field_value(command, "record", 24));
        }
        Ok(())
    }
}
