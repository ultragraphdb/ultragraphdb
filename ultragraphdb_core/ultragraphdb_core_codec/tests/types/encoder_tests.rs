/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use serde::Deserialize;
use ultragraphdb_core_codec::{Encoder, UidBlock, UidPack, decode, encode, exact_len};
use ultragraphdb_tool_bdd::{Context, TestResult, compare_bytes, load_golden, testdata_dir};

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

fn unhex(text: &str) -> TestResult<Vec<u8>> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).context("hex"))
        .collect()
}

/// The golden pack as Dgraph produced it.
fn to_pack(golden: &GoldenPack) -> TestResult<UidPack> {
    Ok(UidPack::new(
        golden.block_size,
        golden
            .blocks
            .iter()
            .map(|b| Ok(UidBlock::new(b.base, unhex(&b.deltas)?, b.num_uids)))
            .collect::<TestResult<_>>()?,
    ))
}

fn golden_cases() -> TestResult<Vec<PackCase>> {
    let path = testdata_dir()?.join("golden/codec_uidpack.json");
    let golden = load_golden::<PackCase>(&path)?;
    assert!(
        golden.cases().len() > 500,
        "the golden file holds the cases"
    );
    Ok(golden.cases().to_vec())
}

#[test]
fn encoding_matches_golden_packs() -> TestResult {
    for case in golden_cases()? {
        let actual = encode(&case.uids, case.block_size);
        assert_eq!(
            case.pack.is_some(),
            actual.is_some(),
            "{}: Dgraph packed {:?}, the port {actual:?}",
            case.name,
            case.pack
        );
        if let (Some(want), Some(got)) = (&case.pack, actual) {
            let want = to_pack(want)?;
            assert_eq!(want.block_size(), got.block_size(), "{}", case.name);
            assert_eq!(want.blocks().len(), got.blocks().len(), "{}", case.name);
            for (i, (a, b)) in want.blocks().iter().zip(got.blocks()).enumerate() {
                assert_eq!(a.base(), b.base(), "{} block {i}", case.name);
                assert_eq!(a.num_uids(), b.num_uids(), "{} block {i}", case.name);
                compare_bytes(&format!("{} block {i}", case.name), a.deltas(), b.deltas())?;
            }
        }
    }
    Ok(())
}

#[test]
fn golden_packs_decode_to_their_lists() -> TestResult {
    for case in golden_cases()? {
        let Some(golden) = &case.pack else {
            assert!(case.uids.is_empty(), "{}", case.name);
            continue;
        };
        let pack = to_pack(golden)?;
        assert_eq!(case.uids.len(), exact_len(&pack), "{}", case.name);
        assert_eq!(case.uids, decode(&pack, 0), "{}", case.name);
    }
    Ok(())
}

#[test]
fn a_one_uid_block_emits_one_empty_group() -> TestResult {
    let pack = encode(&[7], 256).context("one UID packs")?;
    assert_eq!(1, pack.blocks().len());
    assert_eq!(7, pack.blocks()[0].base());
    assert_eq!(1, pack.blocks()[0].num_uids());
    assert_eq!(vec![0u8; 5], pack.blocks()[0].deltas());
    Ok(())
}

#[test]
fn a_block_closes_where_the_upper_32_bits_change() -> TestResult {
    let pack = encode(&[1, 2, (1 << 32) + 1], 256).context("the list packs")?;
    let bases: Vec<u64> = pack.blocks().iter().map(UidBlock::base).collect();
    assert_eq!(vec![1, (1 << 32) + 1], bases);
    Ok(())
}

#[test]
fn block_size_zero_packs_one_uid_per_block() -> TestResult {
    let mut encoder = Encoder::new(0);
    assert_eq!(0, encoder.block_size());
    for uid in [3, 4, 9] {
        encoder.add(uid);
    }
    let pack = encoder.done().context("the list packs")?;
    assert_eq!(0, pack.block_size());
    assert_eq!(
        vec![3, 4, 9],
        pack.blocks().iter().map(UidBlock::base).collect::<Vec<_>>()
    );
    assert_eq!(vec![3, 4, 9], decode(&pack, 0));
    Ok(())
}

#[test]
fn an_encoder_without_uids_returns_no_pack() {
    assert!(Encoder::new(256).done().is_none());
}

#[test]
fn test_an_encoder_reports_the_block_size_it_was_made_with() -> TestResult {
    // 256, the block size Dgraph's posting lists use, and the pack's.
    let mut encoder = Encoder::new(256);
    assert_eq!(256, encoder.block_size());
    encoder.add(1);
    assert_eq!(256, encoder.done().context("a pack")?.block_size());
    Ok(())
}
