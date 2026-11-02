/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_codec::{CodecError, UidBlock};
use ultragraphdb_core_ids::{Bucket, IdError};
use ultragraphdb_tool_bdd::TestResult;

use crate::codec_tests::id;

#[test]
fn test_a_block_returns_what_it_was_built_with() -> TestResult {
    // Distinct values in each field, so that a getter returning another field's value fails: deltas 1 and 2^24.
    let block = UidBlock::new(id(1, 7, 1 << 40)?, vec![0x01, 0x80, 0x80, 0x80, 0x08], 3)?;
    assert_eq!(block.first(), id(1, 7, 1 << 40)?);
    assert_eq!(block.deltas(), &[0x01, 0x80, 0x80, 0x80, 0x08]);
    assert_eq!(block.count(), 3);
    Ok(())
}

#[test]
fn test_an_empty_block_holds_no_delta() -> TestResult {
    let block = UidBlock::new(id(1, 0, 11)?, Vec::new(), 0)?;
    assert_eq!((block.first(), block.count()), (id(1, 0, 11)?, 0));
    assert_eq!(
        UidBlock::new(id(1, 0, 11)?, vec![0x01], 0),
        Err(CodecError::trailing_deltas(0))
    );
    Ok(())
}

#[test]
fn test_fewer_deltas_than_the_count_are_truncated() -> TestResult {
    // A count of 3 needs two deltas; one is there, and the second would start at byte 1.
    assert_eq!(
        UidBlock::new(id(1, 0, 1)?, vec![0x01], 3),
        Err(CodecError::truncated_varint(1))
    );
    // A delta cut inside: its byte carries the continuation bit.
    assert_eq!(
        UidBlock::new(id(1, 0, 1)?, vec![0x80], 2),
        Err(CodecError::truncated_varint(0))
    );
    Ok(())
}

#[test]
fn test_bytes_after_the_last_delta_are_refused() -> TestResult {
    assert_eq!(
        UidBlock::new(id(1, 0, 1)?, vec![0x01, 0x01], 2),
        Err(CodecError::trailing_deltas(1))
    );
    assert_eq!(
        UidBlock::new(id(1, 0, 1)?, vec![0x01], 1),
        Err(CodecError::trailing_deltas(0))
    );
    Ok(())
}

#[test]
fn test_a_delta_longer_than_15_bytes_overflows() -> TestResult {
    // 104 bits take at most 15 groups of seven; a 16th byte is past any delta. The second delta starts at byte 1.
    let mut deltas = vec![0x01];
    deltas.extend_from_slice(&[0x80; 15]);
    deltas.push(0x01);
    assert_eq!(
        UidBlock::new(id(1, 0, 1)?, deltas, 3),
        Err(CodecError::overflowing_varint(1))
    );
    // Exactly 15 bytes at the end of the deltas, the last still promising a 16th group.
    let mut at_end = vec![0x01];
    at_end.extend_from_slice(&[0x80; 15]);
    assert_eq!(
        UidBlock::new(id(1, 0, 1)?, at_end, 3),
        Err(CodecError::overflowing_varint(1))
    );
    Ok(())
}

#[test]
fn test_a_delta_with_a_redundant_group_is_refused() -> TestResult {
    assert_eq!(
        UidBlock::new(id(1, 0, 1)?, vec![0x81, 0x80, 0x00], 2),
        Err(CodecError::non_canonical_varint(0))
    );
    Ok(())
}

#[test]
fn test_a_zero_delta_is_out_of_order() -> TestResult {
    assert_eq!(
        UidBlock::new(id(1, 0, 1)?, vec![0x01, 0x00], 3),
        Err(CodecError::out_of_order(2))
    );
    Ok(())
}

#[test]
fn test_a_delta_carrying_an_id_past_104_bits_is_refused() -> TestResult {
    // The largest ID's first 104 bits are all ones: any delta passes 2^104.
    let largest = id(u16::MAX, Bucket::MAX.get(), u64::MAX)?;
    assert_eq!(
        UidBlock::new(largest, vec![0x01], 2),
        Err(CodecError::delta_overflow(1))
    );
    // 14 bytes of 0xFF and a 15th of 0x7F hold 2^105 - 1, past 104 bits whatever the first ID.
    let mut wide = vec![0xFFu8; 14];
    wide.push(0x7F);
    assert_eq!(
        UidBlock::new(id(1, 0, 1)?, wide, 2),
        Err(CodecError::delta_overflow(1))
    );
    Ok(())
}

#[test]
fn test_a_delta_reaching_an_invalid_id_is_refused() -> TestResult {
    // (type 1, largest sequence, largest bucket) plus 1 in its first 104 bits carries into (type 2, sequence 0,
    // bucket 0), whose sequence is 0.
    let last_of_type_1 = id(1, Bucket::MAX.get(), u64::MAX)?;
    assert_eq!(
        UidBlock::new(last_of_type_1, vec![0x01], 2),
        Err(CodecError::invalid_id(1, IdError::zero_sequence()))
    );
    // Plus 2: (type 2, sequence 0, bucket 1), still sequence 0; plus 2^24 + 1: (type 2, sequence 1, bucket 0), valid.
    assert_eq!(
        UidBlock::new(last_of_type_1, vec![0x02], 2),
        Err(CodecError::invalid_id(1, IdError::zero_sequence()))
    );
    let block = UidBlock::new(last_of_type_1, vec![0x81, 0x80, 0x80, 0x08], 2)?;
    assert_eq!((block.first(), block.count()), (last_of_type_1, 2));
    Ok(())
}
