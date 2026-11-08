/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The size of a statement ID in an object record.
pub(crate) const STATEMENT_ID_SIZE: usize = 16;

/// One object of a list's value (`PROTOCOL.md` §13.14), read in place: its encoding and the statement IDs it is held
/// under.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ObjectRecord<'a> {
    encoding: &'a [u8],
    statement_ids: &'a [u8],
}

impl<'a> ObjectRecord<'a> {
    /// The record of `encoding` and `statement_ids`, a whole number of IDs.
    pub(crate) fn new(encoding: &'a [u8], statement_ids: &'a [u8]) -> Self {
        Self {
            encoding,
            statement_ids,
        }
    }

    /// The object as the store's keys encode it, a system ID for an IRI or blank node.
    #[must_use]
    pub fn encoding(&self) -> &'a [u8] {
        self.encoding
    }

    /// The number of statement IDs the object is held under.
    #[must_use]
    pub fn statement_count(&self) -> usize {
        self.statement_ids.len() / STATEMENT_ID_SIZE
    }

    /// The statement IDs the object is held under, as they are, in order.
    pub fn statement_ids(&self) -> impl Iterator<Item = [u8; STATEMENT_ID_SIZE]> + 'a {
        let (ids, _) = self.statement_ids.as_chunks::<STATEMENT_ID_SIZE>();
        ids.iter().copied()
    }
}
