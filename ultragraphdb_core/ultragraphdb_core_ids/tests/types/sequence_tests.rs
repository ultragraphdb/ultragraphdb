/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{IdError, IdErrorEnum, Sequence};
use ultragraphdb_tool_bdd::TestResult;

#[test]
fn test_sequence_zero_is_refused() {
    assert_eq!(Sequence::new(0), Err(IdError(IdErrorEnum::ZeroSequence)));
}

#[test]
fn test_sequences_one_and_the_largest_hold_their_value() -> TestResult {
    assert_eq!(Sequence::new(1)?.get(), 1);
    assert_eq!(Sequence::new(u64::MAX)?.get(), u64::MAX);
    assert_eq!(Sequence::new(42)?.get(), 42);
    Ok(())
}

#[test]
fn test_sequences_order_as_numbers() -> TestResult {
    assert!(Sequence::new(1)? < Sequence::new(2)?);
    assert!(Sequence::new(u64::MAX)? > Sequence::new(1 << 63)?);
    Ok(())
}
