/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use cucumber::{World, given, then, when};
use serde::Deserialize;
use ultragraphdb_core_codec::{Decoder, SeekPos, decode, encode};
use ultragraphdb_proto_pb::pb::{UidBlock, UidPack};

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
}

fn parse_uids(text: &str) -> Vec<u64> {
    text.split(',')
        .map(|s| s.trim().parse().expect("a UID"))
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn whence(name: &str) -> SeekPos {
    match name {
        "SeekStart" => SeekPos::Start,
        "SeekCurrent" => SeekPos::Current,
        other => panic!("unknown seek position {other}"),
    }
}

fn pack_of(w: &Steps) -> &UidPack {
    w.pack.as_ref().expect("the list was packed")
}

#[given(expr = "the golden UID lists generated from Dgraph with block size {int}")]
fn golden_lists(w: &mut Steps, block_size: usize) {
    let path = ultragraphdb_test_bdd::testdata_dir().join("golden/codec_uidpack.json");
    let golden = ultragraphdb_test_bdd::load_golden::<PackCase>(&path);
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
                want.block_size == got.block_size
                    && want.blocks.len() == got.blocks.len()
                    && want.blocks.iter().zip(&got.blocks).all(|(a, b)| {
                        a.base == b.base && a.num_uids == b.num_uids && a.deltas == hex(&b.deltas)
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
fn uid_list(w: &mut Steps, text: String) {
    w.uids = parse_uids(&text);
}

#[when(expr = "the list is encoded with block size {int}")]
fn encode_list(w: &mut Steps, block_size: usize) {
    w.pack = encode(&w.uids, block_size);
}

#[then(expr = "the pack has {int} block(s)")]
fn block_count(w: &mut Steps, n: usize) {
    assert_eq!(pack_of(w).blocks.len(), n);
}

#[then(expr = "the block bases are {string}")]
fn block_bases(w: &mut Steps, text: String) {
    let bases: Vec<u64> = pack_of(w).blocks.iter().map(|b| b.base).collect();
    assert_eq!(bases, parse_uids(&text));
}

#[then(expr = "block {int} has base {int}, count {int} and deltas {string}")]
fn block_is(w: &mut Steps, index: usize, base: u64, count: u32, deltas: String) {
    let block = &pack_of(w).blocks[index - 1];
    assert_eq!(block.base, base);
    assert_eq!(block.num_uids, count);
    let want: Vec<u8> = deltas
        .split_whitespace()
        .map(|b| u8::from_str_radix(b, 16).expect("a hex byte"))
        .collect();
    ultragraphdb_test_bdd::compare_bytes(&format!("block {index}"), &want, &block.deltas).unwrap();
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
fn seek(w: &mut Steps, target: u64, name: String) {
    let pos = whence(&name);
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
fn seek_to_block_twice(w: &mut Steps, first: u64, second: u64, name: String) {
    let pos = whence(&name);
    let mut decoder = Decoder::new(pack_of(w));
    decoder.seek_to_block(first, pos);
    w.result = decoder.seek_to_block(second, pos).to_vec();
}

#[given(expr = "a pack of the blocks {string}, an empty block of base {int}, and {string}")]
fn pack_with_empty_block(w: &mut Steps, first: String, base: u64, last: String) {
    let range = |text: &str| -> Vec<u64> {
        let (from, to) = text.split_once('-').expect("a range");
        (from.parse().expect("a UID")..=to.parse().expect("a UID")).collect()
    };
    let (first, last) = (range(&first), range(&last));
    let mut pack = encode(&first, 10).expect("the first block packs");
    pack.blocks.push(UidBlock {
        base,
        deltas: Vec::new(),
        num_uids: 0,
    });
    pack.blocks
        .extend(encode(&last, 10).expect("the last block packs").blocks);
    w.uids = first.into_iter().chain(last).collect();
    w.pack = Some(pack);
}

#[when(expr = "a decoder seeks to block {int} with {word}")]
fn seek_to_block_once(w: &mut Steps, target: u64, name: String) {
    let pos = whence(&name);
    w.result = Decoder::new(pack_of(w)).seek_to_block(target, pos).to_vec();
}

#[when(expr = "a decoder seeks linearly to {int}")]
fn linear_seek(w: &mut Steps, target: u64) {
    w.result = Decoder::new(pack_of(w)).linear_seek(target).to_vec();
}

#[then(expr = "the result is {string}")]
fn result_is(w: &mut Steps, text: String) {
    assert_eq!(w.result, parse_uids(&text));
}

fn main() {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_test_bdd::features_dir()),
    );
}
