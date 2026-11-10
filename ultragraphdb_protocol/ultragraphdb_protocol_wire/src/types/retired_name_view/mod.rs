/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::retired_fate::RetiredFate;
use crate::types::retired_item::RetiredItem;
use crate::types::retired_name::RetiredName;
use crate::utils::constants::{ABSENT, RETIRED_NAME_SIZE};
use crate::utils::le::bytes_at;

/// A retired name of a received Describe answer, read in place; see [`crate::RetiredName`] for its layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetiredNameView<'a> {
    item: RetiredItem,
    fate: RetiredFate,
    current: Option<&'a str>,
    version: u64,
    until: u64,
}

// A copy moves at most one cache line (Ultra rule 9).
const _: () = assert!(size_of::<RetiredNameView<'static>>() <= 64);

impl<'a> RetiredNameView<'a> {
    /// The retired name `bytes` hold, its current name the string `current` gives for its index; `None` unless they are
    /// its 24 bytes as `PROTOCOL.md` §8.3 lays them out: bytes 2 and 3 zero, `item` 1 to 5, `fate` 1 or 2, `current` a
    /// string for a renamed name and `ABSENT` for a retired one, `version` at least 1 and `until` 0 for a retired one.
    pub(crate) fn read(bytes: &[u8], current: impl FnOnce(u32) -> Option<&'a str>) -> Option<Self> {
        let record: [u8; RETIRED_NAME_SIZE] = bytes.try_into().ok()?;
        let [item, fate, zero_2, zero_3, ..] = record;
        if zero_2 != 0 || zero_3 != 0 {
            return None;
        }
        let item = RetiredItem::from_number(item)?;
        let fate = RetiredFate::from_number(fate)?;
        let index = u32::from_le_bytes(bytes_at(&record, 4)?);
        let version = u64::from_le_bytes(bytes_at(&record, 8)?);
        let until = u64::from_le_bytes(bytes_at(&record, 16)?);
        let current = match fate {
            RetiredFate::Renamed => Some(current(index)?),
            RetiredFate::Retired if index == ABSENT && until == 0 => None,
            RetiredFate::Retired => return None,
        };
        (version >= 1).then_some(Self {
            item,
            fate,
            current,
            version,
            until,
        })
    }

    /// The name as its sender holds it.
    pub(crate) fn to_retired_name(self) -> RetiredName {
        match self.current {
            Some(current) => {
                RetiredName::renamed(self.item, current, self.version).until(self.until)
            }
            None => RetiredName::retired(self.item, self.version),
        }
    }

    /// What the name named.
    #[must_use]
    pub fn item(&self) -> RetiredItem {
        self.item
    }

    /// What became of it.
    #[must_use]
    pub fn fate(&self) -> RetiredFate {
        self.fate
    }

    /// The item's current name, for a renamed one.
    #[must_use]
    pub fn current(&self) -> Option<&'a str> {
        self.current
    }

    /// The catalog version whose cutover retired the name.
    #[must_use]
    pub fn version(&self) -> u64 {
        self.version
    }

    /// When the deprecation window ends, in microseconds of Unix time; 0 for a retired name.
    #[must_use]
    pub fn deprecated_until(&self) -> u64 {
        self.until
    }
}
