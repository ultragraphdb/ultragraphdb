/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The posting format. The golden encodings were built by hand from the layout the format documents, by a script
//! independent of this crate, and checksummed with python-xxhash 3.6.0, which wraps the xxHash C library (its XXH3-64
//! of the empty input, seed 0, is 0x2d06800538d394c2, the value of xxHash's `doc/xxhash_spec.md`). A test that changes
//! a field behind the checksum recomputes the checksum with `xxhash-rust` only to reach the check it means; no
//! expected checksum comes from it.

use ultragraphdb_core_codec::{
    Facet, Posting, PostingList, PostingType, UidBlock, UidPack, decode as decode_uids, encode,
};
use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use xxhash_rust::xxh3::xxh3_64;

const KEY: &[u8] = b"\x01\x00\x07list";

/// An empty list: version 1, no flags, commit timestamp 0, no postings, no splits, and the checksum.
const EMPTY: &str = "010000000000000000000000000000000000d078c3ec1d4fad97";
/// A list with a pack of block size 0 and no blocks: the pack flag, the block size and a block count of 0.
const EMPTY_PACK: &str = "0101000000000000000000000000000000000000000000000000a391b0ac5dc4bbc6";
/// A delta of one delete-all: uid 2^64-1, op 2, a reference, start timestamp 3, the value `_STAR_ALL`.
const DELETE_ALL: &str = "0100000000000000000001000000ffffffffffffffff020000000003000000000000000000000000000000090000005f535441525f414c4c00000000000000000000000000000000e008ad7e552f2b74";
/// [`full`]: commit timestamp 0x1122334455667788, a pack of block size 4 with the blocks [1, 2, 3, 4] (deltas
/// `00 01 01 01 00`) and [2^32 + 5] (one empty group), a reference at 2 with the facet `since`, a value posting at
/// 2^32 + 5 with the value `01 61 62 63`, the language `en` and the statement IDs 11 and 2^64-1, and the splits 1 and
/// 2^32. Its fields start at: the posting count 60, the first posting 64 (its type 76, its facet key 105), the second
/// 118 (its value 147), the split count 185, the checksum 205.
const FULL: &str = "0101887766554433221104000000020000000100000000000000040000000500000000010101000500000001000000010000000500000000000000000200000002000000000000000100000000050000000000000006000000000000000000000000000000010000000500000073696e6365000000000500000001000000030000000109000000000000000a00000000000000040000000161626302000000656e00000000020000000b00000000000000ffffffffffffffff02000000010000000000000000000000010000008826b2dd2e7f1d6f";

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("hex"))
        .collect()
}

/// The list [`FULL`] encodes, every field distinct from the others.
fn full() -> PostingList {
    PostingList::default()
        .with_commit_ts(0x1122_3344_5566_7788)
        .with_pack(UidPack::new(
            4,
            vec![
                UidBlock::new(1, vec![0x00, 0x01, 0x01, 0x01, 0x00], 4),
                UidBlock::new((1 << 32) + 5, vec![0x00; 5], 1),
            ],
        ))
        .with_postings(vec![
            Posting::default()
                .with_uid(2)
                .with_op(1)
                .with_start_ts(5)
                .with_commit_ts(6)
                .with_facets(vec![Facet::new("since")]),
            Posting::default()
                .with_uid((1 << 32) + 5)
                .with_op(3)
                .with_posting_type(PostingType::Value)
                .with_start_ts(9)
                .with_commit_ts(10)
                .with_value(vec![0x01, b'a', b'b', b'c'])
                .with_lang_tag(b"en".to_vec())
                .with_statement_ids(vec![11, u64::MAX]),
        ])
        .with_splits(vec![1, 1 << 32])
}

fn delete_all() -> PostingList {
    PostingList::default().with_postings(vec![
        Posting::default()
            .with_uid(u64::MAX)
            .with_op(2)
            .with_start_ts(3)
            .with_value(b"_STAR_ALL".to_vec()),
    ])
}

fn goldens() -> Vec<(&'static str, PostingList, Vec<u8>)> {
    vec![
        ("empty", PostingList::default(), unhex(EMPTY)),
        (
            "empty pack",
            PostingList::default().with_pack(UidPack::new(0, Vec::new())),
            unhex(EMPTY_PACK),
        ),
        ("delete-all", delete_all(), unhex(DELETE_ALL)),
        ("full", full(), unhex(FULL)),
    ]
}

/// `body` with its checksum appended, to reach a check behind it.
fn forge(body: &[u8]) -> Vec<u8> {
    let mut bytes = body.to_vec();
    bytes.extend_from_slice(&xxh3_64(body).to_le_bytes());
    bytes
}

/// The body of `bytes`: everything before the checksum.
fn body(bytes: &[u8]) -> Vec<u8> {
    bytes[..bytes.len() - 8].to_vec()
}

fn corrupted(reason: &str) -> Result<PostingList, StoreError> {
    Err(StoreError::Corrupted(KEY.to_vec(), reason.to_string()))
}

fn names_the_key(result: &Result<PostingList, StoreError>, key: &[u8]) -> bool {
    matches!(result, Err(StoreError(StoreErrorEnum::Corrupted { key: k, .. })) if k == key)
}

#[test]
fn a_list_encodes_to_its_golden_bytes() {
    for (name, plist, bytes) in goldens() {
        assert_eq!(plist.encode(), bytes, "{name}");
    }
}

#[test]
fn a_list_appended_to_a_buffer_is_its_golden_bytes_after_what_was_there() {
    for (name, list, golden) in goldens() {
        let mut buffer = b"before".to_vec();
        list.encode_into(&mut buffer);
        assert_eq!(&buffer[..6], b"before", "{name}");
        assert_eq!(
            &buffer[6..],
            golden.as_slice(),
            "{name}: the checksum covers the list alone"
        );
    }
}

#[test]
fn golden_bytes_decode_to_their_list() {
    for (name, plist, bytes) in goldens() {
        assert_eq!(PostingList::decode(KEY, &bytes), Ok(plist), "{name}");
    }
}

#[test]
fn the_encoded_length_is_the_length_of_the_bytes() {
    for (name, plist, bytes) in goldens() {
        assert_eq!(plist.encoded_len(), bytes.len(), "{name}");
    }
    // A value longer than a u16 counts, so a length written in two bytes would show.
    let long = PostingList::default().with_postings(vec![
        Posting::default()
            .with_uid(1)
            .with_value(vec![0xab; 70_000]),
    ]);
    // 26 for the empty list, 45 for a posting's fixed fields and counts, and the value's bytes.
    assert_eq!(long.encoded_len(), 26 + 45 + 70_000);
    assert_eq!(long.encode().len(), long.encoded_len());
}

#[test]
fn a_long_value_round_trips() {
    let long = PostingList::default().with_postings(vec![
        Posting::default()
            .with_uid(1)
            .with_value(vec![0xab; 70_000])
            .with_lang_tag(vec![b'x'; 66_000]),
    ]);
    assert_eq!(PostingList::decode(KEY, &long.encode()), Ok(long));
}

#[test]
fn boundary_values_round_trip() {
    let edge = PostingList::default()
        .with_commit_ts(u64::MAX)
        .with_pack(UidPack::new(
            u32::MAX,
            vec![UidBlock::new(u64::MAX, Vec::new(), u32::MAX)],
        ))
        .with_postings(vec![
            Posting::default()
                .with_uid(u64::MAX)
                .with_op(u32::MAX)
                .with_start_ts(u64::MAX)
                .with_commit_ts(u64::MAX)
                .with_facets(vec![Facet::new(""), Facet::new("é")])
                .with_statement_ids(vec![0, u64::MAX]),
            Posting::default(),
        ])
        .with_splits(vec![0, u64::MAX]);
    assert_eq!(PostingList::decode(KEY, &edge.encode()), Ok(edge));
}

#[test]
fn a_decoded_list_encodes_back_to_its_bytes() {
    for (name, _, bytes) in goldens() {
        let plist = PostingList::decode(KEY, &bytes).expect(name);
        assert_eq!(plist.encode(), bytes, "{name}");
    }
}

#[test]
fn a_list_without_a_pack_and_one_with_an_empty_pack_differ() {
    let none = PostingList::default();
    let empty = PostingList::default().with_pack(UidPack::new(0, Vec::new()));
    assert_ne!(none.encode(), empty.encode());
    assert_eq!(PostingList::decode(KEY, &empty.encode()), Ok(empty));
    assert_eq!(PostingList::decode(KEY, &none.encode()), Ok(none));
}

#[test]
fn the_blocks_keep_the_encoders_deltas_byte_for_byte() {
    // uid-lists' byte identity: the deltas the encoder writes are stored and read back unchanged.
    // Ascending, with gaps of up to four bytes and the upper 32 bits changing every hundred UIDs.
    let uids: Vec<u64> = (0..600u64)
        .map(|i| (i / 100) * (1 << 32) + i * i * 977)
        .collect();
    let pack = encode(&uids, 256).expect("the list packs");
    let plist = PostingList::default().with_pack(pack.clone());
    let decoded = PostingList::decode(KEY, &plist.encode()).expect("decodes");
    let read = decoded.pack().expect("a pack");
    assert_eq!(read.blocks().len(), pack.blocks().len());
    for (a, b) in read.blocks().iter().zip(pack.blocks()) {
        assert_eq!(a.deltas(), b.deltas());
    }
    assert_eq!(decode_uids(read, 0), uids);
}

#[test]
fn every_truncation_is_corruption_naming_the_key() {
    let bytes = unhex(FULL);
    for len in 0..bytes.len() {
        let result = PostingList::decode(KEY, &bytes[..len]);
        assert!(names_the_key(&result, KEY), "{len} bytes: {result:?}");
    }
}

#[test]
fn a_value_shorter_than_an_empty_list_is_truncated() {
    let bytes = unhex(EMPTY);
    assert_eq!(
        PostingList::decode(KEY, &[]),
        corrupted("the value is 0 bytes, shorter than the 26 of an empty posting list")
    );
    assert_eq!(
        PostingList::decode(KEY, &bytes[..1]),
        corrupted("the value is 1 bytes, shorter than the 26 of an empty posting list")
    );
    assert_eq!(
        PostingList::decode(KEY, &bytes[..25]),
        corrupted("the value is 25 bytes, shorter than the 26 of an empty posting list")
    );
}

#[test]
fn every_bit_flip_is_corruption_naming_the_key() {
    let bytes = unhex(FULL);
    for at in 0..bytes.len() {
        for bit in 0..8 {
            let mut flipped = bytes.clone();
            flipped[at] ^= 1 << bit;
            let result = PostingList::decode(KEY, &flipped);
            assert!(
                names_the_key(&result, KEY),
                "byte {at} bit {bit}: {result:?}"
            );
        }
    }
}

#[test]
fn every_flipped_byte_is_corruption_naming_the_key() {
    let bytes = unhex(FULL);
    for at in 0..bytes.len() {
        let mut flipped = bytes.clone();
        flipped[at] ^= 0xff;
        let result = PostingList::decode(KEY, &flipped);
        assert!(names_the_key(&result, KEY), "byte {at}: {result:?}");
    }
}

#[test]
fn a_flipped_bit_fails_the_checksum() {
    let mut bytes = unhex(FULL);
    bytes[100] ^= 0x01;
    // The stored checksum is FULL's last eight bytes; the computed one python-xxhash's of the flipped body.
    assert_eq!(
        PostingList::decode(KEY, &bytes),
        corrupted("checksum 6f1d7f2eddb22688 does not match the bytes' 065e89c950052865")
    );
}

#[test]
fn an_unknown_version_is_refused_before_the_checksum() {
    for version in [0u8, 2, 0xff] {
        let mut b = body(&unhex(EMPTY));
        b[0] = version;
        let message = format!("unknown posting format version {version}");
        assert_eq!(PostingList::decode(KEY, &forge(&b)), corrupted(&message));
        // A wrong checksum does not hide the version.
        let mut unforged = unhex(EMPTY);
        unforged[0] = version;
        assert_eq!(PostingList::decode(KEY, &unforged), corrupted(&message));
    }
    // Even a value of one byte is refused for its version first.
    assert_eq!(
        PostingList::decode(KEY, &[0x02]),
        corrupted("unknown posting format version 2")
    );
}

#[test]
fn unknown_flags_are_refused() {
    for flags in [0x02u8, 0x81] {
        let mut b = body(&unhex(EMPTY));
        b[1] = flags;
        assert_eq!(
            PostingList::decode(KEY, &forge(&b)),
            corrupted(&format!("unknown flags {flags:#04x}"))
        );
    }
}

#[test]
fn bytes_after_the_splits_are_refused() {
    let mut b = body(&unhex(FULL));
    b.push(0x00);
    assert_eq!(
        PostingList::decode(KEY, &forge(&b)),
        corrupted("trailing bytes after the splits: 1")
    );
    let mut b = body(&unhex(EMPTY));
    b.extend_from_slice(&[0x00; 3]);
    assert_eq!(
        PostingList::decode(KEY, &forge(&b)),
        corrupted("trailing bytes after the splits: 3")
    );
}

#[test]
fn a_count_past_the_end_is_refused() {
    // The empty list's posting count, at byte 10, says 1: its split count is read as the posting's uid, which needs
    // eight bytes where four are left.
    let mut b = body(&unhex(EMPTY));
    b[10] = 1;
    assert_eq!(
        PostingList::decode(KEY, &forge(&b)),
        corrupted("a field at byte 14 overruns the value")
    );
}

#[test]
fn a_length_past_the_end_is_refused_without_reading_it() {
    // The second posting's value length, at byte 147, says 2^32 - 1.
    let mut b = body(&unhex(FULL));
    b[147..151].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(
        PostingList::decode(KEY, &forge(&b)),
        corrupted("a field at byte 151 overruns the value")
    );
}

#[test]
fn every_count_at_its_largest_is_refused() {
    // The block count (14), the posting count (60), the first posting's facet count (101), the second's statement ID
    // count (165) and the split count (185) at 2^32 - 1: each runs out of bytes rather than allocating for them.
    for at in [14usize, 60, 101, 165, 185] {
        let mut b = body(&unhex(FULL));
        b[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        let result = PostingList::decode(KEY, &forge(&b));
        assert!(names_the_key(&result, KEY), "count at {at}: {result:?}");
    }
}

#[test]
fn an_unknown_posting_type_is_refused() {
    // Dgraph's VALUE_LANG, 2, is not carried; 255 is no type at all.
    for posting_type in [2u8, 0xff] {
        let mut b = body(&unhex(FULL));
        b[76] = posting_type;
        assert_eq!(
            PostingList::decode(KEY, &forge(&b)),
            corrupted(&format!("unknown posting type {posting_type} at byte 76"))
        );
    }
}

#[test]
fn a_facet_key_that_is_not_utf8_is_refused() {
    let mut b = body(&unhex(FULL));
    // `since`'s first byte, at 109, becomes a lone continuation byte.
    b[109] = 0x80;
    assert_eq!(
        PostingList::decode(KEY, &forge(&b)),
        corrupted("the facet key at byte 105 is not UTF-8")
    );
}

#[test]
fn the_error_names_the_key_it_was_given() {
    let bytes = unhex(EMPTY);
    for key in [&b""[..], b"\x09other"] {
        let result = PostingList::decode(key, &bytes[..3]);
        assert!(names_the_key(&result, key), "{result:?}");
    }
}

#[test]
fn the_fuzz_seeds_are_valid_lists() {
    let dir = ultragraphdb_tool_bdd::testdata_dir().join("fuzz_seeds/posting_list_decode");
    let seeds = ultragraphdb_tool_bdd::fuzz_regressions_in(&dir);
    assert_eq!(seeds.len(), 4, "the four golden encodings");
    for (name, bytes) in seeds {
        let plist = PostingList::decode(KEY, &bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(plist.encode(), bytes, "{name}");
    }
}
