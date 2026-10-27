/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use serde::Deserialize;
use ultragraphdb_core_codec::{Encoder, decode, encode, exact_len};
use ultragraphdb_proto_pb::pb::{UidBlock, UidPack};
use ultragraphdb_tool_bdd::{compare_bytes, load_golden, testdata_dir};

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

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("hex"))
        .collect()
}

/// The golden pack as the message Dgraph produced.
fn to_pack(golden: &GoldenPack) -> UidPack {
    UidPack {
        block_size: golden.block_size,
        blocks: golden
            .blocks
            .iter()
            .map(|b| UidBlock {
                base: b.base,
                deltas: unhex(&b.deltas),
                num_uids: b.num_uids,
            })
            .collect(),
        alloc_ref: 0,
    }
}

fn golden_cases() -> Vec<PackCase> {
    let path = testdata_dir().join("golden/codec_uidpack.json");
    let golden = load_golden::<PackCase>(&path);
    assert!(
        golden.cases().len() > 500,
        "the golden file holds the cases"
    );
    golden.cases().to_vec()
}

#[test]
fn encoding_matches_golden_packs() {
    for case in golden_cases() {
        let actual = encode(&case.uids, case.block_size);
        match (&case.pack, actual) {
            (None, None) => {}
            (Some(want), Some(got)) => {
                let want = to_pack(want);
                assert_eq!(want.block_size, got.block_size, "{}", case.name);
                assert_eq!(want.blocks.len(), got.blocks.len(), "{}", case.name);
                for (i, (a, b)) in want.blocks.iter().zip(&got.blocks).enumerate() {
                    assert_eq!(a.base, b.base, "{} block {i}", case.name);
                    assert_eq!(a.num_uids, b.num_uids, "{} block {i}", case.name);
                    compare_bytes(&format!("{} block {i}", case.name), &a.deltas, &b.deltas)
                        .unwrap();
                }
            }
            (want, got) => panic!("{}: Dgraph packed {want:?}, the port {got:?}", case.name),
        }
    }
}

#[test]
fn golden_packs_decode_to_their_lists() {
    for case in golden_cases() {
        let Some(golden) = &case.pack else {
            assert!(case.uids.is_empty(), "{}", case.name);
            continue;
        };
        let pack = to_pack(golden);
        assert_eq!(case.uids.len(), exact_len(&pack), "{}", case.name);
        assert_eq!(case.uids, decode(&pack, 0), "{}", case.name);
    }
}

#[test]
fn a_one_uid_block_emits_one_empty_group() {
    let pack = encode(&[7], 256).expect("one UID packs");
    assert_eq!(1, pack.blocks.len());
    assert_eq!(7, pack.blocks[0].base);
    assert_eq!(1, pack.blocks[0].num_uids);
    assert_eq!(vec![0u8; 5], pack.blocks[0].deltas);
}

#[test]
fn a_block_closes_where_the_upper_32_bits_change() {
    let pack = encode(&[1, 2, (1 << 32) + 1], 256).expect("the list packs");
    let bases: Vec<u64> = pack.blocks.iter().map(|b| b.base).collect();
    assert_eq!(vec![1, (1 << 32) + 1], bases);
}

#[test]
fn block_size_zero_packs_one_uid_per_block() {
    let mut encoder = Encoder::new(0);
    assert_eq!(0, encoder.block_size());
    for uid in [3, 4, 9] {
        encoder.add(uid);
    }
    let pack = encoder.done().expect("the list packs");
    assert_eq!(0, pack.block_size);
    assert_eq!(
        vec![3, 4, 9],
        pack.blocks.iter().map(|b| b.base).collect::<Vec<_>>()
    );
    assert_eq!(vec![3, 4, 9], decode(&pack, 0));
}

#[test]
fn an_encoder_without_uids_returns_no_pack() {
    assert!(Encoder::new(256).done().is_none());
}
