/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::retired_fate::RetiredFate;
use crate::types::retired_item::RetiredItem;
use crate::utils::constants::{ABSENT, RETIRED_NAME_SIZE};

/// A name an item held in an earlier catalog version, a Describe entry of kind 20, as its sender holds it
/// (`PROTOCOL.md` §8.3).
///
/// | Retired name offset | Field | Type |
/// |---|---|---|
/// | 0 | `item` | u8: 1 a type, 2 an attribute, 3 a role, 4 a navigation name, 5 a graph |
/// | 1 | `fate` | u8: 1 renamed; 2 retired |
/// | 2 | zero | 2 bytes |
/// | 4 | `current` | u32: for `fate` 1, the item's current name, a string index; `ABSENT` for 2 |
/// | 8 | `version` | u64: the catalog version whose cutover retired the name, at least 1 |
/// | 16 | `until` | u64: for `fate` 1, the end of the deprecation window in microseconds of Unix time; 0 for 2 |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetiredName {
    item: RetiredItem,
    fate: RetiredFate,
    current: Option<String>,
    version: u64,
    until: u64,
}

impl RetiredName {
    /// A name renamed to `current` by the cutover of catalog `version`, a query naming it taking the current name
    /// until the deprecation window ends.
    #[must_use]
    pub fn renamed(item: RetiredItem, current: impl Into<String>, version: u64) -> Self {
        Self {
            item,
            fate: RetiredFate::Renamed,
            current: Some(current.into()),
            version,
            until: 0,
        }
    }

    /// A name retired by a split, a union or a drop at the cutover of catalog `version`, holding no item.
    #[must_use]
    pub fn retired(item: RetiredItem, version: u64) -> Self {
        Self {
            item,
            fate: RetiredFate::Retired,
            current: None,
            version,
            until: 0,
        }
    }

    /// A renamed name whose deprecation window ends at `until`, microseconds of Unix time.
    #[must_use]
    pub fn until(self, until: u64) -> Self {
        Self { until, ..self }
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
    pub fn current(&self) -> Option<&str> {
        self.current.as_deref()
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

    /// The name's 24 bytes, its current name the string index `current`; `ABSENT` for a retired one.
    pub(crate) fn encode(&self, current: Option<u32>) -> [u8; RETIRED_NAME_SIZE] {
        let fields = [self.item.number(), self.fate.number(), 0, 0]
            .into_iter()
            .chain(current.unwrap_or(ABSENT).to_le_bytes())
            .chain(self.version.to_le_bytes())
            .chain(self.until.to_le_bytes());
        let mut out = [0u8; RETIRED_NAME_SIZE];
        out.iter_mut()
            .zip(fields)
            .for_each(|(byte, field)| *byte = field);
        out
    }
}
