/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The codec's free functions, and the helpers the codec's tests share: IDs, and the golden ID lists of
//! `testdata/golden/id_lists.json`, written by `formats.py` from the format's specification, independent of the codec.

use serde::Deserialize;
use ultragraphdb_core_codec::{
    CodecError, CodecErrorEnum, UidPack, approx_len, decode, decode_to_buffer, encode,
    encode_from_buffer, exact_len,
};
use ultragraphdb_core_ids::{Bucket, IdError, Sequence, SystemId, TypePrefix};
use ultragraphdb_tool_bdd::{Context, TestResult, load_golden, testdata_dir};

pub fn id(prefix: u16, bucket: u32, sequence: u64) -> TestResult<SystemId> {
    Ok(SystemId::new(
        TypePrefix::new(prefix)?,
        Bucket::new(bucket)?,
        Sequence::new(sequence)?,
    ))
}

/// The IDs of type 1, bucket 0 and the sequences given.
pub fn sequences(sequences: impl Iterator<Item = u64>) -> TestResult<Vec<SystemId>> {
    sequences.map(|s| id(1, 0, s)).collect()
}

/// The 128-bit value of the ID of type 1, bucket 0 and `sequence`, as a target.
pub fn target(sequence: u64) -> TestResult<u128> {
    Ok(id(1, 0, sequence)?.to_u128())
}

pub fn packed(ids: &[SystemId], block_size: usize) -> TestResult<UidPack> {
    encode(ids, block_size)?.context("a non-empty list packs")
}

pub fn unhex(text: &str) -> TestResult<Vec<u8>> {
    (0..text.len())
        .step_by(2)
        .map(|i| {
            text.get(i..i + 2)
                .context("two hex digits")
                .and_then(|pair| u8::from_str_radix(pair, 16).context("hex"))
        })
        .collect()
}

/// A golden block: its first ID and deltas in hex, and its ID count.
#[derive(Debug, Clone, Deserialize)]
pub struct GoldenBlock {
    pub first: String,
    pub count: u32,
    pub deltas: String,
}

/// A golden case: a list and the pack's bytes and blocks, or the position at which encoding refuses it.
#[derive(Debug, Clone, Deserialize)]
pub struct IdListCase {
    pub name: String,
    pub block_size: usize,
    pub ids: Vec<String>,
    #[serde(default)]
    pub pack: Option<String>,
    #[serde(default)]
    pub blocks: Vec<GoldenBlock>,
    #[serde(default)]
    pub out_of_order: Option<usize>,
}

impl IdListCase {
    pub fn ids(&self) -> TestResult<Vec<SystemId>> {
        self.ids.iter().map(|text| parse_id(text)).collect()
    }
}

pub fn parse_id(text: &str) -> TestResult<SystemId> {
    Ok(SystemId::from_u128(
        u128::from_str_radix(text, 16).context("a hex ID")?,
    )?)
}

pub fn id_list_cases() -> TestResult<Vec<IdListCase>> {
    let golden = load_golden::<IdListCase>(&testdata_dir()?.join("golden/id_lists.json"))?;
    assert!(
        golden.cases().len() >= 20,
        "the golden file holds the cases"
    );
    Ok(golden.cases().to_vec())
}

#[test]
fn test_an_empty_list_packs_to_nothing() -> TestResult {
    assert_eq!(encode(&[], 256), Ok(None));
    assert_eq!(0, approx_len(&UidPack::default()));
    assert_eq!(0, exact_len(&UidPack::default()));
    assert!(decode(&UidPack::default(), 0).is_empty());
    Ok(())
}

#[test]
fn test_every_golden_list_decodes_from_every_target_to_its_suffix() -> TestResult {
    for case in id_list_cases()?
        .into_iter()
        .filter(|c| c.out_of_order.is_none())
    {
        let ids = case.ids()?;
        let Some(pack) = encode(&ids, case.block_size)? else {
            assert!(ids.is_empty(), "{}", case.name);
            continue;
        };
        assert_eq!(ids.len(), exact_len(&pack), "{}", case.name);
        assert_eq!(decode(&pack, 0), ids, "{}", case.name);
        // From each ID, and from one past each ID's value, which no valid ID takes: its reserved bits are set.
        for (i, from) in ids.iter().enumerate().step_by(7) {
            let suffix = ids.get(i..).context("a suffix")?;
            assert_eq!(decode(&pack, from.to_u128()), suffix, "{}", case.name);
            let after = ids.get(i + 1..).context("a suffix")?;
            assert_eq!(decode(&pack, from.to_u128() + 1), after, "{}", case.name);
        }
        assert!(decode(&pack, u128::MAX).is_empty());
    }
    Ok(())
}

#[test]
fn test_approx_len_is_the_block_count_times_the_block_size() -> TestResult {
    // Eleven IDs in blocks of 4: three blocks, the last holding three, so 3 * 4 = 12.
    let pack = packed(&sequences(1..=11)?, 4)?;
    assert_eq!(pack.blocks().len(), 3);
    assert_eq!(approx_len(&pack), 12);
    assert_eq!(exact_len(&pack), 11);
    Ok(())
}

#[test]
fn test_buffer_deltas_are_leb128_of_the_ids_values() -> TestResult {
    // (type 1, bucket 0, sequence 1) is 2^112 + 2^48: from 0 its delta is that value, whose LEB128 groups are zero but
    // for bit 48 (group 6, bit 6: 0x40) and bit 112 (group 16, bit 0: 0x01), 17 bytes. The next ID, sequence 2, is
    // 2^48 above it: groups 0 to 5 zero and 0x40 in group 6, 7 bytes. By hand from LEB128's definition.
    let ids = sequences(1..=2)?;
    let mut want = vec![0x80u8; 6];
    want.push(0xC0);
    want.extend_from_slice(&[0x80; 9]);
    want.push(0x01);
    want.extend_from_slice(&[0x80; 6]);
    want.push(0x40);
    let mut buf = Vec::new();
    decode_to_buffer(&mut buf, &packed(&ids, 256)?);
    assert_eq!(buf, want);
    let read = encode_from_buffer(&want, 256)?.context("two IDs")?;
    assert_eq!(decode(&read, 0), ids);
    let mut empty = Vec::new();
    decode_to_buffer(&mut empty, &UidPack::default());
    assert!(empty.is_empty());
    assert_eq!(encode_from_buffer(&[], 256), Ok(None));
    Ok(())
}

#[test]
fn test_every_golden_list_round_trips_through_a_buffer() -> TestResult {
    for case in id_list_cases()?
        .into_iter()
        .filter(|c| c.out_of_order.is_none() && !c.ids.is_empty())
    {
        let pack = packed(&case.ids()?, case.block_size)?;
        let mut buf = Vec::new();
        decode_to_buffer(&mut buf, &pack);
        let read = encode_from_buffer(&buf, case.block_size)?.context("a list")?;
        assert_eq!(read, pack, "{}", case.name);
    }
    Ok(())
}

#[test]
fn test_a_truncated_buffer_varint_is_refused() {
    // One ID's 17 bytes cut after 16: the last byte still carries the continuation bit.
    let mut buf = vec![0x80u8; 6];
    buf.push(0xC0);
    buf.extend_from_slice(&[0x80; 9]);
    assert_eq!(
        encode_from_buffer(&buf, 256),
        Err(CodecError::truncated_varint(0))
    );
}

#[test]
fn test_a_buffer_varint_past_128_bits_is_refused() {
    // 19 groups hold 133 bits: a 19th byte above 0x03 sets a bit past 127, and a 20th byte is past any u128.
    let mut over = vec![0xFFu8; 18];
    over.push(0x04);
    assert_eq!(
        encode_from_buffer(&over, 256),
        Err(CodecError::overflowing_varint(0))
    );
    assert_eq!(
        encode_from_buffer(&[0x80; 20], 256),
        Err(CodecError::overflowing_varint(0))
    );
    // Exactly 19 bytes, the last still promising a 20th group: past any u128 wherever the buffer ends.
    assert_eq!(
        encode_from_buffer(&[0x80; 19], 256),
        Err(CodecError::overflowing_varint(0))
    );
    // Two deltas whose sum passes 2^128: the valid ID 2^127 + 2^48 (type 0x8000, sequence 1; 19 bytes: groups 0 to 5
    // zero, 0x40 in group 6, zero to group 17, 0x02 in group 18), then 2^127 (18 zero groups, then 0x02), which starts
    // at byte 19.
    let mut first = vec![0x80u8; 6];
    first.push(0xC0);
    first.extend_from_slice(&[0x80; 11]);
    first.push(0x02);
    let mut half = vec![0x80u8; 18];
    half.push(0x02);
    let twice = [first, half].concat();
    assert_eq!(
        encode_from_buffer(&twice, 256),
        Err(CodecError::overflowing_varint(19))
    );
}

#[test]
fn test_a_buffer_varint_with_a_redundant_group_is_refused() {
    // 1 written as 0x81 0x00, two bytes where 0x01 is the shortest.
    assert_eq!(
        encode_from_buffer(&[0x81, 0x00], 256),
        Err(CodecError::non_canonical_varint(0))
    );
}

#[test]
fn test_a_buffer_value_that_is_no_valid_id_is_refused() -> TestResult {
    // The value 1: its reserved bits are set.
    assert_eq!(
        encode_from_buffer(&[0x01], 256),
        Err(CodecError::invalid_id(0, IdError::reserved_bits(1)))
    );
    // A valid first ID, then a delta of 0: the second ID equals the first.
    let mut buf = Vec::new();
    decode_to_buffer(&mut buf, &packed(&sequences(1..=1)?, 256)?);
    buf.push(0x00);
    assert_eq!(
        encode_from_buffer(&buf, 256).map_err(|e| e.0),
        Err(CodecErrorEnum::OutOfOrder { position: 1 })
    );
    Ok(())
}
