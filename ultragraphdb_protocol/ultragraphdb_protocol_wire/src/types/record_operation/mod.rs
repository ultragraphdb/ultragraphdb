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
    /// Drops the graph of this UID in the record's namespace: every key of it.
    DropGraph(u64),
    /// Drops the record's namespace's data, keeping its catalog.
    DropNamespaceData,
    /// Drops the record's namespace and writes its initial catalog.
    DropNamespace,
    /// Drops every quad key, keeping the dictionary, and writes the root namespace's initial catalog.
    DropAll,
}

impl RecordOperation {
    /// The operation's code: 1 drop a predicate, 2 swap a build in, 3 undo a build, 4 drop a graph, 5 drop a
    /// namespace's data, 6 drop a namespace, 7 drop every namespace.
    pub const fn code(&self) -> u16 {
        match self {
            RecordOperation::DropPredicate(_) => 1,
            RecordOperation::SwapBuild(_) => 2,
            RecordOperation::UndoBuild(_) => 3,
            RecordOperation::DropGraph(_) => 4,
            RecordOperation::DropNamespaceData => 5,
            RecordOperation::DropNamespace => 6,
            RecordOperation::DropAll => 7,
        }
    }

    /// The predicate's UID, the build's record number or the graph's UID; 0 for the drops of a namespace's data, a
    /// namespace and every namespace.
    pub const fn argument(&self) -> u64 {
        match self {
            RecordOperation::DropPredicate(a)
            | RecordOperation::SwapBuild(a)
            | RecordOperation::UndoBuild(a)
            | RecordOperation::DropGraph(a) => *a,
            RecordOperation::DropNamespaceData
            | RecordOperation::DropNamespace
            | RecordOperation::DropAll => 0,
        }
    }

    /// The operation of `code` with `argument`, `None` for a code of none, or for an argument other than 0 where the
    /// operation takes none.
    pub const fn from_parts(code: u16, argument: u64) -> Option<Self> {
        match (code, argument) {
            (1, a) => Some(RecordOperation::DropPredicate(a)),
            (2, a) => Some(RecordOperation::SwapBuild(a)),
            (3, a) => Some(RecordOperation::UndoBuild(a)),
            (4, a) => Some(RecordOperation::DropGraph(a)),
            (5, 0) => Some(RecordOperation::DropNamespaceData),
            (6, 0) => Some(RecordOperation::DropNamespace),
            (7, 0) => Some(RecordOperation::DropAll),
            _ => None,
        }
    }

    /// Whether the operation is a drop, the last record of its round: codes 1 and 4 to 7.
    pub const fn is_drop(&self) -> bool {
        !matches!(
            self,
            RecordOperation::SwapBuild(_) | RecordOperation::UndoBuild(_)
        )
    }
}
