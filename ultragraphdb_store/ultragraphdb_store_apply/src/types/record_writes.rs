/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::marked_reference::MarkedReference;

/// A logged write-set record's writes, borrowed from the record (7.9): the list deltas, in the posting format, and the
/// statistics deltas, by key; the dictionary entries; the statement records; the other entries without versions; and
/// section 4, the registrations the transaction relied on as a referrer, each written as its mark. A value of `None`
/// is a removal. A record holds no catalog entry: no partition holds catalog keys.
#[derive(Debug, Clone, Default)]
pub struct RecordWrites<'r> {
    pub(crate) lists: Vec<(&'r [u8], &'r [u8])>,
    pub(crate) dictionary: Vec<(&'r [u8], Option<&'r [u8]>)>,
    pub(crate) statements: Vec<(&'r [u8], Option<&'r [u8]>)>,
    pub(crate) versionless: Vec<(&'r [u8], Option<&'r [u8]>)>,
    pub(crate) marks: Vec<MarkedReference>,
}

impl<'r> RecordWrites<'r> {
    /// The writes of a record's sections: `lists`, `dictionary`, `statements` and `versionless`, in that order.
    pub fn new(
        lists: Vec<(&'r [u8], &'r [u8])>,
        dictionary: Vec<(&'r [u8], Option<&'r [u8]>)>,
        statements: Vec<(&'r [u8], Option<&'r [u8]>)>,
        versionless: Vec<(&'r [u8], Option<&'r [u8]>)>,
    ) -> Self {
        Self {
            lists,
            dictionary,
            statements,
            versionless,
            marks: Vec::new(),
        }
    }

    /// The writes with section 4's `marks`, in the record's order.
    pub fn with_marks(mut self, marks: Vec<MarkedReference>) -> Self {
        self.marks = marks;
        self
    }
}
