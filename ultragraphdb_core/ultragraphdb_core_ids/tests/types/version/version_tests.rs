/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{Epoch, IdError, IdErrorEnum, Version};
use ultragraphdb_tool_bdd::TestResult;

// A version is epoch << 22 | sequence; expected numbers by hand: 10 << 22 = 41,943,040, 2^22 − 2 = 4,194,302.

fn commit(epoch: u64, sequence: u32) -> TestResult<Version> {
    Ok(Version::commit(Epoch::new(epoch)?, sequence)?)
}

#[test]
fn test_a_version_carries_the_epoch_in_its_upper_bits() -> TestResult {
    let version = commit(10, 0)?;
    assert_eq!(version.to_u64(), 41_943_040);
    assert_eq!(version.epoch(), Epoch::new(10)?);
    assert_eq!(version.sequence(), 0);
    assert!(version.is_commit());
    Ok(())
}

#[test]
fn test_versions_increase_within_an_epoch() -> TestResult {
    assert_eq!(commit(10, 2)?.to_u64(), 41_943_042);
    assert_eq!(commit(10, 4)?.to_u64(), 41_943_044);
    assert!(commit(10, 2)? < commit(10, 4)?);
    assert_eq!(commit(10, 4)?.sequence(), 4);
    Ok(())
}

#[test]
fn test_the_last_commit_of_an_epoch_is_below_the_next_epochs_first() -> TestResult {
    assert_eq!(Version::COMMIT_SEQUENCE_MAX, 4_194_302);
    let last = commit(10, 4_194_302)?;
    assert_eq!(last.to_u64(), 41_943_040 + 4_194_302);
    assert!(last < commit(11, 0)?);
    assert_eq!(last.epoch(), Epoch::new(10)?);
    Ok(())
}

#[test]
fn test_the_largest_commit_version() -> TestResult {
    let version = Version::commit(Epoch::MAX, Version::COMMIT_SEQUENCE_MAX)?;
    assert_eq!(version.to_u64(), 0xFFFF_FFFF_FFFF_FFFE);
    assert_eq!(
        version.to_be_bytes(),
        [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFE]
    );
    Ok(())
}

#[test]
fn test_an_odd_commit_sequence_is_refused() -> TestResult {
    let epoch = Epoch::new(10)?;
    assert_eq!(
        Version::commit(epoch, 1),
        Err(IdError(IdErrorEnum::OddCommitSequence { sequence: 1 }))
    );
    // 2^22 − 1 is odd and past the range: oddness is named first.
    assert_eq!(
        Version::commit(epoch, 4_194_303),
        Err(IdError(IdErrorEnum::OddCommitSequence {
            sequence: 4_194_303
        }))
    );
    Ok(())
}

#[test]
fn test_a_commit_sequence_past_22_bits_is_refused() -> TestResult {
    let epoch = Epoch::new(10)?;
    assert_eq!(
        Version::commit(epoch, 4_194_304),
        Err(IdError(IdErrorEnum::CommitSequenceRange {
            sequence: 4_194_304
        }))
    );
    assert_eq!(
        Version::commit(epoch, u32::MAX - 1),
        Err(IdError(IdErrorEnum::CommitSequenceRange {
            sequence: u32::MAX - 1
        }))
    );
    Ok(())
}

#[test]
fn test_every_number_is_a_version() -> TestResult {
    let all_ones = Version::from_u64(u64::MAX);
    assert_eq!(all_ones.epoch(), Epoch::MAX);
    assert_eq!(all_ones.sequence(), 4_194_303);
    assert!(!all_ones.is_commit());
    let zero = Version::from_u64(0);
    assert_eq!(
        (zero.epoch(), zero.sequence(), zero.is_commit()),
        (Epoch::new(0)?, 0, true)
    );
    let one = Version::from_u64(1);
    assert_eq!(
        (one.epoch(), one.sequence(), one.is_commit()),
        (Epoch::new(0)?, 1, false)
    );
    // A rollup's version, one below a commit's: epoch 10, sequence 3.
    let rollup = Version::from_u64(41_943_043);
    assert_eq!((rollup.epoch(), rollup.sequence()), (Epoch::new(10)?, 3));
    Ok(())
}

#[test]
fn test_a_version_is_its_8_big_endian_bytes() -> TestResult {
    // 1 << 22 | 2 = 0x400002.
    let version = commit(1, 2)?;
    let bytes = [0, 0, 0, 0, 0, 0x40, 0x00, 0x02];
    assert_eq!(version.to_be_bytes(), bytes);
    assert_eq!(Version::decode(&bytes)?, version);
    assert_eq!(Version::decode(&[0xFF; 8])?, Version::from_u64(u64::MAX));
    assert_eq!(
        Version::from_u64(0x0102_0304_0506_0708).to_u64(),
        0x0102_0304_0506_0708
    );
    Ok(())
}

#[test]
fn test_bytes_of_another_length_are_refused() {
    for len in [0usize, 7, 9, 16] {
        assert_eq!(
            Version::decode(&vec![0x01; len]),
            Err(IdError(IdErrorEnum::Length {
                what: "a version",
                expected: 8,
                found: len,
            })),
            "{len} bytes"
        );
    }
}

#[test]
fn test_versions_order_by_epoch_then_sequence() -> TestResult {
    assert!(commit(1, 4_194_302)? < commit(2, 0)?);
    assert!(Version::from_u64(41_943_043) < commit(10, 4)?);
    assert!(Version::from_u64(41_943_043) > commit(10, 2)?);
    Ok(())
}
