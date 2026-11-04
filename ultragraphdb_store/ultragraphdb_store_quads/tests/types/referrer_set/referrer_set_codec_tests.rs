/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The referrer set's value, version byte 1: the byte 1, the member count as a u32 big-endian, then each member sorted
//! by group as its group u32, epoch u64 and generation u64, big-endian. Every expected value is written out by hand.

use std::num::NonZeroU64;

use ultragraphdb_core_ids::{BucketGroup, Epoch};
use ultragraphdb_store_quads::{QuadErrorEnum, ReferrerMember, ReferrerSet};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

const KEY: &[u8] = b"referrers of o";

/// Group 3 at epoch 8, generation 1, and group 7 at epoch 12, generation 4.
const TWO: [u8; 45] = [
    1, 0, 0, 0, 2, // version, count
    0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 8, 0, 0, 0, 0, 0, 0, 0, 1, // group 3
    0, 0, 0, 7, 0, 0, 0, 0, 0, 0, 0, 12, 0, 0, 0, 0, 0, 0, 0, 4, // group 7
];

fn member(g: u32, e: u64, n: u64) -> TestResult<ReferrerMember> {
    Ok(ReferrerMember::new(
        BucketGroup::new(g)?,
        Epoch::new(e)?,
        NonZeroU64::new(n).context("a generation is at least 1")?,
    ))
}

fn two() -> TestResult<ReferrerSet> {
    let mut set = ReferrerSet::new();
    for m in [member(7, 12, 4)?, member(3, 8, 1)?] {
        set.set(m.group(), m.generation(), m.epoch());
    }
    Ok(set)
}

/// The reason `value` is refused as corruption naming [`KEY`].
fn refused(value: &[u8]) -> TestResult<String> {
    match ReferrerSet::decode(KEY, value) {
        Err(e) => {
            let text = format!("{e:?}");
            let QuadErrorEnum::Corrupted { key, reason } = e.0 else {
                return Err(TestError::new(format!("not corruption: {text}")));
            };
            assert_eq!(key, KEY, "the refusal names the key");
            Ok(reason)
        }
        Ok(set) => Err(TestError::new(format!("decoded {set:?}"))),
    }
}

#[test]
fn test_the_empty_set_encodes_as_its_version_and_a_zero_count() -> TestResult {
    let mut out = Vec::new();
    ReferrerSet::new().encode(&mut out);
    assert_eq!(out, vec![1, 0, 0, 0, 0]);
    assert_eq!(ReferrerSet::decode(KEY, &out)?, ReferrerSet::new());
    Ok(())
}

#[test]
fn test_members_encode_sorted_by_group_and_decode_back() -> TestResult {
    let mut out = Vec::new();
    two()?.encode(&mut out);
    assert_eq!(out, TWO.to_vec());
    let decoded = ReferrerSet::decode(KEY, &TWO)?;
    assert_eq!(decoded.members(), &[member(3, 8, 1)?, member(7, 12, 4)?]);
    Ok(())
}

#[test]
fn test_one_member_at_the_largest_group_epoch_and_generation_round_trips() -> TestResult {
    let mut set = ReferrerSet::new();
    let last = member(BucketGroup::COUNT - 1, Epoch::MAX.get(), u64::MAX)?;
    set.set(last.group(), last.generation(), last.epoch());
    let mut out = Vec::new();
    set.encode(&mut out);
    let expected: Vec<u8> = [
        vec![1, 0, 0, 0, 1],
        vec![0, 3, 0xFF, 0xFF],
        vec![0, 0, 0x03, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF],
        vec![0xFF; 8],
    ]
    .concat();
    assert_eq!(out, expected);
    assert_eq!(ReferrerSet::decode(KEY, &out)?, set);
    Ok(())
}

#[test]
fn test_encode_appends_to_what_the_buffer_holds() {
    let mut out = vec![0xAA];
    ReferrerSet::new().encode(&mut out);
    assert_eq!(out, vec![0xAA, 1, 0, 0, 0, 0]);
}

#[test]
fn test_a_value_without_its_version_byte_one_is_refused() -> TestResult {
    let _empty: String = refused(&[])?;
    let mut other = TWO.to_vec();
    other[0] = 2;
    let _version: String = refused(&other)?;
    let _zero: String = refused(&[0, 0, 0, 0, 0])?;
    Ok(())
}

#[test]
fn test_a_truncated_value_is_refused() -> TestResult {
    // No full count, a count above the members held, and a member cut short.
    let _count: String = refused(&[1, 0, 0])?;
    let _missing: String = refused(&[1, 0, 0, 0, 1])?;
    let _short: String = refused(TWO.get(..TWO.len() - 1).context("a prefix")?)?;
    Ok(())
}

#[test]
fn test_a_value_longer_than_its_members_is_refused() -> TestResult {
    let mut longer = TWO.to_vec();
    longer.push(0);
    let _trailing: String = refused(&longer)?;
    Ok(())
}

#[test]
fn test_unsorted_or_repeated_groups_are_refused() -> TestResult {
    let mut unsorted = TWO.to_vec();
    // Group 7 first, then group 3.
    unsorted.swap(8, 28);
    let _unsorted: String = refused(&unsorted)?;
    let mut repeated = TWO.to_vec();
    repeated[28] = 3;
    let _repeated: String = refused(&repeated)?;
    Ok(())
}

#[test]
fn test_a_zero_generation_is_refused() -> TestResult {
    let mut zero = TWO.to_vec();
    zero[24] = 0;
    let _zero: String = refused(&zero)?;
    Ok(())
}

#[test]
fn test_a_group_or_epoch_out_of_range_is_refused() -> TestResult {
    // Group 2^18 and epoch 2^42, one past each range.
    let group: Vec<u8> = [
        vec![1, 0, 0, 0, 1],
        vec![0, 4, 0, 0],
        vec![0, 0, 0, 0, 0, 0, 0, 1],
        vec![0, 0, 0, 0, 0, 0, 0, 1],
    ]
    .concat();
    let _group: String = refused(&group)?;
    let epoch: Vec<u8> = [
        vec![1, 0, 0, 0, 1],
        vec![0, 0, 0, 1],
        vec![0, 0, 0x04, 0, 0, 0, 0, 0],
        vec![0, 0, 0, 0, 0, 0, 0, 1],
    ]
    .concat();
    let _epoch: String = refused(&epoch)?;
    Ok(())
}

#[test]
fn test_a_count_past_what_memory_could_hold_is_refused_without_allocating_it() -> TestResult {
    let _huge: String = refused(&[1, 0xFF, 0xFF, 0xFF, 0xFF])?;
    Ok(())
}

/// The seeds of `referrer_set_decode`, written from the format's text by `testdata/referrer_set_seeds.py`: one named
/// `malformed_*` is refused as corrupted, naming the key, and any other decodes and encodes back to its bytes.
#[test]
fn test_every_fuzz_seed_decodes_as_its_name_says() -> TestResult {
    let dir = ultragraphdb_tool_bdd::testdata_dir()?
        .join("fuzz_seeds")
        .join("referrer_set_decode");
    let seeds = ultragraphdb_tool_bdd::fuzz_regressions_in(&dir)?;
    assert_eq!(seeds.len(), 16);
    for (name, bytes) in seeds {
        match ReferrerSet::decode(b"key", &bytes) {
            Ok(set) => {
                assert!(!name.starts_with("malformed"), "{name} accepted");
                let mut encoded = Vec::new();
                set.encode(&mut encoded);
                assert_eq!(encoded, bytes, "{name}");
            }
            Err(refusal) => {
                assert!(name.starts_with("malformed"), "{name}: {refusal}");
                assert!(
                    matches!(&refusal.0, QuadErrorEnum::Corrupted { key, .. } if key == b"key"),
                    "{name}: {refusal}"
                );
            }
        }
    }
    Ok(())
}
