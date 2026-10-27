/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod write_view_body_view;

use crate::types::string_list::StringList;
use crate::utils::changes::CHANGE_RECORD_SIZE;

/// The body of a received Write frame, read in place; see [`crate::Write`] for its layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteView<'a> {
    namespace: u64,
    records: &'a [u8],
    args: Vec<u32>,
    strings: StringList<'a>,
}

impl<'a> WriteView<'a> {
    pub fn namespace(&self) -> u64 {
        self.namespace
    }

    pub fn strings(&self) -> &StringList<'a> {
        &self.strings
    }

    /// The number of changes.
    pub fn len(&self) -> usize {
        self.records.len() / CHANGE_RECORD_SIZE
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}
