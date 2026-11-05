/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::SystemId;

use crate::types::build_outcome::BuildOutcome;
use crate::utils::constants::RECORD_OPERATION_SIZE;

/// An operation in a write-set record's section 3 (`PROTOCOL.md` §12.2), 24 bytes: `operation` u16, 6 zero bytes, a
/// 16-byte `argument`. A drop names its target and replay derives its key removals; a catalog version taking effect
/// names the version, and a build's completion the version that started it and how it ended.
///
/// | Code | Operation | Argument |
/// |---|---|---|
/// | 1 | drop a predicate | the predicate's system ID |
/// | 2 | drop a graph | the graph's system ID |
/// | 3 | drop a tenant's data, keeping its catalog | zero |
/// | 4 | drop everything in a tenant | zero |
/// | 6 | a catalog version takes effect | the version, a little-endian u64, then 8 zero bytes |
/// | 7 | complete an index build | the catalog version that started it, a little-endian u64; its outcome's byte; 7 zero bytes |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RecordOperation {
    /// 1: drops the predicate of this system ID in the record's tenant, in every graph.
    DropPredicate(SystemId),
    /// 2: drops the graph of this system ID in the record's tenant.
    DropGraph(SystemId),
    /// 3: drops the record's tenant's data, keeping its catalog.
    DropTenantData,
    /// 4: drops everything in the record's tenant.
    DropTenant,
    /// 6: this catalog version takes effect, starting the builds it asks for.
    CatalogTakesEffect(u64),
    /// 7: completes the index build the catalog version `version` started, as `outcome` says.
    CompleteBuild {
        /// The catalog version that started the build.
        version: u64,
        /// How the build ended.
        outcome: BuildOutcome,
    },
}

impl RecordOperation {
    /// The operation's code: 1 to 4, 6 or 7; 5 is unassigned.
    pub fn code(&self) -> u16 {
        match self {
            RecordOperation::DropPredicate(_) => 1,
            RecordOperation::DropGraph(_) => 2,
            RecordOperation::DropTenantData => 3,
            RecordOperation::DropTenant => 4,
            RecordOperation::CatalogTakesEffect(_) => 6,
            RecordOperation::CompleteBuild { .. } => 7,
        }
    }

    /// The operation's 16-byte argument: a system ID, big-endian; a catalog version, a little-endian u64, then for a
    /// build's completion its outcome's byte, and zero bytes; or zero.
    pub fn argument(&self) -> [u8; 16] {
        match self {
            RecordOperation::DropPredicate(id) | RecordOperation::DropGraph(id) => id.to_be_bytes(),
            RecordOperation::CatalogTakesEffect(catalog) => {
                let [c0, c1, c2, c3, c4, c5, c6, c7] = catalog.to_le_bytes();
                [c0, c1, c2, c3, c4, c5, c6, c7, 0, 0, 0, 0, 0, 0, 0, 0]
            }
            RecordOperation::CompleteBuild { version, outcome } => {
                let [c0, c1, c2, c3, c4, c5, c6, c7] = version.to_le_bytes();
                [
                    c0,
                    c1,
                    c2,
                    c3,
                    c4,
                    c5,
                    c6,
                    c7,
                    outcome.byte(),
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                ]
            }
            RecordOperation::DropTenantData | RecordOperation::DropTenant => [0; 16],
        }
    }

    /// The operation of `code` with `argument`; `None` for a code naming none, or an argument other than the code takes:
    /// an invalid system ID, a catalog version with nonzero bytes after it, a build's completion whose outcome byte
    /// names none or is followed by nonzero bytes, a nonzero argument where none is taken.
    pub fn from_parts(code: u16, argument: [u8; 16]) -> Option<Self> {
        let [c0, c1, c2, c3, c4, c5, c6, c7, outcome, rest @ ..] = argument;
        let version = u64::from_le_bytes([c0, c1, c2, c3, c4, c5, c6, c7]);
        let catalog = (outcome == 0 && rest == [0; 7]).then_some(version);
        let none = argument == [0; 16];
        match code {
            1 => SystemId::decode(&argument)
                .ok()
                .map(RecordOperation::DropPredicate),
            2 => SystemId::decode(&argument)
                .ok()
                .map(RecordOperation::DropGraph),
            3 => none.then_some(RecordOperation::DropTenantData),
            4 => none.then_some(RecordOperation::DropTenant),
            6 => catalog.map(RecordOperation::CatalogTakesEffect),
            7 => (rest == [0; 7])
                .then(|| BuildOutcome::from_byte(outcome))
                .flatten()
                .map(|outcome| RecordOperation::CompleteBuild { version, outcome }),
            _ => None,
        }
    }

    /// Whether the operation is a drop, codes 1 to 4, the last record of its round.
    pub fn is_drop(&self) -> bool {
        !matches!(
            self,
            RecordOperation::CatalogTakesEffect(_) | RecordOperation::CompleteBuild { .. }
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
