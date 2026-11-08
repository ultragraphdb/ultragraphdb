/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// What a Report says (`PROTOCOL.md` §13.19): the byte `kind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReportKind {
    /// 1: the type change's check passed.
    Passed,
    /// 2: the data refused the change.
    Refused,
    /// 3: an index build failed.
    BuildFailed,
}

impl ReportKind {
    /// The byte.
    #[must_use]
    pub fn number(self) -> u8 {
        match self {
            ReportKind::Passed => 1,
            ReportKind::Refused => 2,
            ReportKind::BuildFailed => 3,
        }
    }

    /// The kind the byte `number` names; `None` for 0 and past 3.
    #[must_use]
    pub fn from_number(number: u8) -> Option<Self> {
        match number {
            1 => Some(ReportKind::Passed),
            2 => Some(ReportKind::Refused),
            3 => Some(ReportKind::BuildFailed),
            _ => None,
        }
    }
}
