/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::utils::key_value_writer::write_entries;
use crate::utils::sections::write_sections;

/// One entry of a write-set record's tables: a key and its value, `None` to remove the key.
pub type OwnedEntry = (Vec<u8>, Option<Vec<u8>>);

/// What a worker builds for a transaction and nothing re-encodes: the transaction's writes as store entries and the
/// conflict keys of its writes. It is a section of a Round frame unchanged.
///
/// | Fixed part | Field | Type |
/// |---|---|---|
/// | 0 | `namespace` | u64 |
/// | 8 | `read_version` | u64 |
/// | 16 | `session` | u64 |
/// | 24 | `request` | u64 |
///
/// | Section | Contents |
/// |---|---|
/// | 0 | list deltas, in the posting format: key-value entries |
/// | 1 | dictionary entries |
/// | 2 | statement records |
/// | 3 | catalog entries |
/// | 4 | entries of the keyspace without versions |
/// | 5 | the commit record the client attached |
/// | 6 | conflict points: entries whose value is empty, a key written |
/// | 7 | conflict statements: a list key and the object it changed |
/// | 8 | conflict ranges: the first key and the key past the last |
///
/// A key-value section holds entries of `key_len` u32, `value_len` u32 (`u32::MAX` for a removal, which holds no
/// value), the key, the value, then zeros to a multiple of 8. The fixed part and the section table are those of every
/// body.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct WriteSetRecord {
    namespace: u64,
    read_version: u64,
    session: u64,
    request: u64,
    list_deltas: Vec<OwnedEntry>,
    dictionary: Vec<OwnedEntry>,
    statements: Vec<OwnedEntry>,
    catalog: Vec<OwnedEntry>,
    versionless: Vec<OwnedEntry>,
    commit_record: Vec<u8>,
    conflict_points: Vec<Vec<u8>>,
    conflict_statements: Vec<(Vec<u8>, Vec<u8>)>,
    conflict_ranges: Vec<(Vec<u8>, Vec<u8>)>,
}

impl WriteSetRecord {
    pub fn new(namespace: u64, read_version: u64, session: u64, request: u64) -> Self {
        Self {
            namespace,
            read_version,
            session,
            request,
            ..Self::default()
        }
    }

    pub fn push_list_delta(&mut self, key: Vec<u8>, delta: Vec<u8>) {
        self.list_deltas.push((key, Some(delta)));
    }

    pub fn push_dictionary(&mut self, key: Vec<u8>, value: Option<Vec<u8>>) {
        self.dictionary.push((key, value));
    }

    pub fn push_statement(&mut self, key: Vec<u8>, value: Option<Vec<u8>>) {
        self.statements.push((key, value));
    }

    pub fn push_catalog(&mut self, key: Vec<u8>, value: Option<Vec<u8>>) {
        self.catalog.push((key, value));
    }

    pub fn push_versionless(&mut self, key: Vec<u8>, value: Option<Vec<u8>>) {
        self.versionless.push((key, value));
    }

    pub fn set_commit_record(&mut self, record: Vec<u8>) {
        self.commit_record = record;
    }

    pub fn push_conflict_point(&mut self, key: Vec<u8>) {
        self.conflict_points.push(key);
    }

    pub fn push_conflict_statement(&mut self, key: Vec<u8>, object: Vec<u8>) {
        self.conflict_statements.push((key, object));
    }

    pub fn push_conflict_range(&mut self, start: Vec<u8>, end: Vec<u8>) {
        self.conflict_ranges.push((start, end));
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

    pub fn list_deltas(&self) -> &[OwnedEntry] {
        &self.list_deltas
    }

    pub fn dictionary(&self) -> &[OwnedEntry] {
        &self.dictionary
    }

    pub fn statements(&self) -> &[OwnedEntry] {
        &self.statements
    }

    pub fn catalog(&self) -> &[OwnedEntry] {
        &self.catalog
    }

    pub fn versionless(&self) -> &[OwnedEntry] {
        &self.versionless
    }

    pub fn commit_record(&self) -> &[u8] {
        &self.commit_record
    }

    pub fn conflict_points(&self) -> &[Vec<u8>] {
        &self.conflict_points
    }

    pub fn conflict_statements(&self) -> &[(Vec<u8>, Vec<u8>)] {
        &self.conflict_statements
    }

    pub fn conflict_ranges(&self) -> &[(Vec<u8>, Vec<u8>)] {
        &self.conflict_ranges
    }

    /// The record's bytes.
    pub fn encode(&self) -> Vec<u8> {
        let fixed: Vec<u8> = [
            self.namespace,
            self.read_version,
            self.session,
            self.request,
        ]
        .iter()
        .flat_map(|v| v.to_le_bytes())
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
        write_sections(
            &fixed,
            &[
                &table(&self.list_deltas),
                &table(&self.dictionary),
                &table(&self.statements),
                &table(&self.catalog),
                &table(&self.versionless),
                &self.commit_record,
                &points,
                &pairs(&self.conflict_statements),
                &pairs(&self.conflict_ranges),
            ],
        )
    }
}
