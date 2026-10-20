/*
 * Copyright 2012 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from golang.org/x/text v0.41.0, internal/colltab/collelem.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::utils::unicode::is_ideographic;

/// A collation element. Normal elements have a primary weight or a non-default secondary weight, not both:
///
/// ```text
/// 01pppppp pppppppp ppppppp0 ssssssss   primary p, secondary s
/// 00pppppp pppppppp ppppppps sssttttt   primary p, secondary offset s, tertiary t
/// 100ttttt cccccccc pppppppp pppppppp   tertiary t, combining class c, primary p
/// 1010cccc ccccssss ssssssss tttttttt   combining class c, secondary s, tertiary t
/// 11qqqqqq qqqqqqqq qqqqqqq0 00000000   quaternary q
/// ```
///
/// Values from `0xC0000000` on in a table are contraction, expansion and decomposition indexes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Elem(pub u32);

/// The comparison levels after the primary one (level 0).
pub(crate) const SECONDARY: usize = 1;
pub(crate) const TERTIARY: usize = 2;
pub(crate) const QUATERNARY: usize = 3;
pub(crate) const IDENTITY: usize = 4;
pub(crate) const NUM_LEVELS: usize = 5;

pub(crate) const DEFAULT_SECONDARY: i32 = 0x20;
pub(crate) const DEFAULT_TERTIARY: i32 = 0x2;
pub(crate) const MAX_TERTIARY: u8 = 0x1F;
/// The largest quaternary weight, 21 bits.
pub(crate) const MAX_QUATERNARY: i32 = 0x1FFFFF;

const MAX_CE: u32 = 0xAFFFFFFF;
const MAX_CONTRACT: u32 = 0xDFFFFFFF;
const MAX_EXPAND: u32 = 0xEFFFFFFF;

/// The kind of a table element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CeType {
    /// A collation element, implicits (zero) included.
    Normal,
    /// The rune can start a contraction.
    ContractionIndex,
    /// The rune expands into a sequence of collation elements.
    ExpansionIndex,
    /// The rune expands using its NFKD decomposition.
    Decompose,
}

const CE_TYPE_MASK: u32 = 0xC0000000;
const CE_TYPE_MASK_EXT: u32 = 0xE0000000;
const CE_IGNORE_MASK: u32 = 0xF00FFFFF;
const CE_TYPE1: u32 = 0x40000000;
const CE_TYPE2: u32 = 0x00000000;
const CE_TYPE3_OR4: u32 = 0x80000000;
const CE_TYPE4: u32 = 0xA0000000;
const CE_TYPE_Q: u32 = 0xC0000000;
/// The element ignored at every level.
pub(crate) const IGNORE: Elem = Elem(CE_TYPE4);
const FIRST_NON_PRIMARY: u32 = 0x80000000;
const LAST_SPECIAL_PRIMARY: u32 = 0xA0000000;
const HAS_TERTIARY_MASK: u32 = 0x40000000;
const PRIMARY_VALUE_MASK: u32 = 0x3FFFFE00;
const MAX_PRIMARY_BITS: u32 = 21;
const COMPACT_PRIMARY_BITS: u32 = 16;
const MAX_SECONDARY_BITS: u32 = 12;
const MAX_TERTIARY_BITS: u32 = 8;
const MAX_CCC_BITS: u32 = 8;
const MAX_SECONDARY_COMPACT_BITS: u32 = 8;
const MAX_SECONDARY_DIFF_BITS: u32 = 4;
const MAX_TERTIARY_COMPACT_BITS: u32 = 5;
const PRIMARY_SHIFT: u32 = 9;
const COMPACT_SECONDARY_SHIFT: u32 = 5;
const MIN_COMPACT_SECONDARY: i32 = DEFAULT_SECONDARY - 4;

const MAX_N_BITS: u32 = 4;
const MAX_TRIE_INDEX_BITS: u32 = 12;
const MAX_CONTRACT_OFFSET_BITS: u32 = 13;

// From https://www.unicode.org/versions/Unicode6.0.0/ch12.pdf.
const MIN_UNIFIED: u32 = 0x4E00;
const MAX_UNIFIED: u32 = 0x9FFF;
const MIN_COMPATIBILITY: u32 = 0xF900;
const MAX_COMPATIBILITY: u32 = 0xFAFF;
const COMMON_UNIFIED_OFFSET: i32 = 0x10000;
const RARE_UNIFIED_OFFSET: i32 = 0x20000; // largest rune in common is U+FAFF
const OTHER_OFFSET: i32 = 0x50000; // largest rune in rare is U+2FA1D

/// The largest number of digits a numeric sequence counts.
pub(crate) const MAX_DIGITS: i32 = (1 << MAX_PRIMARY_BITS) - 1;

impl Elem {
    pub(crate) fn ctype(self) -> CeType {
        if self.0 <= MAX_CE {
            return CeType::Normal;
        }
        if self.0 <= MAX_CONTRACT {
            return CeType::ContractionIndex;
        }
        if self.0 <= MAX_EXPAND {
            return CeType::ExpansionIndex;
        }
        CeType::Decompose
    }

    /// The element of an implicit primary weight.
    pub(crate) fn make_implicit(primary: i32) -> Elem {
        Elem(CE_TYPE1 | (primary << PRIMARY_SHIFT) as u32 | DEFAULT_SECONDARY as u32)
    }

    /// Go's `MakeElem`: the element of the given weights, or `None` for an invalid combination.
    pub(crate) fn make(primary: i32, secondary: i32, tertiary: i32, ccc: u8) -> Option<Elem> {
        if !(0..1 << MAX_PRIMARY_BITS).contains(&primary)
            || !(0..1 << MAX_SECONDARY_BITS).contains(&secondary)
            || !(0..1 << MAX_TERTIARY_BITS).contains(&tertiary)
        {
            return None;
        }
        let ce = if primary != 0 {
            if ccc != 0 {
                if primary >= 1 << COMPACT_PRIMARY_BITS || secondary != DEFAULT_SECONDARY {
                    return None;
                }
                let mut ce = (tertiary as u32) << (COMPACT_PRIMARY_BITS + MAX_CCC_BITS);
                ce |= u32::from(ccc) << COMPACT_PRIMARY_BITS;
                ce |= primary as u32;
                ce | CE_TYPE3_OR4
            } else if tertiary == DEFAULT_TERTIARY {
                if secondary >= 1 << MAX_SECONDARY_COMPACT_BITS {
                    return None;
                }
                ((primary << (MAX_SECONDARY_COMPACT_BITS + 1)) + secondary) as u32 | CE_TYPE1
            } else {
                let d = secondary - DEFAULT_SECONDARY + MAX_SECONDARY_DIFF_BITS as i32;
                if !(0..1 << MAX_SECONDARY_DIFF_BITS).contains(&d) {
                    return None;
                }
                if tertiary >= 1 << MAX_TERTIARY_COMPACT_BITS {
                    return None;
                }
                let ce = ((primary << MAX_SECONDARY_DIFF_BITS) + d) as u32;
                (ce << MAX_TERTIARY_COMPACT_BITS) + tertiary as u32
            }
        } else {
            let mut ce = ((secondary << MAX_TERTIARY_BITS) + tertiary) as u32;
            ce += u32::from(ccc) << (MAX_SECONDARY_BITS + MAX_TERTIARY_BITS);
            ce | CE_TYPE4
        };
        Some(Elem(ce))
    }

    /// The element of a quaternary weight.
    pub(crate) fn make_quaternary(v: i32) -> Elem {
        Elem(CE_TYPE_Q | (v << PRIMARY_SHIFT) as u32)
    }

    /// The canonical combining class of the underlying character, or zero.
    pub(crate) fn ccc(self) -> u8 {
        if self.0 & CE_TYPE3_OR4 != 0 {
            if self.0 & CE_TYPE4 == CE_TYPE3_OR4 {
                return (self.0 >> 16) as u8;
            }
            return (self.0 >> 20) as u8;
        }
        0
    }

    pub(crate) fn primary(self) -> i32 {
        if self.0 >= FIRST_NON_PRIMARY {
            if self.0 > LAST_SPECIAL_PRIMARY {
                return 0;
            }
            return i32::from(self.0 as u16);
        }
        ((self.0 & PRIMARY_VALUE_MASK) >> PRIMARY_SHIFT) as i32
    }

    pub(crate) fn secondary(self) -> i32 {
        match self.0 & CE_TYPE_MASK {
            CE_TYPE1 => i32::from(self.0 as u8),
            CE_TYPE2 => MIN_COMPACT_SECONDARY + ((self.0 >> COMPACT_SECONDARY_SHIFT) & 0xF) as i32,
            CE_TYPE3_OR4 => {
                if self.0 < CE_TYPE4 {
                    return DEFAULT_SECONDARY;
                }
                ((self.0 >> 8) & 0xFFF) as i32
            }
            _ => 0, // ceTypeQ
        }
    }

    pub(crate) fn tertiary(self) -> u8 {
        if self.0 & HAS_TERTIARY_MASK == 0 {
            if self.0 & CE_TYPE3_OR4 == 0 {
                return (self.0 & 0x1F) as u8;
            }
            if self.0 & CE_TYPE4 == CE_TYPE4 {
                return self.0 as u8;
            }
            return ((self.0 >> 24) as u8) & 0x1F; // type 2
        } else if self.0 & CE_TYPE_MASK == CE_TYPE1 {
            return DEFAULT_TERTIARY as u8;
        }
        // A quaternary value.
        0
    }

    pub(crate) fn update_tertiary(self, t: u8) -> Elem {
        let mut ce = self.0;
        if ce & CE_TYPE_MASK == CE_TYPE1 {
            // convert to type 4
            let mut nce = ce & PRIMARY_VALUE_MASK;
            nce |= u32::from((ce as u8).wrapping_sub(MIN_COMPACT_SECONDARY as u8))
                << COMPACT_SECONDARY_SHIFT;
            ce = nce;
        } else if ce & CE_TYPE_MASK_EXT == CE_TYPE3_OR4 {
            ce &= !(u32::from(MAX_TERTIARY) << 24);
            return Elem(ce | (u32::from(t) << 24));
        } else {
            // type 2 or 4
            ce &= !u32::from(MAX_TERTIARY);
        }
        Elem(ce | u32::from(t))
    }

    /// The quaternary weight if explicitly specified, zero for `IGNORE`, the largest one otherwise.
    pub(crate) fn quaternary(self) -> i32 {
        if self.0 & CE_TYPE_MASK == CE_TYPE_Q {
            return ((self.0 & PRIMARY_VALUE_MASK) >> PRIMARY_SHIFT) as i32;
        } else if self.0 & CE_IGNORE_MASK == IGNORE.0 {
            return 0;
        }
        MAX_QUATERNARY
    }

    /// The contraction trie index, the size of its first node and the offset into the contraction elements.
    pub(crate) fn split_contract_index(self) -> (usize, usize, usize) {
        let mut ce = self.0;
        let n = (ce & ((1 << MAX_N_BITS) - 1)) as usize;
        ce >>= MAX_N_BITS;
        let index = (ce & ((1 << MAX_TRIE_INDEX_BITS) - 1)) as usize;
        ce >>= MAX_TRIE_INDEX_BITS;
        let offset = (ce & ((1 << MAX_CONTRACT_OFFSET_BITS) - 1)) as usize;
        (index, n, offset)
    }

    /// The index into the expansion table.
    pub(crate) fn split_expand_index(self) -> usize {
        usize::from(self.0 as u16)
    }

    /// The replacement tertiary weights of the first and the second rune of a decomposition.
    pub(crate) fn split_decompose(self) -> (u8, u8) {
        (self.0 as u8, (self.0 >> 8) as u8)
    }
}

/// The primary weight of a rune with no entry in the collation table. It preserves the relative order of
/// https://unicode.org/reports/tr10/#Implicit_Weights with different values.
pub(crate) fn implicit_primary(r: u32) -> i32 {
    if is_ideographic(r) {
        if (MIN_UNIFIED..=MAX_UNIFIED).contains(&r) {
            // The most common case for CJK.
            return r as i32 + COMMON_UNIFIED_OFFSET;
        }
        if (MIN_COMPATIBILITY..=MAX_COMPATIBILITY).contains(&r) {
            // This will typically not hit. The DUCET explicitly specifies mappings for all characters that do
            // not decompose.
            return r as i32 + COMMON_UNIFIED_OFFSET;
        }
        return r as i32 + RARE_UNIFIED_OFFSET;
    }
    r as i32 + OTHER_OFFSET
}
