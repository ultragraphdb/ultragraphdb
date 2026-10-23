/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use serde::Deserialize;
use ultragraphdb_core_codec::{Decoder, SeekPos, decode, encode};
use ultragraphdb_proto_pb::pb::UidPack;
use ultragraphdb_test_bdd::{load_golden, testdata_dir};

#[derive(Debug, Clone, Deserialize)]
struct SeekStep {
    op: String,
    target: u64,
    whence: String,
    uids: Vec<u64>,
    len: usize,
}

#[derive(Debug, Clone, Deserialize)]
struct SeekCase {
    name: String,
    block_size: usize,
    uids: Vec<u64>,
    steps: Vec<SeekStep>,
}

fn golden_cases() -> Vec<SeekCase> {
    let path = testdata_dir().join("golden/codec_seek.json");
    let golden = load_golden::<SeekCase>(&path);
    assert!(golden.cases().len() >= 6, "the golden file holds the cases");
    golden.cases().to_vec()
}

fn whence(name: &str) -> SeekPos {
    match name {
        "start" => SeekPos::Start,
        "current" => SeekPos::Current,
        other => panic!("unknown whence {other:?}"),
    }
}

fn pack(case: &SeekCase) -> UidPack {
    encode(&case.uids, case.block_size).expect("the golden lists are not empty")
}

fn steps<'a>(case: &'a SeekCase, op: &'a str, pos: &'a str) -> impl Iterator<Item = &'a SeekStep> {
    case.steps
        .iter()
        .filter(move |s| s.op == op && s.whence == pos)
}

#[test]
fn decode_from_a_target_matches_golden() {
    for case in golden_cases() {
        let pack = pack(&case);
        let mut checked = 0;
        for step in steps(&case, "decode", "") {
            let actual = decode(&pack, step.target);
            assert_eq!(
                step.len,
                actual.len(),
                "{} decode {}",
                case.name,
                step.target
            );
            assert_eq!(
                step.uids[..],
                actual[..step.uids.len()],
                "{} decode {}",
                case.name,
                step.target
            );
            let suffix: Vec<u64> = case
                .uids
                .iter()
                .copied()
                .filter(|&u| u >= step.target)
                .collect();
            assert_eq!(suffix, actual, "{} decode {}", case.name, step.target);
            checked += 1;
        }
        assert!(checked > 0, "{}", case.name);
    }
}

#[test]
fn seek_matches_golden() {
    for case in golden_cases() {
        let pack = pack(&case);
        let mut dec = Decoder::unpositioned(&pack);
        for pos in ["start", "current"] {
            for step in steps(&case, "seek", pos) {
                let actual = dec.seek(step.target, whence(pos));
                assert_eq!(
                    step.uids, actual,
                    "{} seek {} {pos}",
                    case.name, step.target
                );
            }
        }
    }
}

#[test]
fn seek_to_block_matches_golden() {
    for case in golden_cases() {
        let pack = pack(&case);
        for pos in ["start", "current"] {
            let mut dec = Decoder::new(&pack);
            for step in steps(&case, "seek_to_block", pos) {
                let actual = dec.seek_to_block(step.target, whence(pos));
                assert_eq!(
                    step.uids, actual,
                    "{} seek_to_block {} {pos}",
                    case.name, step.target
                );
            }
        }
    }
}

#[test]
fn linear_seek_matches_golden() {
    for case in golden_cases() {
        let pack = pack(&case);
        let mut dec = Decoder::new(&pack);
        for step in steps(&case, "linear_seek", "") {
            let actual = dec.linear_seek(step.target);
            assert_eq!(
                step.uids, actual,
                "{} linear_seek {}",
                case.name, step.target
            );
        }
    }
}

#[test]
fn a_new_decoder_holds_the_first_block() {
    let uids: Vec<u64> = (1..=25).collect();
    let pack = encode(&uids, 10).expect("the list packs");
    let mut dec = Decoder::new(&pack);
    assert_eq!(0, dec.block_idx());
    assert_eq!(&uids[..10], dec.uids());
    assert_eq!(30, dec.approx_len());
    assert_eq!(11, dec.peek_next_base());
    assert!(dec.valid());
    assert_eq!(&uids[10..20], dec.next_block());
    assert_eq!(&uids[20..], dec.next_block());
    assert_eq!(u64::MAX, dec.peek_next_base());
    assert!(dec.valid());
    assert!(dec.next_block().is_empty());
    assert!(!dec.valid());
    assert_eq!(0, dec.approx_len());
    assert_eq!(&pack, dec.pack());
}

#[test]
fn linear_seek_past_the_last_uid_ends_the_decoder() {
    // Go's LinearSeek never returns for u64::MAX, whose next base past the last block reads as u64::MAX; the port
    // stops past the last block.
    let pack = encode(&[1, 2, 3], 2).expect("the list packs");
    let mut dec = Decoder::new(&pack);
    assert!(dec.linear_seek(u64::MAX).is_empty());
    assert!(!dec.valid());
}

#[test]
#[should_panic]
fn seek_to_block_past_the_last_block_panics_as_dgraph_does() {
    // Dgraph's SeekToBlock indexes the current block, which a decoder past the last block does not have.
    let pack = encode(&[1, 2, 3], 2).expect("the list packs");
    let mut dec = Decoder::new(&pack);
    assert!(dec.seek(4, SeekPos::Start).is_empty());
    dec.seek_to_block(5, SeekPos::Current);
}
