/*
 * Copyright 2014 Google Inc. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from golang/geo v0.0.0-20260427214057-41a1a8c7eb2a, s2/cellid.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! The Hilbert curve lookup tables. Go fills them in `init`; here they are evaluated at
//! compile time by the same recursion.

pub(crate) const LOOKUP_BITS: u32 = 4;
pub(crate) const SWAP_MASK: usize = 0x01;
pub(crate) const INVERT_MASK: usize = 0x02;

const TABLE_SIZE: usize = 1 << (2 * LOOKUP_BITS + 2);

/// ijToPos[orientation][ij] -> pos
pub(crate) const IJ_TO_POS: [[usize; 4]; 4] = [
    [0, 1, 3, 2], // canonical order
    [0, 3, 1, 2], // axes swapped
    [2, 3, 1, 0], // bits inverted
    [2, 1, 3, 0], // swapped & inverted
];

/// posToIJ[orientation][pos] -> ij
pub(crate) const POS_TO_IJ: [[usize; 4]; 4] = [
    [0, 1, 3, 2], // canonical order:    (0,0), (0,1), (1,1), (1,0)
    [0, 2, 3, 1], // axes swapped:       (0,0), (1,0), (1,1), (0,1)
    [3, 2, 0, 1], // bits inverted:      (1,1), (1,0), (0,0), (0,1)
    [3, 1, 0, 2], // swapped & inverted: (1,1), (0,1), (0,0), (1,0)
];

/// posToOrientation[pos] -> orientation_modifier
pub(crate) const POS_TO_ORIENTATION: [usize; 4] = [SWAP_MASK, 0, 0, INVERT_MASK | SWAP_MASK];

/// The entry of a four-entry table at a two-bit index; an orientation holds two mask bits and a child position two
/// bits, so the masked index is the index itself.
pub(crate) fn quad<T: Copy>(table: [T; 4], index: usize) -> T {
    let [t0, t1, t2, t3] = table;
    match index & 3 {
        0 => t0,
        1 => t1,
        2 => t2,
        _ => t3,
    }
}

/// The entry of a lookup table at a ten-bit index: a key or value of the form "iiiijjjjoo" or "ppppppppoo" is
/// below TABLE_SIZE, so the masked index is the index itself and the entry is always present.
pub(crate) fn lookup(table: &[usize; TABLE_SIZE], index: usize) -> usize {
    table
        .get(index & (TABLE_SIZE - 1))
        .copied()
        .unwrap_or_default()
}

struct Tables {
    pos: [usize; TABLE_SIZE],
    ij: [usize; TABLE_SIZE],
    /// Whether every entry the recursion wrote lay within its table; the assertion below fails the build otherwise.
    complete: bool,
}

const TABLES: Tables = build();
const _: () = assert!(
    TABLES.complete,
    "a Hilbert curve lookup entry lies outside its table"
);

pub(crate) const LOOKUP_POS: [usize; TABLE_SIZE] = TABLES.pos;
pub(crate) const LOOKUP_IJ: [usize; TABLE_SIZE] = TABLES.ij;

const fn build() -> Tables {
    let mut t = Tables {
        pos: [0; TABLE_SIZE],
        ij: [0; TABLE_SIZE],
        complete: true,
    };
    init_lookup_cell(&mut t, 0, 0, 0, 0, 0, 0);
    init_lookup_cell(&mut t, 0, 0, 0, SWAP_MASK, 0, SWAP_MASK);
    init_lookup_cell(&mut t, 0, 0, 0, INVERT_MASK, 0, INVERT_MASK);
    init_lookup_cell(
        &mut t,
        0,
        0,
        0,
        SWAP_MASK | INVERT_MASK,
        0,
        SWAP_MASK | INVERT_MASK,
    );
    t
}

/// Writes `value` at `at`, reporting whether `at` lies within the table.
const fn put(table: &mut [usize; TABLE_SIZE], at: usize, value: usize) -> bool {
    match table.split_at_mut_checked(at) {
        Some((_, [slot, ..])) => {
            *slot = value;
            true
        }
        _ => false,
    }
}

/// Initializes the lookupIJ table at init time.
const fn init_lookup_cell(
    t: &mut Tables,
    level: u32,
    i: usize,
    j: usize,
    orig_orientation: usize,
    pos: usize,
    orientation: usize,
) {
    if level == LOOKUP_BITS {
        let ij = (i << LOOKUP_BITS) + j;
        let pos_ok = put(
            &mut t.pos,
            (ij << 2) + orig_orientation,
            (pos << 2) + orientation,
        );
        let ij_ok = put(
            &mut t.ij,
            (pos << 2) + orig_orientation,
            (ij << 2) + orientation,
        );
        t.complete = t.complete && pos_ok && ij_ok;
        return;
    }

    let level = level + 1;
    let i = i << 1;
    let j = j << 1;
    let pos = pos << 2;
    let Some((_, [r, ..])) = POS_TO_IJ.split_at_checked(orientation) else {
        t.complete = false;
        return;
    };
    let [r0, r1, r2, r3] = *r;
    let [o0, o1, o2, o3] = POS_TO_ORIENTATION;
    let quadrants = [(r0, o0), (r1, o1), (r2, o2), (r3, o3)];
    let mut rest: &[(usize, usize)] = &quadrants;
    let mut k = 0;
    while let [(rk, ok), tail @ ..] = rest {
        init_lookup_cell(
            t,
            level,
            i + (*rk >> 1),
            j + (*rk & 1),
            orig_orientation,
            pos + k,
            orientation ^ *ok,
        );
        rest = tail;
        k += 1;
    }
}
