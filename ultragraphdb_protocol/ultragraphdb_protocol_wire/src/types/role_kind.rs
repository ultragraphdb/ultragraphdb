/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// A role's kind (`PROTOCOL.md` §13): the frontend, compute or storage.
///
/// | Number | Kind |
/// |---|---|
/// | 1 | Frontend |
/// | 2 | Compute |
/// | 3 | Storage |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RoleKind {
    /// The frontend: the client endpoint, the sequencer and the metadata group's host.
    Frontend,
    /// A compute process, which answers reads.
    Compute,
    /// A storage process, which hosts partitions.
    Storage,
}

impl RoleKind {
    /// The kind's number, 1 to 3.
    pub fn number(self) -> u8 {
        match self {
            RoleKind::Frontend => 1,
            RoleKind::Compute => 2,
            RoleKind::Storage => 3,
        }
    }

    /// The kind numbered `number`; `None` for a number no kind has.
    pub fn from_number(number: u8) -> Option<RoleKind> {
        match number {
            1 => Some(RoleKind::Frontend),
            2 => Some(RoleKind::Compute),
            3 => Some(RoleKind::Storage),
            _ => None,
        }
    }
}
