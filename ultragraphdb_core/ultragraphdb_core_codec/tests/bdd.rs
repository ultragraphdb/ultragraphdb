/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::ParseIntError;
use std::str::FromStr;

use cucumber::{World, given, then, when};
use serde::Deserialize;
use ultragraphdb_core_codec::{
    Decoder, Facet, Posting, PostingList, PostingType, SeekPos, UidBlock, UidPack, decode, encode,
};
use ultragraphdb_error_store::{StoreError, StoreErrorEnum};

#[derive(Debug, Clone, Deserialize)]
struct GoldenBlock {
    base: u64,
    deltas: String,
    num_uids: u32,
}

#[derive(Debug, Clone, Deserialize)]
struct GoldenPack {
    block_size: u32,
    blocks: Vec<GoldenBlock>,
}

#[derive(Debug, Clone, Deserialize)]
struct PackCase {
    name: String,
    block_size: usize,
    uids: Vec<u64>,
    pack: Option<GoldenPack>,
}

#[derive(Debug, Default, World)]
struct Steps {
    golden: Vec<PackCase>,
    uids: Vec<u64>,
    lists: Vec<Vec<u64>>,
    pack: Option<UidPack>,
    mismatches: Vec<String>,
    seek: Option<(u64, SeekPos)>,
    result: Vec<u64>,
    plist: PostingList,
    key: Vec<u8>,
    stored: Vec<u8>,
    decoded: Vec<Result<PostingList, StoreError>>,
}

/// A UID list a step names, `1, 2, 3`.
struct Uids(Vec<u64>);

impl FromStr for Uids {
    type Err = ParseIntError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        text.split(',')
            .map(|s| s.trim().parse())
            .collect::<Result<_, _>>()
            .map(Uids)
    }
}

/// An inclusive UID range a step names, `1-10`.
struct UidRange(Vec<u64>);

impl FromStr for UidRange {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let (from, to) = text.split_once('-').ok_or("a range")?;
        let uid = |s: &str| s.parse::<u64>().map_err(|e| format!("a UID: {e}"));
        Ok(UidRange((uid(from)?..=uid(to)?).collect()))
    }
}

/// Bytes a step names in hex, `0a ff`.
struct HexBytes(Vec<u8>);

impl FromStr for HexBytes {
    type Err = ParseIntError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        text.split_whitespace()
            .map(|b| u8::from_str_radix(b, 16))
            .collect::<Result<_, _>>()
            .map(HexBytes)
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A seek position a step names, Go's `SeekStart` or `SeekCurrent`.
#[derive(Clone, Copy)]
struct Whence(SeekPos);

impl FromStr for Whence {
    type Err = String;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            "SeekStart" => Ok(Whence(SeekPos::Start)),
            "SeekCurrent" => Ok(Whence(SeekPos::Current)),
            other => Err(format!("unknown seek position {other}")),
        }
    }
}

fn pack_of(w: &Steps) -> &UidPack {
    w.pack.as_ref().expect("the list was packed")
}

#[given(expr = "the golden UID lists generated from Dgraph with block size {int}")]
fn golden_lists(w: &mut Steps, block_size: usize) {
    let path = ultragraphdb_tool_bdd::testdata_dir().join("golden/codec_uidpack.json");
    let golden = ultragraphdb_tool_bdd::load_golden::<PackCase>(&path);
    w.golden = golden
        .cases()
        .iter()
        .filter(|c| c.block_size == block_size)
        .cloned()
        .collect();
    assert!(w.golden.len() > 300, "the golden file holds the lists");
}

#[when(expr = "each golden list is encoded with block size {int}")]
fn encode_golden(w: &mut Steps, block_size: usize) {
    for case in &w.golden {
        let actual = encode(&case.uids, block_size);
        let same = match (&case.pack, &actual) {
            (None, None) => true,
            (Some(want), Some(got)) => {
                want.block_size == got.block_size()
                    && want.blocks.len() == got.blocks().len()
                    && want.blocks.iter().zip(got.blocks()).all(|(a, b)| {
                        a.base == b.base()
                            && a.num_uids == b.num_uids()
                            && a.deltas == hex(b.deltas())
                    })
            }
            _ => false,
        };
        if !same {
            w.mismatches.push(case.name.clone());
        }
    }
}

#[then("every block's base, deltas and count equal Dgraph's")]
fn golden_equal(w: &mut Steps) {
    assert!(
        w.mismatches.is_empty(),
        "packs differ from Dgraph's: {:?}",
        w.mismatches
    );
}

#[given(expr = "the UID list {string}")]
fn uid_list(w: &mut Steps, uids: Uids) {
    w.uids = uids.0;
}

#[when(expr = "the list is encoded with block size {int}")]
fn encode_list(w: &mut Steps, block_size: usize) {
    w.pack = encode(&w.uids, block_size);
}

#[then(expr = "the pack has {int} block(s)")]
fn block_count(w: &mut Steps, n: usize) {
    assert_eq!(pack_of(w).blocks().len(), n);
}

#[then(expr = "the block bases are {string}")]
fn block_bases(w: &mut Steps, want: Uids) {
    let Uids(want) = want;
    let bases: Vec<u64> = pack_of(w).blocks().iter().map(UidBlock::base).collect();
    assert_eq!(bases, want);
}

#[then(expr = "block {int} has base {int}, count {int} and deltas {string}")]
fn block_is(w: &mut Steps, index: usize, base: u64, count: u32, deltas: HexBytes) {
    let HexBytes(want) = deltas;
    let block = &pack_of(w).blocks()[index - 1];
    assert_eq!(block.base(), base);
    assert_eq!(block.num_uids(), count);
    ultragraphdb_tool_bdd::compare_bytes(&format!("block {index}"), &want, block.deltas()).unwrap();
}

#[given("every sorted UID list of length 0 to 100, below 2^32 and across 2^32")]
fn every_length(w: &mut Steps) {
    for n in 0..=100u64 {
        // Gaps of one to four bytes below 2^32, and a list centred on 2^32, whose upper 32 bits change inside it.
        w.lists
            .push((0..n).map(|i| i * i * 400_000 + i * 7).collect());
        w.lists
            .push((0..n).map(|i| (1u64 << 32) - n * 3 / 2 + i * 3).collect());
    }
    assert!(
        w.lists
            .iter()
            .any(|l| l.first().is_some_and(|f| f >> 32 == 0)
                && l.last().is_some_and(|l| l >> 32 == 1))
    );
}

#[when(expr = "each list is encoded with block size {int} and decoded")]
fn round_trip(w: &mut Steps, block_size: usize) {
    for list in &w.lists {
        let decoded = encode(list, block_size)
            .map(|p| decode(&p, 0))
            .unwrap_or_default();
        if &decoded != list {
            w.mismatches
                .push(format!("{list:?} decoded as {decoded:?}"));
        }
    }
}

#[then("each decoded list equals its input")]
fn round_trip_equal(w: &mut Steps) {
    assert!(w.mismatches.is_empty(), "{:?}", w.mismatches);
}

#[given(expr = "the UIDs {int} to {int} in steps of {int}, packed with block size {int}")]
fn stepped(w: &mut Steps, from: u64, to: u64, stride: usize, block_size: usize) {
    w.uids = (from..=to).step_by(stride).collect();
    w.pack = encode(&w.uids, block_size);
}

#[when(expr = "the pack is decoded from {int}")]
fn decode_from(w: &mut Steps, target: u64) {
    w.result = decode(pack_of(w), target);
}

#[when(expr = "a decoder seeks {int} with {word}")]
fn seek(w: &mut Steps, target: u64, whence: Whence) {
    let Whence(pos) = whence;
    w.seek = Some((target, pos));
    w.result = Decoder::sought(pack_of(w), target, pos).uids().to_vec();
}

#[when("the decoder moves to the next block")]
fn next_block(w: &mut Steps) {
    let (target, pos) = w.seek.expect("a decoder sought");
    let mut decoder = Decoder::sought(pack_of(w), target, pos);
    w.result = decoder.next_block().to_vec();
}

#[then(expr = "the result holds {int} UIDs from {int} to {int}")]
fn result_holds(w: &mut Steps, n: usize, first: u64, last: u64) {
    assert_eq!(w.result.len(), n);
    assert_eq!(w.result.first(), Some(&first));
    assert_eq!(w.result.last(), Some(&last));
    let run: Vec<u64> = w
        .uids
        .iter()
        .copied()
        .filter(|&u| u >= first && u <= last)
        .collect();
    assert_eq!(
        w.result, run,
        "the result is the input's run from {first} to {last}"
    );
}

#[when(expr = "a decoder seeks to block {int} and then to block {int} with {word}")]
fn seek_to_block_twice(w: &mut Steps, first: u64, second: u64, whence: Whence) {
    let Whence(pos) = whence;
    let mut decoder = Decoder::new(pack_of(w));
    decoder.seek_to_block(first, pos);
    w.result = decoder.seek_to_block(second, pos).to_vec();
}

#[given(expr = "a pack of the blocks {string}, an empty block of base {int}, and {string}")]
fn pack_with_empty_block(w: &mut Steps, first: UidRange, base: u64, last: UidRange) {
    let (UidRange(first), UidRange(last)) = (first, last);
    let mut blocks = encode(&first, 10)
        .expect("the first block packs")
        .blocks()
        .to_vec();
    blocks.push(UidBlock::new(base, Vec::new(), 0));
    blocks.extend_from_slice(encode(&last, 10).expect("the last block packs").blocks());
    let pack = UidPack::new(10, blocks);
    w.uids = first.into_iter().chain(last).collect();
    w.pack = Some(pack);
}

#[when(expr = "a decoder seeks to block {int} with {word}")]
fn seek_to_block_once(w: &mut Steps, target: u64, whence: Whence) {
    let Whence(pos) = whence;
    w.result = Decoder::new(pack_of(w)).seek_to_block(target, pos).to_vec();
}

#[when(expr = "a decoder seeks linearly to {int}")]
fn linear_seek(w: &mut Steps, target: u64) {
    w.result = Decoder::new(pack_of(w)).linear_seek(target).to_vec();
}

#[then(expr = "the result is {string}")]
fn result_is(w: &mut Steps, want: Uids) {
    let Uids(want) = want;
    assert_eq!(w.result, want);
}

#[given("a list with a UID pack, value postings with statement IDs and splits")]
fn full_list(w: &mut Steps) {
    let pack = encode(&[2, 3, 5, (1 << 32) + 7], 256).expect("the list packs");
    w.plist = PostingList::default()
        .with_pack(pack)
        .with_commit_ts(9)
        .with_postings(vec![
            Posting::default()
                .with_uid(2)
                .with_op(1)
                .with_facets(vec![Facet::new("since")]),
            Posting::default()
                .with_uid(5)
                .with_op(1)
                .with_posting_type(PostingType::Value)
                .with_value(b"\x01five".to_vec())
                .with_statement_ids(vec![40, 41]),
            Posting::default()
                .with_uid((1 << 32) + 7)
                .with_op(3)
                .with_posting_type(PostingType::Value)
                .with_value(b"\x01seven".to_vec())
                .with_statement_ids(vec![42]),
        ])
        .with_splits(vec![1, 1 << 32]);
}

#[given(
    expr = "a list with a UID pack, value postings with statement IDs and splits, stored under the key {string}"
)]
fn stored_list(w: &mut Steps, key: String) {
    full_list(w);
    w.key = key.into_bytes();
    w.stored = w.plist.encode();
}

#[when("the list is encoded and decoded")]
fn encode_and_decode(w: &mut Steps) {
    let bytes = w.plist.encode();
    w.decoded = vec![PostingList::decode(b"list", &bytes)];
}

#[then("the decoded list equals the original")]
fn decoded_equals(w: &mut Steps) {
    assert_eq!(w.decoded, vec![Ok(w.plist.clone())]);
}

#[when("one byte of the stored list is flipped and the list is read, for every byte in turn")]
fn flip_each_byte(w: &mut Steps) {
    w.decoded = (0..w.stored.len())
        .map(|at| {
            let mut flipped = w.stored.clone();
            flipped[at] ^= 0x5a;
            PostingList::decode(&w.key, &flipped)
        })
        .collect();
}

#[then(expr = "every read fails with a corruption error naming the key {string}")]
fn every_read_corrupted(w: &mut Steps, key: String) {
    let key = key.into_bytes();
    assert_eq!(w.decoded.len(), w.stored.len());
    for (at, result) in w.decoded.iter().enumerate() {
        assert!(
            matches!(result, Err(StoreError(StoreErrorEnum::Corrupted { key: k, .. })) if *k == key),
            "byte {at}: {result:?}"
        );
    }
}

fn main() {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()),
    );
}
