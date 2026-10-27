/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_codec::UidBlock;

#[test]
fn a_block_returns_what_it_was_built_with() {
    // Distinct values in each field, so that a getter returning another field's value fails.
    let block = UidBlock::new(1 << 40, vec![0x00, 0x01, 0x02], 3);
    assert_eq!(block.base(), 1 << 40);
    assert_eq!(block.deltas(), &[0x00, 0x01, 0x02]);
    assert_eq!(block.num_uids(), 3);
}

#[test]
fn the_default_block_is_empty() {
    let block = UidBlock::default();
    assert_eq!(
        (block.base(), block.deltas(), block.num_uids()),
        (0, &[][..], 0)
    );
    assert_eq!(block, UidBlock::new(0, Vec::new(), 0));
}
