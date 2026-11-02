/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_codec::{CodecError, Encoder, decode, encode};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::codec_tests::{id, id_list_cases, parse_id, sequences, unhex};

#[test]
fn test_encoding_matches_the_golden_blocks() -> TestResult {
    let mut checked = 0;
    for case in id_list_cases()?
        .into_iter()
        .filter(|c| c.out_of_order.is_none())
    {
        let Some(pack) = encode(&case.ids()?, case.block_size)? else {
            assert!(
                case.blocks.is_empty() && case.pack.is_none(),
                "{}",
                case.name
            );
            continue;
        };
        assert_eq!(pack.blocks().len(), case.blocks.len(), "{}", case.name);
        for (i, (got, want)) in pack.blocks().iter().zip(&case.blocks).enumerate() {
            assert_eq!(
                got.first(),
                parse_id(&want.first)?,
                "{} block {i}",
                case.name
            );
            assert_eq!(got.count(), want.count, "{} block {i}", case.name);
            assert_eq!(
                got.deltas(),
                unhex(&want.deltas)?,
                "{} block {i}",
                case.name
            );
        }
        checked += 1;
    }
    assert!(checked >= 15, "{checked} golden lists checked");
    Ok(())
}

#[test]
fn test_a_list_out_of_order_is_refused_at_its_first_position_out_of_order() -> TestResult {
    let mut checked = 0;
    for case in id_list_cases()? {
        let Some(position) = case.out_of_order else {
            continue;
        };
        assert_eq!(
            encode(&case.ids()?, case.block_size),
            Err(CodecError::out_of_order(position)),
            "{}",
            case.name
        );
        checked += 1;
    }
    assert_eq!(checked, 4);
    Ok(())
}

#[test]
fn test_a_refused_id_leaves_the_encoder_as_it_was() -> TestResult {
    let mut encoder = Encoder::new(2);
    encoder.add(id(1, 0, 5)?)?;
    encoder.add(id(1, 0, 6)?)?;
    assert_eq!(encoder.add(id(1, 0, 6)?), Err(CodecError::out_of_order(2)));
    assert_eq!(encoder.add(id(1, 0, 1)?), Err(CodecError::out_of_order(2)));
    encoder.add(id(1, 0, 7)?)?;
    let pack = encoder.done().context("three IDs")?;
    assert_eq!(decode(&pack, 0), sequences(5..=7)?);
    Ok(())
}

#[test]
fn test_block_size_zero_packs_one_id_per_block() -> TestResult {
    let mut encoder = Encoder::new(0);
    assert_eq!(0, encoder.block_size());
    for sequence in [3, 4, 9] {
        encoder.add(id(1, 0, sequence)?)?;
    }
    let pack = encoder.done().context("the list packs")?;
    assert_eq!(0, pack.block_size());
    assert_eq!(
        pack.blocks()
            .iter()
            .map(|b| (b.first(), b.count()))
            .collect::<Vec<_>>(),
        vec![(id(1, 0, 3)?, 1), (id(1, 0, 4)?, 1), (id(1, 0, 9)?, 1)]
    );
    Ok(())
}

#[test]
fn test_an_encoder_without_ids_returns_no_pack() {
    assert!(Encoder::new(256).done().is_none());
}

#[test]
fn test_an_encoder_reports_the_block_size_it_was_made_with() -> TestResult {
    // 256, the block size the posting lists use, and the pack's.
    let mut encoder = Encoder::new(256);
    assert_eq!(256, encoder.block_size());
    encoder.add(id(1, 0, 1)?)?;
    assert_eq!(256, encoder.done().context("a pack")?.block_size());
    Ok(())
}

#[test]
fn test_a_one_id_block_holds_no_delta() -> TestResult {
    let pack = encode(&[id(3, 0xAB_CDEF, 42)?], 256)?.context("one ID packs")?;
    let [block] = pack.blocks() else {
        return Err(TestError::new("one block"));
    };
    assert_eq!((block.first(), block.count()), (id(3, 0xAB_CDEF, 42)?, 1));
    assert!(block.deltas().is_empty());
    Ok(())
}

#[test]
fn test_a_delta_counts_the_bucket_and_sequence_below_the_reserved_bits() -> TestResult {
    // (1, bucket 7, sequence 5) to (1, bucket 8, sequence 5): the first 104 bits differ by 1, one byte 0x01. To
    // (1, bucket 0, sequence 6): by 2^24 - 8, 0xFFFFF8, whose LEB128 is F8 FF FF 07. By hand.
    let pack = encode(&[id(1, 7, 5)?, id(1, 8, 5)?, id(1, 0, 6)?], 256)?.context("a pack")?;
    let [block] = pack.blocks() else {
        return Err(TestError::new("one block"));
    };
    assert_eq!(block.deltas(), [0x01, 0xF8, 0xFF, 0xFF, 0x07]);
    Ok(())
}
