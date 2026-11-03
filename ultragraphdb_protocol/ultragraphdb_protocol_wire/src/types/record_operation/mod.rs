/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::SystemId;

use crate::utils::constants::RECORD_OPERATION_SIZE;

/// An operation in a write-set record's section 3 (`PROTOCOL.md` §12.2), 24 bytes: `operation` u16, 6 zero bytes, a
/// 16-byte `argument`. A drop names its target and replay derives its key removals; a build names the catalog version
/// starting it.
///
/// | Code | Operation | Argument |
/// |---|---|---|
/// | 1 | drop a predicate | the predicate's system ID |
/// | 2 | drop a graph | the graph's system ID |
/// | 3 | drop a namespace's data, keeping its catalog | zero |
/// | 4 | drop everything in a namespace | zero |
/// | 5 | drop everything | zero |
/// | 6 | start an index build | the catalog version starting it, a little-endian u64, then 8 zero bytes |
/// | 7 | complete an index build | as 6 |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RecordOperation {
    /// 1: drops the predicate of this system ID in the record's namespace, in every graph.
    DropPredicate(SystemId),
    /// 2: drops the graph of this system ID in the record's namespace.
    DropGraph(SystemId),
    /// 3: drops the record's namespace's data, keeping its catalog.
    DropNamespaceData,
    /// 4: drops everything in the record's namespace.
    DropNamespace,
    /// 5: drops every quad key.
    DropAll,
    /// 6: starts the index build of this catalog version.
    StartBuild(u64),
    /// 7: completes the index build of this catalog version.
    CompleteBuild(u64),
}

impl RecordOperation {
    /// The operation's code, 1 to 7.
    pub fn code(&self) -> u16 {
        match self {
            RecordOperation::DropPredicate(_) => 1,
            RecordOperation::DropGraph(_) => 2,
            RecordOperation::DropNamespaceData => 3,
            RecordOperation::DropNamespace => 4,
            RecordOperation::DropAll => 5,
            RecordOperation::StartBuild(_) => 6,
            RecordOperation::CompleteBuild(_) => 7,
        }
    }

    /// The operation's 16-byte argument: a system ID, big-endian; a catalog version, a little-endian u64 then 8 zero
    /// bytes; or zero.
    pub fn argument(&self) -> [u8; 16] {
        match self {
            RecordOperation::DropPredicate(id) | RecordOperation::DropGraph(id) => id.to_be_bytes(),
            RecordOperation::StartBuild(catalog) | RecordOperation::CompleteBuild(catalog) => {
                let [c0, c1, c2, c3, c4, c5, c6, c7] = catalog.to_le_bytes();
                [c0, c1, c2, c3, c4, c5, c6, c7, 0, 0, 0, 0, 0, 0, 0, 0]
            }
            RecordOperation::DropNamespaceData
            | RecordOperation::DropNamespace
            | RecordOperation::DropAll => [0; 16],
        }
    }

    /// The operation of `code` with `argument`; `None` for a code naming none, or an argument other than the code takes:
    /// an invalid system ID, a catalog version with nonzero bytes after it, a nonzero argument where none is taken.
    pub fn from_parts(code: u16, argument: [u8; 16]) -> Option<Self> {
        let [c0, c1, c2, c3, c4, c5, c6, c7, rest @ ..] = argument;
        let catalog =
            (rest == [0; 8]).then_some(u64::from_le_bytes([c0, c1, c2, c3, c4, c5, c6, c7]));
        let none = argument == [0; 16];
        match code {
            1 => SystemId::decode(&argument)
                .ok()
                .map(RecordOperation::DropPredicate),
            2 => SystemId::decode(&argument)
                .ok()
                .map(RecordOperation::DropGraph),
            3 => none.then_some(RecordOperation::DropNamespaceData),
            4 => none.then_some(RecordOperation::DropNamespace),
            5 => none.then_some(RecordOperation::DropAll),
            6 => catalog.map(RecordOperation::StartBuild),
            7 => catalog.map(RecordOperation::CompleteBuild),
            _ => None,
        }
    }

    /// Whether the operation is a drop, codes 1 to 5, the last record of its round.
    pub fn is_drop(&self) -> bool {
        !matches!(
            self,
            RecordOperation::StartBuild(_) | RecordOperation::CompleteBuild(_)
        )
    }

    /// The operation's 24 bytes.
    pub fn encode(&self) -> [u8; RECORD_OPERATION_SIZE] {
        let fields = self
            .code()
            .to_le_bytes()
            .into_iter()
            .chain([0; 6])
            .chain(self.argument());
        let mut out = [0u8; RECORD_OPERATION_SIZE];
        out.iter_mut()
            .zip(fields)
            .for_each(|(byte, field)| *byte = field);
        out
    }
}
