/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_transactions_commit::{CloseRequest, SettingErrorEnum};

#[test]
fn test_the_bounds() {
    assert_eq!(CloseRequest::MS_MIN, 1);
    assert_eq!(CloseRequest::MS_MAX, 60_000);
}

#[test]
fn test_a_value_at_twice_epoch_ms_max_is_taken() -> TestResult {
    let request = CloseRequest::new(500, 250).context("500 is twice 250")?;
    assert_eq!(request.ms(), 500);
    assert_eq!(request.micros(), 500_000);
    Ok(())
}

#[test]
fn test_the_bounds_are_taken() -> TestResult {
    assert_eq!(CloseRequest::new(1, 0).context("1 ms")?.ms(), 1);
    let most = CloseRequest::new(60_000, 10_000).context("60,000 ms")?;
    // 60,000 ms × 1,000.
    assert_eq!(most.micros(), 60_000_000);
    Ok(())
}

#[test]
fn test_a_value_outside_its_range_is_refused() {
    for value in [0, 60_001, u64::MAX] {
        assert_eq!(
            CloseRequest::new(value, 0).map_err(|e| e.0),
            Err(SettingErrorEnum::CloseRequestRange { value }),
            "{value}"
        );
    }
}

#[test]
fn test_a_value_below_twice_epoch_ms_max_is_refused() {
    assert_eq!(
        CloseRequest::new(499, 250).map_err(|e| e.0),
        Err(SettingErrorEnum::CloseRequestBelowEpochs {
            value: 499,
            epoch_ms_max: 250
        })
    );
    // A range refusal comes first.
    assert_eq!(
        CloseRequest::new(0, 250).map_err(|e| e.0),
        Err(SettingErrorEnum::CloseRequestRange { value: 0 })
    );
}

#[test]
fn test_twice_a_huge_epoch_ms_max_does_not_overflow() {
    assert_eq!(
        CloseRequest::new(60_000, u64::MAX).map_err(|e| e.0),
        Err(SettingErrorEnum::CloseRequestBelowEpochs {
            value: 60_000,
            epoch_ms_max: u64::MAX
        })
    );
}
