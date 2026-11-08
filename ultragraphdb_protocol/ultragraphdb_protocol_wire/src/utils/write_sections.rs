/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What DirectWrite and ConditionalWrite share (`PROTOCOL.md` §6.6): sections 0 to 5.

use ultragraphdb_error_protocol::ProtocolError;

use crate::types::string_list::StringList;
use crate::types::term_table_view::TermTableView;
use crate::types::write_change::WriteChange;
use crate::utils::changes::{CHANGE_RECORD_SIZE, decode_change};
use crate::utils::sections::{Sections, u32s};

/// Sections 0 to 5 of a received body: the term table and the args checked, the names found empty; the records the
/// caller's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WriteSections<'a> {
    pub(crate) records: &'a [u8],
    pub(crate) args: Vec<u32>,
    pub(crate) terms: TermTableView<'a>,
}

impl<'a> WriteSections<'a> {
    /// `sections` checked in the protocol's order: the term table (2, 3); the names (4, 5), a string list holding no name
    /// ([`ProtocolError::count_mismatch`] `"names"`, the names' count against 0); the args a whole number of u32s
    /// ([`ProtocolError::section_length`], section 1).
    pub(crate) fn read<const N: usize>(
        command: u16,
        sections: &Sections<'a, N>,
    ) -> Result<Self, ProtocolError> {
        let terms = TermTableView::read(
            command,
            (2, 3),
            sections.offset(2),
            sections.get(2),
            sections.get(3),
        )?;
        let names = StringList::read(command, (4, 5), sections.get(4), sections.get(5))?;
        if !names.is_empty() {
            return Err(ProtocolError::count_mismatch(
                command,
                "names",
                names.len() as u64, // lossless: a `usize` is at most 64 bits
                0,
            ));
        }
        let args = u32s(command, 1, sections.get(1))?;
        Ok(Self {
            records: sections.get(0),
            args,
            terms,
        })
    }

    /// The number of whole change records.
    pub(crate) fn len(&self) -> usize {
        self.records.len() / CHANGE_RECORD_SIZE
    }

    /// Change `index` of the checked records, `None` past the last.
    pub(crate) fn change(&self, index: usize) -> Option<WriteChange> {
        decode_change(self.records, index, &self.args, &StringList::EMPTY)
    }

    /// The changes of the checked records, in order.
    pub(crate) fn changes(&self) -> Vec<WriteChange> {
        (0..self.len())
            .filter_map(|index| self.change(index))
            .collect()
    }
}
