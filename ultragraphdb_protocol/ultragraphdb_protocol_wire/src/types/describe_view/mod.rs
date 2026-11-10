/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod describe_view_body_view;

use ultragraphdb_core_body::StringList;

use crate::types::describe_entry_view::DescribeEntryView;
use crate::types::entry_kind::EntryKind;
use crate::types::retired_name_view::RetiredNameView;
use crate::utils::changes::CHANGE_RECORD_SIZE;
use crate::utils::constants::DESCRIBE_ENTRY_SIZE;
use crate::utils::le::bytes_at;

/// The body of a received Describe frame, read in place; see [`crate::Describe`] for its layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DescribeView<'a> {
    tenant: u64,
    read_version: u64,
    strings: StringList<'a>,
    labels: Vec<u32>,
    relationship_types: Vec<u32>,
    property_keys: Vec<u32>,
    records: &'a [u8],
    args: Vec<u32>,
    entries: &'a [u8],
    entry_bytes: &'a [u8],
}

impl<'a> DescribeView<'a> {
    /// The tenant described.
    pub fn tenant(&self) -> u64 {
        self.tenant
    }

    /// The snapshot the answer was read at; zero in a request.
    pub fn read_version(&self) -> u64 {
        self.read_version
    }

    /// The strings the body's indices name.
    pub fn strings(&self) -> &StringList<'a> {
        &self.strings
    }

    /// The labels, in order.
    pub fn labels(&self) -> impl Iterator<Item = &'a str> + '_ {
        self.labels
            .iter()
            .filter_map(|i| self.strings.get(*i as usize))
    }

    /// The relationship types, in order.
    pub fn relationship_types(&self) -> impl Iterator<Item = &'a str> + '_ {
        self.relationship_types
            .iter()
            .filter_map(|i| self.strings.get(*i as usize))
    }

    /// The property keys, in order.
    pub fn property_keys(&self) -> impl Iterator<Item = &'a str> + '_ {
        self.property_keys
            .iter()
            .filter_map(|i| self.strings.get(*i as usize))
    }

    /// The number of declarations.
    pub fn declarations(&self) -> usize {
        self.records.len() / CHANGE_RECORD_SIZE
    }

    /// The number of entries.
    #[must_use]
    pub fn entry_count(&self) -> usize {
        self.entries.len() / DESCRIBE_ENTRY_SIZE
    }

    /// Entry `index`, read in place; `None` past the last.
    #[must_use]
    pub fn entry(&self, index: usize) -> Option<DescribeEntryView<'a>> {
        let record = self
            .entries
            .get(index.checked_mul(DESCRIBE_ENTRY_SIZE)?..)?
            .get(..DESCRIBE_ENTRY_SIZE)?;
        let field = |at: usize| bytes_at(record, at).map(u32::from_le_bytes);
        let kind = EntryKind::from_number(*record.first()?)?;
        // The casts are lossless: a `usize` is at least 32 bits.
        let name = self.strings.get(field(4)? as usize)?;
        let start = field(8)? as usize;
        let bytes = self
            .entry_bytes
            .get(start..start.checked_add(field(12)? as usize)?)?;
        // A renamed name's current name, by the index its bytes hold.
        let current = match kind {
            EntryKind::RetiredName => {
                RetiredNameView::read(bytes, |index| self.strings.get(index as usize)) // lossless: a `usize` is at least 32 bits
                    .and_then(|retired| retired.current())
            }
            _ => None,
        };
        Some(DescribeEntryView::new(kind, name, bytes, current))
    }

    /// The entries in order.
    pub fn entries(&self) -> impl Iterator<Item = DescribeEntryView<'a>> + '_ {
        (0..self.entry_count()).filter_map(|index| self.entry(index))
    }
}
