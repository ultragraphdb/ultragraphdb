/*
 * Copyright 2013 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from golang.org/x/text v0.41.0, internal/language/lookup.go and internal/language/language.go (type
 * Region). Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::tables::language_tables::{
    ISO_REGION_OFFSET, M49, N_REGION_GROUPS, REGION_CONTAINMENT, REGION_INCLUSION,
    REGION_INCLUSION_BITS, REGION_ISO, REGION_OLD_MAP, REGION_TYPES, REGION_XK,
};
use crate::utils::tag_index::elem;

/// An ISO 3166-1 or UN M.49 region identifier. Zero is unspecified; from `ISO_REGION_OFFSET` on the identifiers
/// index the ISO region table.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct RegionId(pub u16);

const ISO3166_USER_ASSIGNED: u8 = 1;

impl RegionId {
    fn typ(self) -> u8 {
        REGION_TYPES
            .get(usize::from(self.0))
            .copied()
            .unwrap_or_default()
    }

    /// The BCP 47 representation, `ZZ` for an unspecified region.
    pub(crate) fn as_string(self) -> String {
        if self.0 < ISO_REGION_OFFSET {
            if self.0 == 0 {
                return "ZZ".to_string();
            }
            return format!("{:03}", self.m49());
        }
        let r = usize::from(self.0 - ISO_REGION_OFFSET);
        let [a, b, ..] = elem(REGION_ISO, r);
        String::from_utf8_lossy(&[a, b]).into_owned()
    }

    /// The UN M.49 code, or zero.
    pub(crate) fn m49(self) -> i32 {
        M49.get(usize::from(self.0)).map_or(0, |&m| i32::from(m))
    }

    /// Whether the region has the ISO 3166 user-assigned status.
    pub(crate) fn is_private_use(self) -> bool {
        self.typ() & ISO3166_USER_ASSIGNED != 0
    }

    /// Whether the region is a country or autonomous area.
    pub(crate) fn is_country(self) -> bool {
        !(self.0 == 0 || self.is_group() || self.is_private_use() && self.0 != REGION_XK)
    }

    /// Whether the region is a collection of regions.
    pub(crate) fn is_group(self) -> bool {
        if self.0 == 0 {
            return false;
        }
        REGION_INCLUSION
            .get(usize::from(self.0))
            .is_some_and(|&g| usize::from(g) < REGION_CONTAINMENT.len())
    }

    /// Whether `c` is contained by this region; true if both are equal.
    pub(crate) fn contains(self, c: RegionId) -> bool {
        if self == c {
            return true;
        }
        let Some(&g) = REGION_INCLUSION.get(usize::from(self.0)) else {
            return false;
        };
        if g >= N_REGION_GROUPS {
            return false;
        }
        let (Some(&m), Some(&d)) = (
            REGION_CONTAINMENT.get(usize::from(g)),
            REGION_INCLUSION.get(usize::from(c.0)),
        ) else {
            return false;
        };
        let Some(&b) = REGION_INCLUSION_BITS.get(usize::from(d)) else {
            return false;
        };
        // A contained country may belong to multiple disjoint groups. Matching any of these indicates containment.
        // If the contained region is a group, it must strictly be a subset.
        if d >= N_REGION_GROUPS {
            return b & m != 0;
        }
        b & !m == 0
    }

    /// The replacement of a deprecated region, or the region itself.
    pub(crate) fn canonicalize(self) -> RegionId {
        let k = REGION_OLD_MAP.partition_point(|old| old.from < self.0);
        if let Some(old) = REGION_OLD_MAP.get(k)
            && old.from == self.0
            && old.to != 0
        {
            return RegionId(old.to);
        }
        self
    }
}
