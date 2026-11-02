/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The action a principal was refused, the `detail` of [`crate::ErrorCode::PermissionDenied`] (`SECURITY.md`).
///
/// | Number | Action |
/// |---|---|
/// | 1 | Read |
/// | 2 | Write |
/// | 3 | Declare |
/// | 4 | Drop |
/// | 5 | Admin |
/// | 6 | Operate |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Action {
    /// Reading statements and their properties, and seeing declarations.
    Read,
    /// Write, Operation and Commit of statements.
    Write,
    /// Declaring predicates, entity types, relations and indexes.
    Declare,
    /// The Drop command.
    Drop,
    /// Administering roles, grants and group mappings, and at the cluster principals and issuers.
    Admin,
    /// Cluster operations: namespaces, backup, restore, export.
    Operate,
}

impl Action {
    /// Every action, in the order of its number.
    pub const ALL: [Action; 6] = [
        Action::Read,
        Action::Write,
        Action::Declare,
        Action::Drop,
        Action::Admin,
        Action::Operate,
    ];

    /// The action's number, 1 to 6.
    pub fn number(self) -> u64 {
        Action::ALL
            .iter()
            .position(|a| *a == self)
            .map_or(0, |i| i as u64 + 1)
    }

    /// The action numbered `number`; `None` for a number no action has.
    pub fn from_number(number: u64) -> Option<Action> {
        Action::ALL
            .get(usize::try_from(number).ok()?.checked_sub(1)?)
            .copied()
    }
}
