/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_codec::{UidBlock, UidPack, encode};

#[test]
fn a_pack_returns_what_it_was_built_with() {
    let blocks = vec![
        UidBlock::new(1, vec![0x00, 0x01, 0x00, 0x00, 0x00], 2),
        UidBlock::new(1 << 32, vec![0x00; 5], 1),
    ];
    let pack = UidPack::new(4, blocks.clone());
    assert_eq!(pack.block_size(), 4);
    assert_eq!(pack.blocks(), &blocks[..]);
}

#[test]
fn the_default_pack_has_no_blocks() {
    let pack = UidPack::default();
    assert_eq!((pack.block_size(), pack.blocks().len()), (0, 0));
}

#[test]
fn the_encoder_appends_each_block_it_closes() {
    // Hand-derived: block size 2 closes [5, 6] and [7]; deltas 1 then nothing, one group each.
    let pack = encode(&[5, 6, 7], 2).expect("three UIDs pack");
    assert_eq!(
        pack,
        UidPack::new(
            2,
            vec![
                UidBlock::new(5, vec![0x00, 0x01, 0x00, 0x00, 0x00], 2),
                UidBlock::new(7, vec![0x00; 5], 1),
            ]
        )
    );
}
