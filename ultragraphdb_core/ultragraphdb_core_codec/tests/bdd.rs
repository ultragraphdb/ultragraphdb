/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use cucumber::{World, given, then, when};
use serde::Deserialize;
use ultragraphdb_core_codec::{Decoder, SeekPos, decode, encode};
use ultragraphdb_proto_pb::pb::UidPack;

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
    w.result = Decoder::unpositioned(pack_of(w)).seek(target, pos).to_vec();
}

#[when("the decoder moves to the next block")]
fn next_block(w: &mut Steps) {
    let (target, pos) = w.seek.expect("a decoder sought");
    let mut decoder = Decoder::unpositioned(pack_of(w));
    decoder.seek(target, pos);
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

fn main() {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_test_bdd::features_dir()),
    );
}
