/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use xxhash_rust::xxh3::xxh3_64;

use crate::types::resume_row::ResumeRow;
use crate::types::term_table::TermTable;
use crate::utils::constants::ABSENT;
use crate::utils::sections::{u32_bytes, write_sections};

/// The bytes of a resume key's fixed part: `checksum`, `pattern` and `scope`.
pub(crate) const RESUME_FIXED: usize = 24;

/// The token a limited read continues from (`PROTOCOL.md` §11.8), which compute alone writes and decodes: the pattern's
/// hash, what binds it to its transaction and the last row's terms as text, under an XXH3-64 checksum; never a store key or
/// a system ID.
///
/// | Fixed part | Field | Type |
/// |---|---|---|
/// | 0 | `checksum` | u64: XXH3-64, seed 0, of the key's bytes from 8 to its end |
/// | 8 | `pattern` | u64: the pattern's hash, [`crate::ComputeReadView::pattern_hash`] |
/// | 16 | `scope` | u64: for a read-write transaction, XXH3-64 of its identity; for a read-only one, its `read_version` |
///
/// | Section | Contents |
/// |---|---|
/// | 0 | the last row: five u32 indices into the term table, graph, subject, predicate, object and reifier |
/// | 1 | the term table's records |
/// | 2 | the term table's text |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumeKey {
    pattern: u64,
    scope: u64,
    terms: TermTable,
    row: ResumeRow,
}

impl ResumeKey {
    /// The key continuing the read of hash `pattern` in `scope` after `row`, whose terms `terms` holds.
    #[must_use]
    pub fn new(pattern: u64, scope: u64, terms: TermTable, row: ResumeRow) -> Self {
        Self {
            pattern,
            scope,
            terms,
            row,
        }
    }

    /// The key's bytes.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let row = [
            self.row.graph().unwrap_or(ABSENT),
            self.row.subject(),
            self.row.predicate(),
            self.row.object(),
            self.row.reifier().unwrap_or(ABSENT),
        ];
        let (records, text) = self.terms.sections();
        let mut fixed = [0u8; RESUME_FIXED];
        fixed
            .iter_mut()
            .skip(8)
            .zip(
                self.pattern
                    .to_le_bytes()
                    .into_iter()
                    .chain(self.scope.to_le_bytes()),
            )
            .for_each(|(byte, field)| *byte = field);
        let mut key = write_sections(&fixed, &[&u32_bytes(&row), &records, &text]);
        let checksum = xxh3_64(key.get(8..).unwrap_or_default()).to_le_bytes();
        key.iter_mut()
            .zip(checksum)
            .for_each(|(byte, field)| *byte = field);
        key
    }
}
