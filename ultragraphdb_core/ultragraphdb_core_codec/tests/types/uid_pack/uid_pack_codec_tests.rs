/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A pack's bytes against the golden packs `formats.py` wrote from the format's specification.

use ultragraphdb_core_codec::{CodecError, CodecErrorEnum, UidPack};
use ultragraphdb_core_ids::IdError;
use ultragraphdb_tool_bdd::{Context, TestResult};

use crate::codec_tests::{IdListCase, id_list_cases, packed, sequences, unhex};

/// The golden cases that pack, with their bytes.
fn golden_packs() -> TestResult<Vec<(IdListCase, Vec<u8>)>> {
    id_list_cases()?
        .into_iter()
        .filter_map(|case| {
            let bytes = case.pack.as_deref().map(unhex)?;
            Some(bytes.map(|b| (case, b)))
        })
        .collect()
}

/// The bytes of sequences 5, 6 and 7 in blocks of 2: the version at 0, the block size at 1, the block count at 5, the
/// first block's first ID at 9, its count at 25, its deltas' length at 29 and its delta at 33, the second block's first
/// ID at 37, its count at 53 and its deltas' length at 57; 61 bytes.
fn three() -> TestResult<Vec<u8>> {
    Ok(packed(&sequences(5..=7)?, 2)?.encode())
}

#[test]
fn test_a_pack_encodes_to_its_golden_bytes() -> TestResult {
    let packs = golden_packs()?;
    assert!(packs.len() >= 15);
    for (case, bytes) in packs {
        let pack = packed(&case.ids()?, case.block_size)?;
        assert_eq!(pack.encode(), bytes, "{}", case.name);
        assert_eq!(pack.encoded_len(), bytes.len(), "{}", case.name);
    }
    Ok(())
}

#[test]
fn test_golden_bytes_decode_to_their_pack_and_back() -> TestResult {
    for (case, bytes) in golden_packs()? {
        let pack = UidPack::decode(&bytes)?;
        assert_eq!(
            pack,
            packed(&case.ids()?, case.block_size)?,
            "{}",
            case.name
        );
        assert_eq!(pack.encode(), bytes, "{}", case.name);
    }
    Ok(())
}

#[test]
fn test_a_pack_appended_to_a_buffer_follows_what_was_there() -> TestResult {
    let pack = packed(&sequences(5..=7)?, 2)?;
    let mut buffer = b"before".to_vec();
    pack.encode_into(&mut buffer);
    assert_eq!(buffer.get(..6), Some(&b"before"[..]));
    assert_eq!(buffer.get(6..), Some(&three()?[..]));
    Ok(())
}

#[test]
fn test_the_empty_pack_is_nine_bytes() -> TestResult {
    // The version, a block size of 0 and a block count of 0.
    let bytes = [1u8, 0, 0, 0, 0, 0, 0, 0, 0];
    assert_eq!(UidPack::default().encode(), bytes);
    assert_eq!(UidPack::default().encoded_len(), 9);
    assert_eq!(UidPack::decode(&bytes), Ok(UidPack::default()));
    Ok(())
}

#[test]
fn test_every_proper_prefix_is_truncated() -> TestResult {
    for (case, bytes) in golden_packs()?.into_iter().take(6) {
        for len in 0..bytes.len() {
            let result = UidPack::decode(bytes.get(..len).context("a prefix")?).map_err(|e| e.0);
            assert!(
                matches!(result, Err(CodecErrorEnum::Truncated { .. })),
                "{} cut at {len}: {result:?}",
                case.name
            );
        }
    }
    // The fields the cut lands in: the version, the block count, a first ID, a delta.
    let bytes = three()?;
    for (len, offset) in [(0, 0), (7, 5), (20, 9), (34, 33)] {
        assert_eq!(
            UidPack::decode(bytes.get(..len).context("a prefix")?),
            Err(CodecError::truncated(offset)),
            "{len}"
        );
    }
    Ok(())
}

#[test]
fn test_an_unknown_version_is_refused() -> TestResult {
    for version in [0u8, 2, 0xFF] {
        let mut bytes = three()?;
        if let Some(first) = bytes.first_mut() {
            *first = version;
        }
        assert_eq!(
            UidPack::decode(&bytes),
            Err(CodecError::unknown_version(version))
        );
    }
    Ok(())
}

#[test]
fn test_bytes_after_the_last_block_are_refused() -> TestResult {
    let mut bytes = three()?;
    bytes.push(0);
    assert_eq!(UidPack::decode(&bytes), Err(CodecError::trailing_bytes(61)));
    Ok(())
}

#[test]
fn test_a_blocks_refusal_is_the_packs() -> TestResult {
    // The first block's first ID with its last reserved byte set.
    let mut reserved = three()?;
    *reserved.get_mut(24).context("a reserved byte")? = 1;
    assert_eq!(
        UidPack::decode(&reserved),
        Err(CodecError::invalid_id(0, IdError::reserved_bits(1)))
    );
    // The first block's delta, 80 80 80 08, cut to 80 80 80 80: its last byte carries the continuation bit.
    let mut cut = three()?;
    *cut.get_mut(36).context("the delta's last byte")? = 0x80;
    assert_eq!(UidPack::decode(&cut), Err(CodecError::truncated_varint(0)));
    Ok(())
}

#[test]
fn test_blocks_out_of_order_are_refused() -> TestResult {
    // The second block's first ID, sequence 7, rewritten as sequence 6, the first block's last.
    let mut bytes = three()?;
    let six = sequences(6..=6)?.first().context("an ID")?.to_be_bytes();
    bytes
        .get_mut(37..53)
        .context("the second block's first ID")?
        .copy_from_slice(&six);
    assert_eq!(
        UidPack::decode(&bytes),
        Err(CodecError::blocks_out_of_order(1))
    );
    Ok(())
}

#[test]
fn test_a_count_or_length_at_its_largest_runs_out_of_bytes() -> TestResult {
    // The block count and the first block's deltas' length at 2^32 - 1: each overruns the bytes rather than allocating.
    for (at, offset) in [(5usize, 61usize), (29, 33)] {
        let mut bytes = three()?;
        bytes
            .get_mut(at..at + 4)
            .context("a count")?
            .copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(
            UidPack::decode(&bytes),
            Err(CodecError::truncated(offset)),
            "{at}"
        );
    }
    Ok(())
}

#[test]
fn test_the_fuzz_seeds_are_the_golden_packs() -> TestResult {
    let dir = ultragraphdb_tool_bdd::testdata_dir()?.join("fuzz_seeds/uid_pack_decode");
    let seeds = ultragraphdb_tool_bdd::fuzz_regressions_in(&dir)?;
    assert_eq!(
        seeds.len(),
        golden_packs()?.len(),
        "one seed per golden pack"
    );
    for (name, bytes) in seeds {
        let pack = UidPack::decode(&bytes)
            .map_err(|e| format!("{name}: {e}"))
            .context("a fuzz seed decodes")?;
        assert_eq!(pack.encode(), bytes, "{name}");
    }
    Ok(())
}
