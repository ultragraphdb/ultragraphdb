/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_codec::{CodecError, UidBlock, UidPack, encode};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::codec_tests::{id, sequences};

#[test]
fn test_a_pack_returns_what_it_was_built_with() -> TestResult {
    let blocks = vec![
        UidBlock::new(id(1, 0, 1)?, vec![0x80, 0x80, 0x80, 0x08], 2)?,
        UidBlock::new(id(1, 0, 1 << 32)?, Vec::new(), 1)?,
    ];
    let pack = UidPack::new(4, blocks.clone())?;
    assert_eq!(pack.block_size(), 4);
    assert_eq!(pack.blocks(), &blocks[..]);
    Ok(())
}

#[test]
fn test_the_default_pack_has_no_blocks() {
    let pack = UidPack::default();
    assert_eq!((pack.block_size(), pack.blocks().len()), (0, 0));
    assert_eq!(UidPack::new(0, Vec::new()), Ok(pack));
}

#[test]
fn test_the_encoder_appends_each_block_it_closes() -> TestResult {
    // Block size 2 closes [5, 6] and [7]: one delta of 2^24, LEB128 80 80 80 08, then none. By hand.
    let pack = encode(&sequences(5..=7)?, 2)?.context("three IDs pack")?;
    assert_eq!(
        pack,
        UidPack::new(
            2,
            vec![
                UidBlock::new(id(1, 0, 5)?, vec![0x80, 0x80, 0x80, 0x08], 2)?,
                UidBlock::new(id(1, 0, 7)?, Vec::new(), 1)?,
            ]
        )?
    );
    Ok(())
}

#[test]
fn test_blocks_must_rise_above_the_block_before() -> TestResult {
    // [1, 2] then a block from 2: its first equals the previous block's last.
    let overlapping = vec![
        UidBlock::new(id(1, 0, 1)?, vec![0x80, 0x80, 0x80, 0x08], 2)?,
        UidBlock::new(id(1, 0, 2)?, Vec::new(), 1)?,
    ];
    assert_eq!(
        UidPack::new(2, overlapping),
        Err(CodecError::blocks_out_of_order(1))
    );
    // A block below the one before it.
    let falling = vec![
        UidBlock::new(id(1, 0, 9)?, Vec::new(), 1)?,
        UidBlock::new(id(1, 0, 3)?, Vec::new(), 1)?,
    ];
    assert_eq!(
        UidPack::new(2, falling),
        Err(CodecError::blocks_out_of_order(1))
    );
    // An empty block orders by its first ID, below which the next block may not start.
    let after_empty = vec![
        UidBlock::new(id(1, 0, 1)?, Vec::new(), 1)?,
        UidBlock::new(id(1, 0, 5)?, Vec::new(), 0)?,
        UidBlock::new(id(1, 0, 5)?, Vec::new(), 1)?,
    ];
    assert_eq!(
        UidPack::new(2, after_empty),
        Err(CodecError::blocks_out_of_order(2))
    );
    let empty_below = vec![
        UidBlock::new(id(1, 0, 5)?, Vec::new(), 1)?,
        UidBlock::new(id(1, 0, 4)?, Vec::new(), 0)?,
    ];
    assert_eq!(
        UidPack::new(2, empty_below),
        Err(CodecError::blocks_out_of_order(1))
    );
    Ok(())
}
