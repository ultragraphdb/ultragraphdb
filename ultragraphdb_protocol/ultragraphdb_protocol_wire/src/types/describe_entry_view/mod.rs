/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::describe_entry::DescribeEntry;
use crate::types::entry_kind::EntryKind;
use crate::types::retired_name_view::RetiredNameView;

/// An entry of a received Describe answer, read in place; see [`crate::DescribeEntry`] for its layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DescribeEntryView<'a> {
    kind: EntryKind,
    name: &'a str,
    bytes: &'a [u8],
    current: Option<&'a str>,
}

// A copy moves at most one cache line (Ultra rule 9).
const _: () = assert!(size_of::<DescribeEntryView<'static>>() <= 64);

impl<'a> DescribeEntryView<'a> {
    /// The entry of `kind` named `name` holding `bytes`; a renamed name's `current` name.
    pub(crate) fn new(
        kind: EntryKind,
        name: &'a str,
        bytes: &'a [u8],
        current: Option<&'a str>,
    ) -> Self {
        Self {
            kind,
            name,
            bytes,
            current,
        }
    }

    /// The entry's kind.
    #[must_use]
    pub fn kind(&self) -> EntryKind {
        self.kind
    }

    /// The name the catalog orders the entry by.
    #[must_use]
    pub fn name(&self) -> &'a str {
        self.name
    }

    /// The entry's bytes, borrowed from the answer: a catalog version's entry, a source's UTF-8 text, or a retired
    /// name's 24 bytes.
    #[must_use]
    pub fn bytes(&self) -> &'a [u8] {
        self.bytes
    }

    /// The retired name an entry of kind 20 holds; `None` for every other kind.
    #[must_use]
    pub fn retired_name(&self) -> Option<RetiredNameView<'a>> {
        if self.kind == EntryKind::RetiredName {
            RetiredNameView::read(self.bytes, |_| self.current)
        } else {
            None
        }
    }

    /// The entry as its sender holds it.
    pub(crate) fn to_entry(self) -> DescribeEntry {
        match self.retired_name() {
            Some(retired) => DescribeEntry::retired(self.name, retired.to_retired_name()),
            None => DescribeEntry::new(self.kind, self.name, self.bytes.to_vec()),
        }
    }
}
