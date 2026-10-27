/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The kind of read a conflict abort names, the `detail` of [`crate::ErrorCode::Conflict`].
///
/// | Number | Kind |
/// |---|---|
/// | 1 | List: a list read whole |
/// | 2 | Statement: a statement tested |
/// | 3 | Range: a key range scanned |
/// | 4 | AbsentTerm: a dictionary key found absent |
/// | 5 | Catalog: the namespace's catalog |
/// | 6 | Drop: a range a drop wrote |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ConflictKind {
    List,
    Statement,
    Range,
    AbsentTerm,
    Catalog,
    Drop,
}

impl ConflictKind {
    pub const ALL: [ConflictKind; 6] = [
        ConflictKind::List,
        ConflictKind::Statement,
        ConflictKind::Range,
        ConflictKind::AbsentTerm,
        ConflictKind::Catalog,
        ConflictKind::Drop,
    ];

    pub fn number(self) -> u64 {
        ConflictKind::ALL
            .iter()
            .position(|k| *k == self)
            .map_or(0, |i| i as u64 + 1)
    }

    pub fn from_number(number: u64) -> Option<ConflictKind> {
        ConflictKind::ALL
            .get(usize::try_from(number).ok()?.checked_sub(1)?)
            .copied()
    }
}
