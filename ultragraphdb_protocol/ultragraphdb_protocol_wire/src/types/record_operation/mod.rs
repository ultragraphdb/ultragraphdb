/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// A write-set record's operation, section 10: a write the store computes when it applies the record, at the record's
/// commit version, rather than one the record carries (decided 2026-10-28). Its 16 bytes: `operation` u16, 6 zero
/// bytes, `argument` u64.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RecordOperation {
    /// Drops the predicate of this UID in the record's namespace: every key of it and its catalog entry.
    DropPredicate(u64),
    /// Swaps in the lists of the build of this record number, and its new declaration.
    SwapBuild(u64),
    /// Writes back the old declaration of the build of this record number, which failed.
    UndoBuild(u64),
}

impl RecordOperation {
    /// The operation's code: 1 drop, 2 swap, 3 undo.
    pub const fn code(&self) -> u16 {
        match self {
            RecordOperation::DropPredicate(_) => 1,
            RecordOperation::SwapBuild(_) => 2,
            RecordOperation::UndoBuild(_) => 3,
        }
    }

    /// The predicate's UID, or the build's record number.
    pub const fn argument(&self) -> u64 {
        match self {
            RecordOperation::DropPredicate(a)
            | RecordOperation::SwapBuild(a)
            | RecordOperation::UndoBuild(a) => *a,
        }
    }

    /// The operation of `code` with `argument`, `None` for a code of none.
    pub const fn from_parts(code: u16, argument: u64) -> Option<Self> {
        match code {
            1 => Some(RecordOperation::DropPredicate(argument)),
            2 => Some(RecordOperation::SwapBuild(argument)),
            3 => Some(RecordOperation::UndoBuild(argument)),
            _ => None,
        }
    }
}
