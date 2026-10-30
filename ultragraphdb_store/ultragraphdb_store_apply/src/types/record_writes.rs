/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// A logged write-set record's writes, sections 0 to 4, borrowed from the record (7.9): the list deltas, in the
/// posting format, and the statistics deltas, by key; the dictionary entries; the statement records; the catalog
/// entries; the other entries without versions. A value of `None` is a removal.
#[derive(Debug, Clone, Default)]
pub struct RecordWrites<'r> {
    pub(crate) lists: Vec<(&'r [u8], &'r [u8])>,
    pub(crate) dictionary: Vec<(&'r [u8], Option<&'r [u8]>)>,
    pub(crate) statements: Vec<(&'r [u8], Option<&'r [u8]>)>,
    pub(crate) catalog: Vec<(&'r [u8], Option<&'r [u8]>)>,
    pub(crate) versionless: Vec<(&'r [u8], Option<&'r [u8]>)>,
}

impl<'r> RecordWrites<'r> {
    pub fn new(
        lists: Vec<(&'r [u8], &'r [u8])>,
        dictionary: Vec<(&'r [u8], Option<&'r [u8]>)>,
        statements: Vec<(&'r [u8], Option<&'r [u8]>)>,
        catalog: Vec<(&'r [u8], Option<&'r [u8]>)>,
        versionless: Vec<(&'r [u8], Option<&'r [u8]>)>,
    ) -> Self {
        Self {
            lists,
            dictionary,
            statements,
            catalog,
            versionless,
        }
    }
}
