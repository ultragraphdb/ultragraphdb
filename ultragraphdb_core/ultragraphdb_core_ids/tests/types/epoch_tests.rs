/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{Epoch, IdError, IdErrorEnum};
use ultragraphdb_tool_bdd::TestResult;

// An epoch fits in 42 bits (sequencer, "The sequencer closes epochs at its own wall-clock time"): the largest is
// 2^42 − 1 = 4,398,046,511,103, computed by hand.

#[test]
fn test_the_largest_epoch_is_two_to_the_42_minus_one() -> TestResult {
    assert_eq!(Epoch::MAX.get(), 4_398_046_511_103);
    assert_eq!(Epoch::new(4_398_046_511_103)?, Epoch::MAX);
    Ok(())
}

#[test]
fn test_an_epoch_past_42_bits_is_refused() {
    assert_eq!(
        Epoch::new(4_398_046_511_104),
        Err(IdError(IdErrorEnum::EpochRange {
            epoch: 4_398_046_511_104
        }))
    );
    assert_eq!(
        Epoch::new(u64::MAX),
        Err(IdError(IdErrorEnum::EpochRange { epoch: u64::MAX }))
    );
}

#[test]
fn test_epochs_zero_and_one_hold_their_value() -> TestResult {
    assert_eq!(Epoch::new(0)?.get(), 0);
    assert_eq!(Epoch::new(1)?.get(), 1);
    Ok(())
}

#[test]
fn test_the_next_epoch_is_one_above() -> TestResult {
    assert_eq!(Epoch::new(7)?.next()?.get(), 8);
    assert_eq!(Epoch::new(0)?.next()?.get(), 1);
    assert_eq!(Epoch::new(4_398_046_511_102)?.next()?, Epoch::MAX);
    Ok(())
}

#[test]
fn test_no_epoch_follows_the_last() {
    assert_eq!(
        Epoch::MAX.next(),
        Err(IdError(IdErrorEnum::EpochRange {
            epoch: 4_398_046_511_104
        }))
    );
}
