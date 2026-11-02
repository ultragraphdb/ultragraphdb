/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, codec/codec_test.go (TestSeek, TestLinearSeek, TestDecoder). Changed for ultragraphdb:
 * translated to Rust over system IDs; the tests of a block holding duplicate UIDs are gone with lists that hold none,
 * and the golden-vector tests with Dgraph's format.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_codec::{Decoder, SeekPos, UidBlock, UidPack, decode};
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::codec_tests::{id, packed, sequences, target};

/// Sequences 10 to 10,000 in steps of 10, in blocks of 10: Go's TestSeek list without its 0, which no ID takes.
fn stepped() -> TestResult<UidPack> {
    packed(&sequences((10..=10_000).step_by(10))?, 10)
}

#[test]
fn test_seek() -> TestResult {
    let pack = stepped()?;
    let mut dec = Decoder::new(&pack);
    // (target, whence, the first ID the seek returns or None). Expected by hand from the list: Start returns the
    // first ID at or after the target, Current the first after it.
    let cases = [
        (0, SeekPos::Start, Some(10)),
        (0, SeekPos::Current, Some(10)),
        (target(100)?, SeekPos::Start, Some(100)),
        (target(100)?, SeekPos::Current, Some(110)),
        (target(1000)?, SeekPos::Start, Some(1000)),
        (target(1000)?, SeekPos::Current, Some(1010)),
        (target(1999)?, SeekPos::Start, Some(2000)),
        (target(1999)?, SeekPos::Current, Some(2000)),
        (target(1101)?, SeekPos::Start, Some(1110)),
        (target(10_000)?, SeekPos::Start, Some(10_000)),
        (target(9999)?, SeekPos::Current, Some(10_000)),
        (target(10_001)?, SeekPos::Start, None),
        (target(10_000)?, SeekPos::Current, None),
        (u128::MAX, SeekPos::Start, None),
        (u128::MAX, SeekPos::Current, None),
    ];
    for (t, whence, first) in cases {
        let ids = dec.seek(t, whence);
        let want = first.map(|s| id(1, 0, s)).transpose()?;
        assert_eq!(ids.first().copied(), want, "seek {t:#x} {whence:?}");
    }
    Ok(())
}

#[test]
fn test_a_target_that_is_no_valid_id_seeks_by_its_value() -> TestResult {
    let pack = stepped()?;
    let mut dec = Decoder::new(&pack);
    // One past an ID's value sets a reserved bit: no ID, between that ID and the next.
    assert_eq!(
        dec.seek(target(100)? + 1, SeekPos::Start).first(),
        Some(&id(1, 0, 110)?)
    );
    // One below an ID's value: the ID itself is the first at or after it.
    assert_eq!(
        dec.seek(target(100)? - 1, SeekPos::Start).first(),
        Some(&id(1, 0, 100)?)
    );
    // A value of type 0, below every ID.
    assert_eq!(
        dec.seek(1 << 100, SeekPos::Current).first(),
        Some(&id(1, 0, 10)?)
    );
    Ok(())
}

#[test]
fn test_linear_seek() -> TestResult {
    // Go's list: the numbers 0 to 10,000 in steps of 10, 1,001 of them, standing for sequences 1 to 10,001; in blocks
    // of 10 the last block holds the last ID alone.
    let pack = packed(&sequences((1..=10_001).step_by(10))?, 10)?;
    let mut dec = Decoder::new(&pack);
    for sequence in (1..20_002).step_by(10) {
        let ids = dec.linear_seek(target(sequence)?);
        assert_eq!(
            ids.contains(&id(1, 0, sequence)?),
            sequence <= 10_001,
            "linear seek {sequence}"
        );
    }
    // The decoder is on the last block now and does not move back.
    for sequence in (1..9991).step_by(10) {
        assert!(
            !dec.linear_seek(target(sequence)?)
                .contains(&id(1, 0, sequence)?)
        );
    }
    Ok(())
}

#[test]
fn test_decoder() -> TestResult {
    let expected = sequences((3..10_001).step_by(3))?;
    let pack = packed(&expected, 10)?;
    let mut dec = Decoder::new(&pack);
    for (i, want) in expected.iter().enumerate() {
        let sequence = want.sequence().get();
        assert_eq!(
            dec.seek(target(sequence)?, SeekPos::Start).first(),
            Some(want)
        );
        assert_eq!(
            dec.seek(target(sequence - 1)?, SeekPos::Start).first(),
            Some(want)
        );
        assert_eq!(
            dec.seek(target(sequence - 2)?, SeekPos::Start).first(),
            Some(want)
        );
        if i % 97 == 0 {
            assert_eq!(
                decode(&pack, want.to_u128()),
                expected.get(i..).context("a suffix")?
            );
        }
    }
    Ok(())
}

#[test]
fn test_a_new_decoder_holds_the_first_block() -> TestResult {
    let ids = sequences(1..=25)?;
    let pack = packed(&ids, 10)?;
    let mut dec = Decoder::new(&pack);
    assert_eq!(0, dec.block_idx());
    assert_eq!(ids.get(..10).context("block 0")?, dec.ids());
    assert_eq!(30, dec.approx_len());
    assert_eq!(Some(id(1, 0, 11)?), dec.peek_next_first());
    assert!(dec.valid());
    assert_eq!(ids.get(10..20).context("block 1")?, dec.next_block());
    assert_eq!(ids.get(20..).context("block 2")?, dec.next_block());
    assert_eq!(None, dec.peek_next_first());
    assert!(dec.valid());
    assert!(dec.next_block().is_empty());
    assert!(!dec.valid());
    assert_eq!(0, dec.approx_len());
    assert!(dec.unpack_block().is_empty());
    assert_eq!(&pack, dec.pack());
    Ok(())
}

#[test]
fn test_unpack_block_returns_the_whole_block_after_a_seek_into_it() -> TestResult {
    let ids = sequences(1..=25)?;
    let pack = packed(&ids, 10)?;
    let mut dec = Decoder::sought(&pack, target(15)?, SeekPos::Start);
    assert_eq!(1, dec.block_idx());
    assert_eq!(ids.get(14..20).context("from 15")?, dec.ids());
    assert_eq!(ids.get(10..20).context("block 1")?, dec.unpack_block());
    Ok(())
}

#[test]
fn test_seek_to_block_past_the_last_block_returns_nothing() -> TestResult {
    // A decoder past the last block searches from the last block and finds no ID after the target, where Dgraph's
    // SeekToBlock indexes the block it does not have and panics (ledger 3).
    let pack = packed(&sequences(1..=3)?, 2)?;
    let mut dec = Decoder::sought(&pack, target(4)?, SeekPos::Start);
    assert!(dec.ids().is_empty());
    assert!(dec.seek_to_block(target(5)?, SeekPos::Current).is_empty());
    Ok(())
}

#[test]
fn test_seek_to_block_returns_the_target_block_whole() -> TestResult {
    let ids = sequences(1..=40)?;
    let pack = packed(&ids, 10)?;
    let mut dec = Decoder::new(&pack);
    // A target inside a block, at a block's first ID, and at a block's last ID with Current: the block holding it.
    assert_eq!(
        dec.seek_to_block(target(15)?, SeekPos::Start),
        ids.get(10..20).context("b1")?
    );
    assert_eq!(
        dec.seek_to_block(target(21)?, SeekPos::Start),
        ids.get(20..30).context("b2")?
    );
    assert_eq!(dec.block_idx(), 2);
    assert_eq!(
        dec.seek_to_block(target(30)?, SeekPos::Current),
        ids.get(20..30).context("b2")?
    );
    // Past a block's last ID, the next block, whole.
    assert_eq!(
        dec.seek_to_block(target(30)? + 1, SeekPos::Start),
        ids.get(30..).context("b3")?
    );
    assert_eq!(
        dec.seek_to_block(0, SeekPos::Start),
        ids.get(..10).context("b0")?
    );
    Ok(())
}

#[test]
fn test_seeks_step_over_an_empty_block() -> TestResult {
    let blocks = [
        packed(&sequences(1..=2)?, 2)?.blocks().to_vec(),
        vec![UidBlock::new(id(1, 0, 3)?, Vec::new(), 0)?],
        packed(&sequences(4..=5)?, 2)?.blocks().to_vec(),
    ]
    .concat();
    let pack = UidPack::new(2, blocks)?;
    assert_eq!(decode(&pack, 0), sequences([1, 2, 4, 5].into_iter())?);
    let mut dec = Decoder::new(&pack);
    assert_eq!(dec.seek(target(3)?, SeekPos::Start), sequences(4..=5)?);
    // A linear seek lands on the empty block, whose first ID is the target, and moves on from it.
    let mut linear = Decoder::new(&pack);
    assert_eq!(linear.linear_seek(target(3)?), sequences(4..=5)?);
    assert_eq!(linear.block_idx(), 2);
    let mut to_block = Decoder::new(&pack);
    assert_eq!(
        to_block.seek_to_block(target(3)?, SeekPos::Start),
        sequences(4..=5)?
    );
    Ok(())
}
