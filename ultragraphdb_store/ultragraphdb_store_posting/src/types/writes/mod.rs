/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_codec::PostingList;
use ultragraphdb_traits_store::{Entry, VersionlessEntry};

/// What one request writes, apart from the request and its read: the deltas of its lists by key, the entries written at
/// its commit version, those written at their own versions, the writes of the keyspace without versions, whether its
/// commit advances the cache's epoch, and the rebuild records it removes. What a transaction executed on a worker hands
/// to the commit step, which writes it at the commit version the step gives it ([`crate::Lists::commit_round`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Writes {
    deltas: Vec<(Vec<u8>, PostingList)>,
    entries: Vec<Entry>,
    versioned: Vec<Entry>,
    versionless: Vec<VersionlessEntry>,
    advance_epoch: bool,
    removed_records: Vec<u64>,
}

impl Writes {
    pub(crate) fn new(
        deltas: Vec<(Vec<u8>, PostingList)>,
        entries: Vec<Entry>,
        versioned: Vec<Entry>,
        versionless: Vec<VersionlessEntry>,
        advance_epoch: bool,
        removed_records: Vec<u64>,
    ) -> Self {
        Self {
            deltas,
            entries,
            versioned,
            versionless,
            advance_epoch,
            removed_records,
        }
    }

    /// The deltas, by key, in key order: every list whose scratch layer held postings.
    pub fn deltas(&self) -> &[(Vec<u8>, PostingList)] {
        &self.deltas
    }

    /// The keys of the deltas, in key order.
    pub fn written_keys(&self) -> impl Iterator<Item = &[u8]> {
        self.deltas.iter().map(|(key, _)| key.as_slice())
    }

    /// The entries written at the commit version, whatever version each names, in the order they were added.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// The entries written at the versions they name, in the order they were added.
    pub fn versioned_entries(&self) -> &[Entry] {
        &self.versioned
    }

    /// The keys of the entries, at the commit version or at their own versions.
    pub fn entry_keys(&self) -> impl Iterator<Item = &[u8]> {
        self.entries.iter().chain(&self.versioned).map(Entry::key)
    }

    /// The writes of the keyspace without versions, in order.
    pub fn versionless(&self) -> &[VersionlessEntry] {
        &self.versionless
    }

    /// Whether the commit advances the cache's epoch.
    pub fn advances_epoch(&self) -> bool {
        self.advance_epoch
    }

    /// The rebuild records the commit removes.
    pub fn removed_records(&self) -> &[u64] {
        &self.removed_records
    }
}
