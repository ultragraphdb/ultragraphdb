/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::revision::Revision;

/// One target: its chain of revisions, the `n`th at index `n - 1`, and which is active.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct TargetState {
    pub(crate) revisions: Vec<Revision>,
    pub(crate) active: Option<u64>,
    pub(crate) retired: bool,
}

impl TargetState {
    /// The latest revision's number, 0 for none.
    pub(crate) fn latest(&self) -> u64 {
        self.revisions.len() as u64 // lossless: a `usize` is at most 64 bits
    }

    /// Revision `number`.
    pub(crate) fn revision(&self, number: u64) -> Option<&Revision> {
        let index = usize::try_from(number.checked_sub(1)?).ok()?;
        self.revisions.get(index)
    }
}
