/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The version a Scan reads each range at (`PROTOCOL.md` §13.13), its command header's `at` byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScanAt {
    /// 1: at `read_version`, once the partition has closed `epoch`.
    ReadVersion,
    /// 2: at the partition's latest applied version, once it has closed `epoch`.
    Latest,
    /// 3: at the version bound of the partition's latest closed epoch, once that is at least `epoch`.
    LatestClosed,
    /// 4: at `read_version`, a version the client named, without waiting.
    Named,
}

impl ScanAt {
    /// The choice's byte.
    pub fn number(self) -> u8 {
        match self {
            ScanAt::ReadVersion => 1,
            ScanAt::Latest => 2,
            ScanAt::LatestClosed => 3,
            ScanAt::Named => 4,
        }
    }

    /// The choice of byte `number`; `None` for any byte but 1 to 4.
    pub fn from_number(number: u8) -> Option<ScanAt> {
        match number {
            1 => Some(ScanAt::ReadVersion),
            2 => Some(ScanAt::Latest),
            3 => Some(ScanAt::LatestClosed),
            4 => Some(ScanAt::Named),
            _ => None,
        }
    }

    /// Whether the Scan names a version in `read_version`: [`ScanAt::ReadVersion`] and [`ScanAt::Named`].
    pub fn names_version(self) -> bool {
        matches!(self, ScanAt::ReadVersion | ScanAt::Named)
    }
}
