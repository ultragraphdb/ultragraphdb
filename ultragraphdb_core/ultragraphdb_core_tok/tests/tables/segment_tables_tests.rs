/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_tok::word_segments;

/// FNV-1a 64 of blevesearch/segment v0.9.1's direct word segmentation, by a Go program, per plane: for every scalar
/// value `c` of the plane and each context `x` of none, `a` and `1`, the input `x c x` gives `c` as four
/// little-endian bytes, `x`'s byte (0 for none), each segment's length as four little-endian bytes and its type as
/// one byte, a 0xfe if segmentation stopped on an error, and a 0xff.
///
/// The `tok_segment` golden vectors run Unicode 8.0.0's WordBreakTest, which takes a few runes of each break class;
/// this takes every rune of planes 0 to 3 and 14, which hold every character Unicode 8.0.0 assigns, so every range
/// of the generated tables is read.
const GO_PLANE_HASHES: [(u32, u64); 5] = [
    (0, 0x9472_eb0f_f953_925f),
    (1, 0xe2f3_4de5_0787_3cab),
    (2, 0xa88d_b493_bdde_bb1d),
    (3, 0x047c_841c_9178_e325),
    (14, 0xd540_f471_738f_a66a),
];

struct Fnv(u64);

impl Fnv {
    fn byte(&mut self, b: u8) {
        self.0 = (self.0 ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
    }

    fn u32(&mut self, x: u32) {
        x.to_le_bytes().into_iter().for_each(|b| self.byte(b));
    }
}

#[test]
fn test_segment_tables_every_scalar() {
    for (plane, want) in GO_PLANE_HASHES {
        let mut h = Fnv(0xcbf2_9ce4_8422_2325);
        for c in (plane << 16..(plane + 1) << 16).filter_map(char::from_u32) {
            for ctx in ["", "a", "1"] {
                let input = format!("{ctx}{c}{ctx}");
                h.u32(u32::from(c));
                h.byte(ctx.bytes().next().unwrap_or(0));
                let (segments, err) = word_segments(&input);
                for (text, typ) in segments {
                    h.u32(text.len() as u32);
                    h.byte(typ as u8);
                }
                if err.is_some() {
                    h.byte(0xfe);
                }
                h.byte(0xff);
            }
        }
        assert_eq!(h.0, want, "plane {plane}");
    }
}
