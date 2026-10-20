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

struct Tables {
    pos: [usize; TABLE_SIZE],
    ij: [usize; TABLE_SIZE],
}

const TABLES: Tables = build();

pub(crate) const LOOKUP_POS: [usize; TABLE_SIZE] = TABLES.pos;
pub(crate) const LOOKUP_IJ: [usize; TABLE_SIZE] = TABLES.ij;

const fn build() -> Tables {
    let mut t = Tables {
        pos: [0; TABLE_SIZE],
        ij: [0; TABLE_SIZE],
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
        t.pos[(ij << 2) + orig_orientation] = (pos << 2) + orientation;
        t.ij[(pos << 2) + orig_orientation] = (ij << 2) + orientation;
        return;
    }

    let level = level + 1;
    let i = i << 1;
    let j = j << 1;
    let pos = pos << 2;
    let r = POS_TO_IJ[orientation];
    let mut k = 0;
    while k < 4 {
        init_lookup_cell(
            t,
            level,
            i + (r[k] >> 1),
            j + (r[k] & 1),
            orig_orientation,
            pos + k,
            orientation ^ POS_TO_ORIENTATION[k],
        );
        k += 1;
    }
}
