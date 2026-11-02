/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The posting format, version 2. The golden encodings are `testdata/golden/posting_lists.json`, written by
//! `formats.py` from the layout the format documents, independent of this crate, and checksummed with python-xxhash
//! 4.0.2 over the xxHash C library 0.8.3 (its XXH3-64 of the empty input, seed 0, is 0x2d06800538d394c2, the value of
//! xxHash's `doc/xxhash_spec.md`). A test that changes a field behind the checksum recomputes the checksum with
//! `xxhash-rust` only to reach the check it means; no expected checksum comes from it.
//!
//! The golden full list's fields start at: the pack 10 (its block count 15, its first block's first ID 19), the
//! posting count 79, the first posting 83 (its type 103, its facet count 128, its facet key's length 132 and the key
//! 136), the second 145 (its value's length 182, its statement ID count 200, its statement IDs 204 and 220), the split
//! count 236, the checksum 272; 280 bytes.

use serde::Deserialize;
use ultragraphdb_core_codec::{Facet, Posting, PostingList, PostingType, UidPack, decode};
use ultragraphdb_core_ids::Bucket;
use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_tool_bdd::{Context, TestResult, load_golden, testdata_dir};
use xxhash_rust::xxh3::xxh3_64;

use crate::codec_tests::{id, packed, sequences, unhex};

const KEY: &[u8] = b"\x10\x00\x07list";

#[derive(Debug, Clone, Deserialize)]
struct GoldenList {
    name: String,
    bytes: String,
}

fn golden_bytes(name: &str) -> TestResult<Vec<u8>> {
    let golden = load_golden::<GoldenList>(&testdata_dir()?.join("golden/posting_lists.json"))?;
    let case = golden
        .cases()
        .iter()
        .find(|c| c.name == name)
        .context("the golden list")?;
    unhex(&case.bytes)
}

/// The list the golden `full` encodes, every field distinct from the others.
fn full() -> TestResult<PostingList> {
    let mut pack_ids = sequences(1..=4)?;
    pack_ids.push(id(1, 5, 9)?);
    Ok(PostingList::default()
        .with_commit_ts(0x1122_3344_5566_7788)
        .with_pack(packed(&pack_ids, 4)?)
        .with_postings(vec![
            Posting::default()
                .with_uid(id(1, 0, 2)?.to_u128())
                .with_op(1)
                .with_start_ts(5)
                .with_commit_ts(6)
                .with_facets(vec![Facet::new("since")]),
            Posting::default()
                .with_uid(0x0123_4567_89AB_CDEF)
                .with_op(3)
                .with_posting_type(PostingType::Value)
                .with_start_ts(9)
                .with_commit_ts(10)
                .with_value(vec![0x01, b'a', b'b', b'c'])
                .with_lang_tag(b"en".to_vec())
                .with_statement_ids(vec![id(2, 0, 11)?, id(2, Bucket::MAX.get(), u64::MAX)?]),
        ])
        .with_splits(vec![1, id(1, 0, 3)?.to_u128()]))
}

fn delete_all() -> PostingList {
    PostingList::default().with_postings(vec![
        Posting::default()
            .with_uid(u128::from(u64::MAX))
            .with_op(2)
            .with_start_ts(3)
            .with_value(b"_STAR_ALL".to_vec()),
    ])
}

fn goldens() -> TestResult<Vec<(&'static str, PostingList, Vec<u8>)>> {
    Ok(vec![
        ("empty", PostingList::default(), golden_bytes("empty")?),
        (
            "empty pack",
            PostingList::default().with_pack(UidPack::default()),
            golden_bytes("empty pack")?,
        ),
        ("delete-all", delete_all(), golden_bytes("delete-all")?),
        ("full", full()?, golden_bytes("full")?),
    ])
}

/// `body` with its checksum appended, to reach a check behind it.
fn forge(body: &[u8]) -> Vec<u8> {
    let mut bytes = body.to_vec();
    bytes.extend_from_slice(&xxh3_64(body).to_le_bytes());
    bytes
}

/// The body of `bytes`: everything before the checksum.
fn body(bytes: &[u8]) -> TestResult<Vec<u8>> {
    Ok(bytes
        .get(..bytes.len().saturating_sub(8))
        .context("a body")?
        .to_vec())
}

fn corrupted(reason: &str) -> Result<PostingList, StoreError> {
    Err(StoreError::corrupted(KEY.to_vec(), reason.to_string()))
}

fn names_the_key(result: &Result<PostingList, StoreError>, key: &[u8]) -> bool {
    matches!(result, Err(StoreError(StoreErrorEnum::Corrupted { key: k, .. })) if k == key)
}

/// The full list's body with the bytes from `at` replaced by `bytes`, resealed.
fn full_with(at: usize, bytes: &[u8]) -> TestResult<Vec<u8>> {
    let mut b = body(&golden_bytes("full")?)?;
    b.get_mut(at..at + bytes.len())
        .context("the field")?
        .copy_from_slice(bytes);
    Ok(forge(&b))
}

#[test]
fn test_a_list_encodes_to_its_golden_bytes() -> TestResult {
    for (name, plist, bytes) in goldens()? {
        assert_eq!(plist.encode(), bytes, "{name}");
        assert_eq!(plist.encoded_len(), bytes.len(), "{name}");
    }
    Ok(())
}

#[test]
fn test_a_list_appended_to_a_buffer_is_its_golden_bytes_after_what_was_there() -> TestResult {
    for (name, list, golden) in goldens()? {
        let mut buffer = b"before".to_vec();
        list.encode_into(&mut buffer);
        assert_eq!(buffer.get(..6), Some(&b"before"[..]), "{name}");
        assert_eq!(
            buffer.get(6..),
            Some(golden.as_slice()),
            "{name}: the checksum covers the list alone"
        );
    }
    Ok(())
}

#[test]
fn test_golden_bytes_decode_to_their_list_and_back() -> TestResult {
    for (name, plist, bytes) in goldens()? {
        assert_eq!(PostingList::decode(KEY, &bytes), Ok(plist), "{name}");
        let decoded = PostingList::decode(KEY, &bytes).context(name)?;
        assert_eq!(decoded.encode(), bytes, "{name}");
    }
    Ok(())
}

#[test]
fn test_the_version_is_2() -> TestResult {
    assert_eq!(PostingList::VERSION, 2);
    assert_eq!(golden_bytes("empty")?.first(), Some(&2));
    Ok(())
}

#[test]
fn test_a_long_value_round_trips_and_counts() {
    let long = PostingList::default().with_postings(vec![
        Posting::default()
            .with_uid(1)
            .with_value(vec![0xab; 70_000])
            .with_lang_tag(vec![b'x'; 66_000]),
    ]);
    // 26 for the empty list, 53 for a posting's fixed fields and counts (16 + 4 + 1 + 8 + 8 + four u32 counts), and
    // the value's and language's bytes.
    assert_eq!(long.encoded_len(), 26 + 53 + 70_000 + 66_000);
    assert_eq!(long.encode().len(), long.encoded_len());
    assert_eq!(PostingList::decode(KEY, &long.encode()), Ok(long));
}

#[test]
fn test_boundary_values_round_trip() -> TestResult {
    let edge = PostingList::default()
        .with_commit_ts(u64::MAX)
        .with_pack(packed(
            &[id(u16::MAX, Bucket::MAX.get(), u64::MAX)?],
            usize::try_from(u32::MAX)?,
        )?)
        .with_postings(vec![
            Posting::default()
                .with_uid(u128::MAX)
                .with_op(u32::MAX)
                .with_start_ts(u64::MAX)
                .with_commit_ts(u64::MAX)
                .with_facets(vec![Facet::new(""), Facet::new("é")])
                .with_statement_ids(vec![
                    id(1, 0, 1)?,
                    id(u16::MAX, Bucket::MAX.get(), u64::MAX)?,
                ]),
            Posting::default(),
        ])
        .with_splits(vec![0, u128::MAX]);
    assert_eq!(PostingList::decode(KEY, &edge.encode()), Ok(edge));
    Ok(())
}

#[test]
fn test_a_list_without_a_pack_and_one_with_an_empty_pack_differ() {
    let none = PostingList::default();
    let empty = PostingList::default().with_pack(UidPack::default());
    assert_ne!(none.encode(), empty.encode());
    assert_eq!(PostingList::decode(KEY, &empty.encode()), Ok(empty));
    assert_eq!(PostingList::decode(KEY, &none.encode()), Ok(none));
}

#[test]
fn test_the_pack_reads_back_whole() -> TestResult {
    // Sequences across both buckets and types, with gaps of up to 2^30, in blocks of 256.
    let mut ids = sequences((1..600u64).map(|i| i * i * 977))?;
    ids.extend(
        (1..600u64)
            .map(|i| id(2, 7, i * 1000))
            .collect::<TestResult<Vec<_>>>()?,
    );
    let pack = packed(&ids, 256)?;
    let plist = PostingList::default().with_pack(pack.clone());
    let decoded = PostingList::decode(KEY, &plist.encode()).context("decodes")?;
    let read = decoded.pack().context("a pack")?;
    assert_eq!(read, &pack);
    assert_eq!(decode(read, 0), ids);
    Ok(())
}

#[test]
fn test_every_truncation_is_corruption_naming_the_key() -> TestResult {
    let bytes = golden_bytes("full")?;
    for len in 0..bytes.len() {
        let result = PostingList::decode(KEY, bytes.get(..len).context("a prefix")?);
        assert!(names_the_key(&result, KEY), "{len} bytes: {result:?}");
    }
    Ok(())
}

#[test]
fn test_a_value_shorter_than_an_empty_list_is_truncated() -> TestResult {
    let bytes = golden_bytes("empty")?;
    for len in [0usize, 1, 25] {
        assert_eq!(
            PostingList::decode(KEY, bytes.get(..len).context("a prefix")?),
            corrupted(&format!(
                "the value is {len} bytes, shorter than the 26 of an empty posting list"
            ))
        );
    }
    Ok(())
}

#[test]
fn test_every_bit_flip_is_corruption_naming_the_key() -> TestResult {
    let bytes = golden_bytes("full")?;
    for at in 0..bytes.len() {
        for bit in 0..8 {
            let mut flipped = bytes.clone();
            if let Some(byte) = flipped.get_mut(at) {
                *byte ^= 1 << bit;
            }
            let result = PostingList::decode(KEY, &flipped);
            assert!(
                names_the_key(&result, KEY),
                "byte {at} bit {bit}: {result:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn test_a_flipped_bit_fails_the_checksum() -> TestResult {
    let mut bytes = golden_bytes("full")?;
    *bytes.get_mut(100).context("byte 100")? ^= 0x01;
    // The stored checksum is the golden full list's last eight bytes; the computed one python-xxhash's of the flipped
    // body.
    assert_eq!(
        PostingList::decode(KEY, &bytes),
        corrupted("checksum 8ff002be2cb5b6b5 does not match the bytes' 0933e64757240b34")
    );
    Ok(())
}

#[test]
fn test_an_unknown_version_is_refused_before_the_checksum() -> TestResult {
    for version in [0u8, 1, 3, 0xff] {
        let mut b = body(&golden_bytes("empty")?)?;
        *b.first_mut().context("the version")? = version;
        let message = format!("unknown posting format version {version}");
        assert_eq!(PostingList::decode(KEY, &forge(&b)), corrupted(&message));
        // A wrong checksum does not hide the version.
        let mut unforged = golden_bytes("empty")?;
        *unforged.first_mut().context("the version")? = version;
        assert_eq!(PostingList::decode(KEY, &unforged), corrupted(&message));
    }
    // Even a value of one byte is refused for its version first.
    assert_eq!(
        PostingList::decode(KEY, &[0x01]),
        corrupted("unknown posting format version 1")
    );
    Ok(())
}

#[test]
fn test_unknown_flags_are_refused() -> TestResult {
    for flags in [0x02u8, 0x81] {
        let mut b = body(&golden_bytes("empty")?)?;
        *b.get_mut(1).context("the flags")? = flags;
        assert_eq!(
            PostingList::decode(KEY, &forge(&b)),
            corrupted(&format!("unknown flags {flags:#04x}"))
        );
    }
    Ok(())
}

#[test]
fn test_bytes_after_the_splits_are_refused() -> TestResult {
    let mut b = body(&golden_bytes("full")?)?;
    b.push(0x00);
    assert_eq!(
        PostingList::decode(KEY, &forge(&b)),
        corrupted("trailing bytes after the splits: 1")
    );
    let mut empty_body = body(&golden_bytes("empty")?)?;
    empty_body.extend_from_slice(&[0x00; 3]);
    assert_eq!(
        PostingList::decode(KEY, &forge(&empty_body)),
        corrupted("trailing bytes after the splits: 3")
    );
    Ok(())
}

#[test]
fn test_a_count_past_the_end_is_refused() -> TestResult {
    // The empty list's posting count, at byte 10, says 1: the posting's key needs 16 bytes where 4 are left.
    let mut b = body(&golden_bytes("empty")?)?;
    *b.get_mut(10).context("the posting count")? = 1;
    assert_eq!(
        PostingList::decode(KEY, &forge(&b)),
        corrupted("a field at byte 14 overruns the value")
    );
    Ok(())
}

#[test]
fn test_a_length_past_the_end_is_refused_without_reading_it() -> TestResult {
    // The second posting's value length, at byte 182, says 2^32 - 1.
    assert_eq!(
        PostingList::decode(KEY, &full_with(182, &u32::MAX.to_le_bytes())?),
        corrupted("a field at byte 186 overruns the value")
    );
    Ok(())
}

#[test]
fn test_every_count_at_its_largest_is_refused() -> TestResult {
    // The block count (15), the posting count (79), the first posting's facet count (128), the second's statement ID
    // count (200) and the split count (236) at 2^32 - 1: each runs out of bytes rather than allocating for them.
    for at in [15usize, 79, 128, 200, 236] {
        let result = PostingList::decode(KEY, &full_with(at, &u32::MAX.to_le_bytes())?);
        assert!(names_the_key(&result, KEY), "count at {at}: {result:?}");
    }
    Ok(())
}

#[test]
fn test_an_unknown_posting_type_is_refused() -> TestResult {
    // Dgraph's VALUE_LANG, 2, is not carried; 255 is no type at all.
    for posting_type in [2u8, 0xff] {
        assert_eq!(
            PostingList::decode(KEY, &full_with(103, &[posting_type])?),
            corrupted(&format!("unknown posting type {posting_type} at byte 103"))
        );
    }
    Ok(())
}

#[test]
fn test_a_facet_key_that_is_not_utf8_is_refused() -> TestResult {
    // `since`'s first byte, at 136, becomes a lone continuation byte.
    assert_eq!(
        PostingList::decode(KEY, &full_with(136, &[0x80])?),
        corrupted("the facet key at byte 132 is not UTF-8")
    );
    Ok(())
}

#[test]
fn test_a_statement_id_that_is_no_valid_id_is_refused() -> TestResult {
    // The first statement ID's sequence, at bytes 206 to 213, all zero.
    assert_eq!(
        PostingList::decode(KEY, &full_with(206, &[0; 8])?),
        corrupted("the statement ID at byte 204 is invalid: invalid system ID: its sequence is 0")
    );
    Ok(())
}

#[test]
fn test_a_corrupt_pack_is_corruption_naming_the_key() -> TestResult {
    // The pack's own version, at byte 10, becomes 2.
    assert_eq!(
        PostingList::decode(KEY, &full_with(10, &[2])?),
        corrupted("the pack is corrupt: unknown packed ID list version 2")
    );
    // The first block's first ID's last reserved byte, at 34, becomes 1.
    assert_eq!(
        PostingList::decode(KEY, &full_with(34, &[1])?),
        corrupted(
            "the pack is corrupt: the ID at position 0 is invalid: invalid system ID: its reserved bits are 0x000001, not zero"
        )
    );
    Ok(())
}

#[test]
fn test_the_error_names_the_key_it_was_given() -> TestResult {
    let bytes = golden_bytes("empty")?;
    for key in [&b""[..], b"\x09other"] {
        let result = PostingList::decode(key, bytes.get(..3).context("a prefix")?);
        assert!(names_the_key(&result, key), "{result:?}");
    }
    Ok(())
}

#[test]
fn test_the_fuzz_seeds_are_valid_lists() -> TestResult {
    let dir = testdata_dir()?.join("fuzz_seeds/posting_list_decode");
    let seeds = ultragraphdb_tool_bdd::fuzz_regressions_in(&dir)?;
    assert_eq!(seeds.len(), 4, "the four golden encodings");
    for (name, bytes) in seeds {
        let plist = PostingList::decode(KEY, &bytes)
            .map_err(|e| format!("{name}: {e}"))
            .context("a fuzz seed decodes")?;
        assert_eq!(plist.encode(), bytes, "{name}");
    }
    Ok(())
}
