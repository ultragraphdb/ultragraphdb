/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::entry_kind::EntryKind;
use crate::types::retired_name::RetiredName;

/// An entry of a Describe answer, as its sender holds it (`PROTOCOL.md` §8.3): its kind, the name the catalog orders it
/// by, and its bytes, which the answer's section 8 carries opaque. A retired name's 24 bytes name its current name by
/// an index into the answer's string list, so it is held as a [`RetiredName`] and laid out when the answer is built.
///
/// | Entry offset | Field | Type |
/// |---|---|---|
/// | 0 | `kind` | u8 |
/// | 1 | zero | 3 bytes |
/// | 4 | `name` | u32: a string index; for the report, the empty string |
/// | 8 | `offset` | u32: the entry's first byte in section 8, where the bytes of the entry before end |
/// | 12 | `len` | u32: its bytes' length |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DescribeEntry {
    kind: EntryKind,
    name: String,
    bytes: Vec<u8>,
    retired: Option<RetiredName>,
}

impl DescribeEntry {
    /// The entry of `kind` named `name`, holding `bytes`: a catalog version's entry as it encodes it, or a source's
    /// text.
    #[must_use]
    pub fn new(kind: EntryKind, name: impl Into<String>, bytes: Vec<u8>) -> Self {
        Self {
            kind,
            name: name.into(),
            bytes,
            retired: None,
        }
    }

    /// The retired name `name`, an entry of kind 20.
    #[must_use]
    pub fn retired(name: impl Into<String>, retired: RetiredName) -> Self {
        Self {
            kind: EntryKind::RetiredName,
            name: name.into(),
            bytes: Vec::new(),
            retired: Some(retired),
        }
    }

    /// The entry's kind.
    #[must_use]
    pub fn kind(&self) -> EntryKind {
        self.kind
    }

    /// The name the catalog orders the entry by.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The entry's bytes; empty for a retired name, laid out from [`DescribeEntry::retired_name`].
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The retired name an entry of kind 20 holds; `None` for every other kind.
    #[must_use]
    pub fn retired_name(&self) -> Option<&RetiredName> {
        self.retired.as_ref()
    }
}
