/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::operation_result::OperationResult;
use crate::types::record_kind::RecordKind;
use crate::types::record_operation::RecordOperation;
use crate::types::record_registration::RecordRegistration;
use crate::types::term_table::TermTable;
use crate::utils::key_value_writer::write_entries;
use crate::utils::results::encode_results;
use crate::utils::sections::write_sections_into;

/// One entry of a write-set record's key-value sections: a key and its value, `None` for a removal.
pub type OwnedEntry = (Vec<u8>, Option<Vec<u8>>);

/// The logical write-set record of one round entry (`PROTOCOL.md` §12.2): what a worker builds for a transaction and
/// what a partition's commit step logs for each committed entry, a transaction's resolved quads with their system IDs,
/// its drops and deletes in their logical form, its session-table entry, the registrations it relied on as a referrer
/// and the conflict keys of its writes; never a key the store derives from them. The commit step writes it again,
/// once, for a transaction that ran commit-time operations, with their writes and results.
///
/// The body layout of §4.1 with a 64-byte fixed part and 12 sections; offsets count from the record's first byte.
///
/// | Fixed part | Field | Type |
/// |---|---|---|
/// | 0 | `kind` | u8, [`RecordKind`]; then 7 zero bytes |
/// | 8 | `tenant` | u64 |
/// | 16 | `catalog` | u64: kinds 1 and 3, the tenant's catalog version in effect at the round's epoch; 0 for kind 2 |
/// | 24 | `read_version` | u64: kind 1, the version the transaction read at; 0 otherwise and for a Drop |
/// | 32 | `session` | 16 bytes: kind 1, the session committing through it, zero for none; zero otherwise |
/// | 48 | `request` | u64: kind 1, the session's request number, 0 for none; 0 otherwise |
/// | 56 | `principal` | u64: kind 1, the principal the transaction acted for; 0 otherwise |
///
/// | Section | Contents |
/// |---|---|
/// | 0 | statements inserted: each key a quad, its value the statement's ID, 16 bytes, or empty for none |
/// | 1 | statements removed, as section 0: a delete by pattern as the statements it resolved to |
/// | 2 | statement records: each key a graph and a statement ID, its value the record or a removal |
/// | 3 | operations, 24 bytes each ([`RecordOperation`]) |
/// | 4 | registrations, 32 bytes each ([`RecordRegistration`]) |
/// | 5 | the commit record the client attached |
/// | 6 | conflict points: each a key written, its value empty |
/// | 7 | conflict statements: a list key and the object it changed |
/// | 8 | conflict ranges: the first key and the key past the last |
/// | 9 | the commit-time operations' results, 16 bytes each ([`OperationResult`]), in the order asked |
/// | 10 | the results' term table's records: each node an IRI entry, its IRI or skolem IRI |
/// | 11 | the results' term table's text |
///
/// | Kind | Fixed part | Sections |
/// |---|---|---|
/// | 1, a transaction | every field | 0 to 2 and 4 to 11 as it wrote and answered; 3 empty |
/// | 1, a Drop | every field, `read_version` 0 | 3 one drop; 8 the dropped key ranges; every other empty |
/// | 2 | `tenant` | 4 one registration, or none for an entity got or created with no bucket group; every other empty |
/// | 3 | `tenant`, `catalog` | 3 one build operation; every other empty |
///
/// A key-value section holds entries of `key_len` u32, `value_len` u32 ([`crate::ABSENT`] for a removal, which holds
/// no value), the key, the value, then zeros to a multiple of 8.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteSetRecord {
    kind: RecordKind,
    tenant: u64,
    catalog: u64,
    read_version: u64,
    session: [u8; 16],
    request: u64,
    principal: u64,
    inserted: Vec<OwnedEntry>,
    removed: Vec<OwnedEntry>,
    statement_records: Vec<OwnedEntry>,
    operations: Vec<RecordOperation>,
    registrations: Vec<RecordRegistration>,
    commit_record: Vec<u8>,
    conflict_points: Vec<Vec<u8>>,
    conflict_statements: Vec<(Vec<u8>, Vec<u8>)>,
    conflict_ranges: Vec<(Vec<u8>, Vec<u8>)>,
    results: Vec<OperationResult>,
    result_terms: TermTable,
}

impl WriteSetRecord {
    /// An empty record of a transaction or a Drop (kind 1) in `tenant`, every other field zero.
    pub fn transaction(tenant: u64) -> Self {
        Self::empty(RecordKind::Transaction, tenant)
    }

    /// An empty record of `kind` in `tenant`, every other field zero.
    pub(crate) fn empty(kind: RecordKind, tenant: u64) -> Self {
        Self {
            kind,
            tenant,
            catalog: 0,
            read_version: 0,
            session: [0; 16],
            request: 0,
            principal: 0,
            inserted: Vec::new(),
            removed: Vec::new(),
            statement_records: Vec::new(),
            operations: Vec::new(),
            registrations: Vec::new(),
            commit_record: Vec::new(),
            conflict_points: Vec::new(),
            conflict_statements: Vec::new(),
            conflict_ranges: Vec::new(),
            results: Vec::new(),
            result_terms: TermTable::new(),
        }
    }

    /// The record of `registration`, which the partition accepted as its entity's home (kind 2), in `tenant`.
    pub fn registration(tenant: u64, registration: RecordRegistration) -> Self {
        let mut record = Self::empty(RecordKind::Registration, tenant);
        record.registrations.push(registration);
        record
    }

    /// The record of an entity the partition got or created as its home for a registration naming no bucket group (kind
    /// 2 with no registration), in `tenant`: its dictionary entries ride in the round's dictionary section.
    pub fn entity(tenant: u64) -> Self {
        Self::empty(RecordKind::Registration, tenant)
    }

    /// The record of an index build's start or completion (kind 3), `operation`, in `tenant`; its catalog set by
    /// [`WriteSetRecord::set_catalog`].
    pub fn build(tenant: u64, operation: RecordOperation) -> Self {
        let mut record = Self::empty(RecordKind::Build, tenant);
        record.operations.push(operation);
        record
    }

    /// Sets the tenant's catalog version the entry was validated against.
    pub fn set_catalog(&mut self, catalog: u64) {
        self.catalog = catalog;
    }

    /// Sets the version the transaction read at.
    pub fn set_read_version(&mut self, read_version: u64) {
        self.read_version = read_version;
    }

    /// Sets the session committing through the transaction and its request number.
    pub fn set_session(&mut self, session: [u8; 16], request: u64) {
        self.session = session;
        self.request = request;
    }

    /// Sets the principal the transaction acted for.
    pub fn set_principal(&mut self, principal: u64) {
        self.principal = principal;
    }

    /// Sets the commit record the client attached.
    pub fn set_commit_record(&mut self, commit_record: Vec<u8>) {
        self.commit_record = commit_record;
    }

    /// Appends a statement inserted: its `quad` and its `statement` ID, `None` for a statement without one.
    pub fn push_inserted(&mut self, quad: Vec<u8>, statement: Option<[u8; 16]>) {
        self.inserted
            .push((quad, Some(statement.map_or_else(Vec::new, Vec::from))));
    }

    /// Appends a statement removed: its `quad` and its `statement` ID, `None` for a statement without one.
    pub fn push_removed(&mut self, quad: Vec<u8>, statement: Option<[u8; 16]>) {
        self.removed
            .push((quad, Some(statement.map_or_else(Vec::new, Vec::from))));
    }

    /// Appends the statement record of `key`, a graph and a statement ID: `record`, `None` for its removal.
    pub fn push_statement_record(&mut self, key: Vec<u8>, record: Option<Vec<u8>>) {
        self.statement_records.push((key, record));
    }

    /// Appends an operation: a Drop's drop, or a build's start or completion.
    pub fn push_operation(&mut self, operation: RecordOperation) {
        self.operations.push(operation);
    }

    /// Appends a registration: one the transaction relied on as a referrer, or the one a home accepted.
    pub fn push_registration(&mut self, registration: RecordRegistration) {
        self.registrations.push(registration);
    }

    /// Appends the conflict point `key`.
    pub fn push_conflict_point(&mut self, key: Vec<u8>) {
        self.conflict_points.push(key);
    }

    /// Appends the conflict statement of the list `key` and the `object` it changed.
    pub fn push_conflict_statement(&mut self, key: Vec<u8>, object: Vec<u8>) {
        self.conflict_statements.push((key, object));
    }

    /// Appends the conflict range from `start` to the key `end` past the last.
    pub fn push_conflict_range(&mut self, start: Vec<u8>, end: Vec<u8>) {
        self.conflict_ranges.push((start, end));
    }

    /// Appends the result of an insert-if-absent or upsert-and-link naming the node of IRI or skolem IRI `node`,
    /// interned in the results' term table, and whether the operation created it.
    pub fn push_node_result(&mut self, node: &str, created: bool) {
        let index = self.result_terms.iri(node);
        self.results.push(OperationResult::for_node(index, created));
    }

    /// Appends a commit-time operation's result. A result naming a node names an index of the results' term table,
    /// which [`WriteSetRecord::push_node_result`] fills.
    pub fn push_result(&mut self, result: OperationResult) {
        self.results.push(result);
    }

    /// The record's kind.
    pub fn kind(&self) -> RecordKind {
        self.kind
    }

    /// The tenant.
    pub fn tenant(&self) -> u64 {
        self.tenant
    }

    /// The tenant's catalog version the entry was validated against; 0 for a registration.
    pub fn catalog(&self) -> u64 {
        self.catalog
    }

    /// The version the transaction read at; 0 for a Drop, a registration and a build.
    pub fn read_version(&self) -> u64 {
        self.read_version
    }

    /// The session committing through the transaction; zero for none.
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

    /// The statements inserted: each a quad and its statement ID, empty for none.
    pub fn inserted(&self) -> &[OwnedEntry] {
        &self.inserted
    }

    /// The statements removed: each a quad and its statement ID, empty for none.
    pub fn removed(&self) -> &[OwnedEntry] {
        &self.removed
    }

    /// The statement records: each a graph and statement ID, and the record or its removal.
    pub fn statement_records(&self) -> &[OwnedEntry] {
        &self.statement_records
    }

    /// The operations.
    pub fn operations(&self) -> &[RecordOperation] {
        &self.operations
    }

    /// The registrations.
    pub fn registrations(&self) -> &[RecordRegistration] {
        &self.registrations
    }

    /// The commit record the client attached.
    pub fn commit_record(&self) -> &[u8] {
        &self.commit_record
    }

    /// The conflict points: each a key written.
    pub fn conflict_points(&self) -> &[Vec<u8>] {
        &self.conflict_points
    }

    /// The conflict statements: a list key and the object it changed.
    pub fn conflict_statements(&self) -> &[(Vec<u8>, Vec<u8>)] {
        &self.conflict_statements
    }

    /// The conflict ranges: the first key and the key past the last.
    pub fn conflict_ranges(&self) -> &[(Vec<u8>, Vec<u8>)] {
        &self.conflict_ranges
    }

    /// The commit-time operations' results, in the order the transaction asked them.
    pub fn results(&self) -> &[OperationResult] {
        &self.results
    }

    /// The results' term table: each node by its IRI or skolem IRI.
    pub fn result_terms(&self) -> &TermTable {
        &self.result_terms
    }

    /// The record's bytes.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        self.encode_into(&mut out);
        out
    }

    /// Writes the record's bytes into `out`, cleared first and its capacity kept: the log's buffer holds them with no
    /// copy between.
    pub fn encode_into(&self, out: &mut Vec<u8>) {
        let fixed: Vec<u8> = [self.kind.code(), 0, 0, 0, 0, 0, 0, 0]
            .into_iter()
            .chain(self.tenant.to_le_bytes())
            .chain(self.catalog.to_le_bytes())
            .chain(self.read_version.to_le_bytes())
            .chain(self.session)
            .chain(self.request.to_le_bytes())
            .chain(self.principal.to_le_bytes())
            .collect();
        let table = |entries: &[OwnedEntry]| {
            write_entries(entries.iter().map(|(k, v)| (k.as_slice(), v.as_deref())))
        };
        let pairs = |entries: &[(Vec<u8>, Vec<u8>)]| {
            write_entries(
                entries
                    .iter()
                    .map(|(k, v)| (k.as_slice(), Some(v.as_slice()))),
            )
        };
        let points = write_entries(
            self.conflict_points
                .iter()
                .map(|k| (k.as_slice(), Some(&[][..]))),
        );
        let operations: Vec<u8> = self
            .operations
            .iter()
            .flat_map(RecordOperation::encode)
            .collect();
        let registrations: Vec<u8> = self
            .registrations
            .iter()
            .flat_map(RecordRegistration::encode)
            .collect();
        let (term_records, term_text) = self.result_terms.sections();
        write_sections_into(
            out,
            &fixed,
            &[
                &table(&self.inserted),
                &table(&self.removed),
                &table(&self.statement_records),
                &operations,
                &registrations,
                &self.commit_record,
                &points,
                &pairs(&self.conflict_statements),
                &pairs(&self.conflict_ranges),
                &encode_results(&self.results),
                &term_records,
                &term_text,
            ],
        );
    }

    /// Sets the results' term table, as a received record holds it.
    pub(crate) fn set_result_terms(&mut self, result_terms: TermTable) {
        self.result_terms = result_terms;
    }
}
