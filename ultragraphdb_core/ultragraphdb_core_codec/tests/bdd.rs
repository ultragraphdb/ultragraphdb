/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The codec's scenarios, bound to `features/`: packing ID lists, seeking through packs, and storing posting lists.

use cucumber::{World, then, when};
use ultragraphdb_core_codec::{
    CodecError, CodecErrorEnum, Decoder, Facet, Posting, PostingList, PostingType, SeekPos,
    UidBlock, UidPack, decode, encode,
};
use ultragraphdb_core_ids::{Bucket, Sequence, SystemId, TypePrefix};
use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

#[derive(Debug, Default, World)]
struct Steps {
    ids: Vec<SystemId>,
    pack: Option<UidPack>,
    refused: Option<CodecError>,
    round_trips: Vec<(Vec<SystemId>, Vec<SystemId>)>,
    decoded: Vec<SystemId>,
    sought: Vec<SystemId>,
    plist: PostingList,
    reads: Vec<Result<PostingList, StoreError>>,
}

const KEY: &[u8] = b"\x10list-7";

fn id(prefix: u16, bucket: u32, sequence: u64) -> TestResult<SystemId> {
    Ok(SystemId::new(
        TypePrefix::new(prefix)?,
        Bucket::new(bucket)?,
        Sequence::new(sequence)?,
    ))
}

/// The IDs of type 1, bucket 0 and the sequences given.
fn sequences(sequences: impl Iterator<Item = u64>) -> TestResult<Vec<SystemId>> {
    sequences.map(|s| id(1, 0, s)).collect()
}

fn target(sequence: u64) -> TestResult<u128> {
    Ok(id(1, 0, sequence)?.to_u128())
}

fn packed(ids: &[SystemId], block_size: usize) -> TestResult<UidPack> {
    encode(ids, block_size)?.context("a non-empty list packs")
}

#[when("300 increasing IDs are encoded with block size 256")]
fn encode_300(w: &mut Steps) -> TestResult {
    w.ids = sequences(1..=300)?;
    w.pack = Some(packed(&w.ids, 256)?);
    Ok(())
}

#[then("the encoding holds two blocks, of 256 and 44 IDs, each beginning with its first ID whole")]
fn two_blocks(w: &mut Steps) -> TestResult {
    let pack = w.pack.as_ref().context("the pack")?;
    let [a, b] = pack.blocks() else {
        return Err(TestError::new(format!(
            "two blocks, not {}",
            pack.blocks().len()
        )));
    };
    assert_eq!((a.count(), b.count()), (256, 44));
    assert_eq!((a.first(), b.first()), (id(1, 0, 1)?, id(1, 0, 257)?));
    // The pack's bytes hold each first ID whole: the 16 bytes of (type 1, bucket 0, sequence 257).
    let bytes = pack.encode();
    let whole = id(1, 0, 257)?.to_be_bytes();
    assert!(bytes.windows(16).any(|window| window == whole));
    Ok(())
}

#[when("the IDs (type 1, bucket 0, sequence 1) and (type 1, bucket 0, sequence 129) are encoded")]
fn encode_two(w: &mut Steps) -> TestResult {
    w.pack = Some(packed(&[id(1, 0, 1)?, id(1, 0, 129)?], 256)?);
    Ok(())
}

#[then(
    "the second is written as the difference of their first 104 bits, 128 × 2^24 since the sequence sits above the 24-bit bucket, in LEB128 the bytes `80 80 80 80 08`"
)]
fn leb128_delta(w: &mut Steps) -> TestResult {
    let pack = w.pack.as_ref().context("the pack")?;
    let [block] = pack.blocks() else {
        return Err(TestError::new("one block"));
    };
    // 128 × 2^24 = 2^31: four groups of seven zero bits, each with the continuation bit, then 2^31 >> 28 = 8.
    assert_eq!(block.deltas(), [0x80, 0x80, 0x80, 0x80, 0x08]);
    assert_eq!(block.count(), 2);
    Ok(())
}

#[when("a list whose third ID is not greater than its second is encoded")]
fn encode_out_of_order(w: &mut Steps) -> TestResult {
    let ids = [id(1, 0, 1)?, id(1, 0, 5)?, id(1, 0, 5)?, id(1, 0, 9)?];
    w.refused = encode(&ids, 256).err();
    Ok(())
}

#[then("encoding fails with an error naming position 2")]
fn names_position_2(w: &mut Steps) -> TestResult {
    let error = w.refused.take().context("refused")?;
    assert_eq!(error.0, CodecErrorEnum::OutOfOrder { position: 2 });
    assert!(error.to_string().contains("position 2"), "{error}");
    Ok(())
}

/// 400 IDs of types 1 and 2 and buckets 0 and 7, sorted: the pool the round-trip lists are taken from.
fn pool() -> TestResult<Vec<SystemId>> {
    let mut ids = Vec::with_capacity(400);
    for prefix in [1u16, 2] {
        for sequence in 1..=100u64 {
            for bucket in [0u32, 7] {
                ids.push(id(prefix, bucket, sequence * 3)?);
            }
        }
    }
    ids.sort();
    Ok(ids)
}

#[when(
    "each sorted list of length 0 to 100, including lists spanning two types and two buckets, is encoded and decoded"
)]
fn every_length(w: &mut Steps) -> TestResult {
    let pool = pool()?;
    for len in 0..=100usize {
        // Every third ID of the pool, alternating buckets, so the list spans both types once it holds more than 67.
        let list: Vec<SystemId> = pool.iter().step_by(3).take(len).copied().collect();
        for block_size in [10, 256] {
            let decoded = encode(&list, block_size)?.map_or_else(Vec::new, |p| decode(&p, 0));
            w.round_trips.push((list.clone(), decoded));
        }
    }
    Ok(())
}

#[then("the decoded list equals the input")]
fn round_trip_equal(w: &mut Steps) -> TestResult {
    assert_eq!(w.round_trips.len(), 202);
    let spanning = w.round_trips.iter().any(|(list, _)| {
        let types: std::collections::BTreeSet<_> = list.iter().map(|i| i.type_prefix()).collect();
        let buckets: std::collections::BTreeSet<_> = list.iter().map(|i| i.bucket()).collect();
        types.len() == 2 && buckets.len() == 2
    });
    assert!(spanning, "some lists span two types and two buckets");
    for (list, decoded) in &w.round_trips {
        assert_eq!(list, decoded);
    }
    Ok(())
}

#[when(
    "a packed list of the 1,000 IDs of type 1, bucket 0 and sequences 10, 20, …, 10,000 is decoded from sequence 5,005 and a decoder is sought to sequence 5,005"
)]
fn decode_and_seek(w: &mut Steps) -> TestResult {
    let pack = packed(&sequences((10..=10_000).step_by(10))?, 256)?;
    w.decoded = decode(&pack, target(5005)?);
    w.sought = Decoder::sought(&pack, target(5005)?, SeekPos::Start)
        .ids()
        .to_vec();
    Ok(())
}

#[then(
    "decoding returns the 500 IDs from sequence 5,010 on, and the seek returns the rest of the target's block from sequence 5,010"
)]
fn decoded_and_sought(w: &mut Steps) -> TestResult {
    assert_eq!(w.decoded, sequences((5010..=10_000).step_by(10))?);
    assert_eq!(w.decoded.len(), 500);
    // Blocks of 256 from sequence 10: the second block holds sequences 2,570 to 5,120.
    assert_eq!(w.sought, sequences((5010..=5120).step_by(10))?);
    Ok(())
}

#[when(
    "a decoder over sequences 1 to 40 of type 1, bucket 0, in blocks of 10, seeks to sequence 35 and then to 5"
)]
fn backward_seek(w: &mut Steps) -> TestResult {
    let pack = packed(&sequences(1..=40)?, 10)?;
    let mut decoder = Decoder::new(&pack);
    let first: Vec<SystemId> = decoder
        .seek_to_block(target(35)?, SeekPos::Current)
        .to_vec();
    assert_eq!(first, sequences(31..=40)?);
    w.sought = decoder.seek_to_block(target(5)?, SeekPos::Current).to_vec();
    Ok(())
}

#[then("the second seek returns the block of sequences 1 to 10")]
fn backward_block(w: &mut Steps) -> TestResult {
    assert_eq!(w.sought, sequences(1..=10)?);
    Ok(())
}

#[when("a pack holds a block whose ID count is 0 and a seek targets it")]
fn empty_block(w: &mut Steps) -> TestResult {
    let first = packed(&sequences(1..=10)?, 10)?;
    let last = packed(&sequences(21..=30)?, 10)?;
    let blocks = [
        first.blocks().to_vec(),
        vec![UidBlock::new(id(1, 0, 11)?, Vec::new(), 0)?],
        last.blocks().to_vec(),
    ]
    .concat();
    let pack = UidPack::new(10, blocks)?;
    let to_block = std::panic::catch_unwind(|| {
        Decoder::new(&pack)
            .seek_to_block(id(1, 0, 15).map_or(0, SystemId::to_u128), SeekPos::Start)
            .to_vec()
    })
    .map_err(|_| TestError::new("a seek never panics"))?;
    let sought = std::panic::catch_unwind(|| {
        Decoder::sought(
            &pack,
            id(1, 0, 15).map_or(0, SystemId::to_u128),
            SeekPos::Start,
        )
        .ids()
        .to_vec()
    })
    .map_err(|_| TestError::new("a seek never panics"))?;
    w.decoded = to_block;
    w.sought = sought;
    Ok(())
}

#[then("the seek moves on to the next block and nothing panics")]
fn moves_on(w: &mut Steps) -> TestResult {
    assert_eq!(w.decoded, sequences(21..=30)?);
    assert_eq!(w.sought, sequences(21..=30)?);
    Ok(())
}

#[when("a decoder over sequences 1 to 3 of type 1, bucket 0, in blocks of 2, seeks to `u128::MAX`")]
fn seek_largest(w: &mut Steps) -> TestResult {
    let pack = packed(&sequences(1..=3)?, 2)?;
    let mut decoder = Decoder::new(&pack);
    w.sought = decoder.seek(u128::MAX, SeekPos::Start).to_vec();
    w.decoded = decoder.seek_to_block(u128::MAX, SeekPos::Start).to_vec();
    Ok(())
}

#[then("the seek returns nothing and nothing panics")]
fn returns_nothing(w: &mut Steps) {
    assert!(w.sought.is_empty(), "{:?}", w.sought);
    assert!(w.decoded.is_empty(), "{:?}", w.decoded);
}

#[when(
    "a decoder over sequences 1 to 3 of type 1, bucket 0, in blocks of 2, is linearly sought to `u128::MAX`"
)]
fn linear_seek_largest(w: &mut Steps) -> TestResult {
    let pack = packed(&sequences(1..=3)?, 2)?;
    w.sought = Decoder::new(&pack).linear_seek(u128::MAX).to_vec();
    Ok(())
}

#[then("it returns the block holding sequence 3")]
fn last_block(w: &mut Steps) -> TestResult {
    assert_eq!(w.sought, sequences(3..=3)?);
    Ok(())
}

/// A list with every part the format holds: a pack, a reference with a facet, value postings with statement IDs, and
/// splits.
fn full_list() -> TestResult<PostingList> {
    Ok(PostingList::default()
        .with_commit_ts(42)
        .with_pack(packed(&sequences([3, 9, 1 << 40].into_iter())?, 2)?)
        .with_postings(vec![
            Posting::default()
                .with_uid(id(1, 0, 3)?.to_u128())
                .with_facets(vec![Facet::new("since")]),
            Posting::default()
                .with_uid(0x0123_4567_89AB_CDEF)
                .with_posting_type(PostingType::Value)
                .with_value(b"\x01Ann".to_vec())
                .with_statement_ids(vec![id(2, 0, 7)?, id(2, 0, 8)?]),
            Posting::default()
                .with_uid(0xFEDC_BA98_7654_3210)
                .with_posting_type(PostingType::Value)
                .with_value(b"\x01Bob".to_vec())
                .with_statement_ids(vec![id(2, 9, 1)?]),
        ])
        .with_splits(vec![1, id(1, 0, 9)?.to_u128()]))
}

#[when(
    "a list with a packed ID list, value postings with statement IDs and splits is encoded and decoded"
)]
fn encode_and_decode(w: &mut Steps) -> TestResult {
    w.plist = full_list()?;
    w.reads.push(PostingList::decode(KEY, &w.plist.encode()));
    Ok(())
}

#[then("the decoded list equals the original")]
fn decoded_equals(w: &mut Steps) -> TestResult {
    let read = w.reads.pop().context("a read")??;
    assert_eq!(read, w.plist);
    Ok(())
}

#[when("one byte of a stored list is flipped and the list is read")]
fn flip_each_byte(w: &mut Steps) -> TestResult {
    let stored = full_list()?.encode();
    for at in 0..stored.len() {
        let mut flipped = stored.clone();
        if let Some(byte) = flipped.get_mut(at) {
            *byte ^= 0xff;
        }
        w.reads.push(PostingList::decode(KEY, &flipped));
    }
    Ok(())
}

#[then("the read fails with a corruption error naming the key")]
fn every_read_corrupted(w: &mut Steps) {
    assert!(
        w.reads.len() > 100,
        "every byte of the list is flipped in turn"
    );
    for read in &w.reads {
        assert!(
            matches!(read, Err(StoreError(StoreErrorEnum::Corrupted { key, .. })) if key == KEY),
            "{read:?}"
        );
    }
}

/// Stage 5's encoding of the empty list: version 1, no flags, commit timestamp 0, no postings, no splits, and its
/// XXH3-64 checksum (stage 5's golden value, from python-xxhash).
const STAGE_5_EMPTY: [u8; 26] = [
    0x01, 0x00, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xd0, 0x78, 0xc3, 0xec, 0x1d, 0x4f,
    0xad, 0x97,
];

#[when("a value written with stage 5's version byte is read")]
fn read_stage_5(w: &mut Steps) {
    w.reads.push(PostingList::decode(KEY, &STAGE_5_EMPTY));
}

#[then("the read fails with a corruption error naming the key and the unknown version")]
fn unknown_version(w: &mut Steps) -> TestResult {
    let read = w.reads.pop().context("a read")?;
    let Err(StoreError(StoreErrorEnum::Corrupted { key, reason })) = &read else {
        return Err(TestError::new(format!("corruption, not {read:?}")));
    };
    assert_eq!(key, KEY);
    assert_eq!(reason, "unknown posting format version 1");
    Ok(())
}

fn main() -> TestResult {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()?),
    );
    Ok(())
}
