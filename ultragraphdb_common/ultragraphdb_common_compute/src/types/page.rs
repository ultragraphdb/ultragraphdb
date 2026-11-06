/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_keys::QuadKey;

use crate::types::page_entry::PageEntry;
use crate::types::page_row::PageRow;
use crate::types::row_view::RowView;

/// What an Entries frame gave, its buffer returned: its keys and object encodings copied once, so a partition's stream holds
/// this and no frame while the merge drains it.
#[derive(Debug, Default)]
pub(crate) struct Page {
    entries: Vec<PageEntry>,
    bytes: Vec<u8>,
    rows: Vec<PageRow>,
    next: usize,
    named: usize,
}

impl Page {
    /// Empties the page, keeping what it allocated.
    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.bytes.clear();
        self.rows.clear();
        self.next = 0;
        self.named = 0;
    }

    /// Whether a row is left.
    pub(crate) fn has_row(&self) -> bool {
        self.next < self.rows.len()
    }

    /// Adds the entry `key`, parsed as `parsed`.
    pub(crate) fn add_entry(&mut self, key: &[u8], parsed: QuadKey) {
        self.entries.push(PageEntry {
            key: key.to_vec(),
            parsed,
        });
    }

    /// Adds a row of the last entry: `encoding`, under `statement`, of the property `property`.
    pub(crate) fn add_row(
        &mut self,
        encoding: &[u8],
        statement: Option<SystemId>,
        property: Option<SystemId>,
    ) {
        let start = self.bytes.len();
        self.bytes.extend_from_slice(encoding);
        self.rows.push(PageRow {
            entry: u32::try_from(self.entries.len().saturating_sub(1)).unwrap_or(u32::MAX),
            start: u32::try_from(start).unwrap_or(u32::MAX),
            end: u32::try_from(self.bytes.len()).unwrap_or(u32::MAX),
            statement,
            property,
        });
    }

    /// The row `at`, counted from the page's first.
    fn row_at(&self, at: usize) -> Option<RowView<'_>> {
        let row = self.rows.get(at)?;
        let entry = self.entries.get(row.entry as usize)?;
        Some(RowView {
            key: &entry.key,
            parsed: &entry.parsed,
            encoding: self.bytes.get(row.start as usize..row.end as usize)?,
            statement: row.statement,
            property: row.property,
        })
    }

    /// The next row.
    pub(crate) fn peek(&self) -> Option<RowView<'_>> {
        self.row_at(self.next)
    }

    /// Moves past the next row.
    pub(crate) fn advance(&mut self) {
        self.next = self.next.saturating_add(1);
    }

    /// Whether the next row has its names.
    pub(crate) fn next_is_named(&self) -> bool {
        self.next < self.named
    }

    /// Every row past those that have their names.
    pub(crate) fn rows_unnamed(&self) -> impl Iterator<Item = RowView<'_>> {
        (self.named.max(self.next)..self.rows.len()).filter_map(|at| self.row_at(at))
    }

    /// `rows` more rows, past those that have their names, have theirs.
    pub(crate) fn named_more(&mut self, rows: usize) {
        self.named = self.named.max(self.next).saturating_add(rows);
    }
}
