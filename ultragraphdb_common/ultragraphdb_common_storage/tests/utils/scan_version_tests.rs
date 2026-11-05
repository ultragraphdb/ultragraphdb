/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_storage::scan_version;
use ultragraphdb_core_ids::{Epoch, Version};
use ultragraphdb_protocol_wire::{ErrorCode, ScanAt};
use ultragraphdb_tool_bdd::{Context, TestResult};

const CLOSED: u64 = 5;

fn closed() -> TestResult<Epoch> {
    Ok(Epoch::new(CLOSED)?)
}

fn in_epoch(epoch: u64, sequence: u32) -> TestResult<u64> {
    Ok(Version::commit(Epoch::new(epoch)?, sequence)?.to_u64())
}

/// With `at` 1 and 4 alike: a version within a closed epoch reads what was applied at or below it, on an idle partition too.
#[test]
fn test_a_version_in_a_closed_epoch_reads_at_or_below_the_watermark() -> TestResult {
    let watermark = in_epoch(3, 2)?;
    let bound = Version::bound_of(closed()?);
    for at in [ScanAt::ReadVersion, ScanAt::Named] {
        assert_eq!(
            scan_version(at, bound, closed()?, watermark).ok(),
            Some((watermark, CLOSED))
        );
        let older = in_epoch(2, 4)?;
        assert_eq!(
            scan_version(at, older, closed()?, watermark).ok(),
            Some((older, 2))
        );
    }
    Ok(())
}

/// Epoch 0's version bound counts as closed on a partition that has closed nothing.
#[test]
fn test_a_version_of_epoch_0_reads_before_any_close() -> TestResult {
    let bound = Version::bound_of(Epoch::ZERO);
    let answer = scan_version(ScanAt::Named, bound, Epoch::ZERO, 0).ok();
    assert_eq!(answer, Some((0, 0)));
    Ok(())
}

/// Any other version above the watermark names the watermark, and one at or below it in an open epoch is read.
#[test]
fn test_a_version_above_the_watermark_in_an_open_epoch_is_refused() -> TestResult {
    let watermark = in_epoch(CLOSED + 1, 2)?;
    for at in [ScanAt::ReadVersion, ScanAt::Named] {
        let refused = scan_version(at, watermark + 2, closed()?, watermark).err();
        let refused = refused.context("a refusal")?;
        assert_eq!(refused.code(), ErrorCode::AheadOfWatermark);
        assert_eq!(refused.detail(), watermark);
        assert_eq!(
            scan_version(at, watermark, closed()?, watermark).ok(),
            Some((watermark, CLOSED + 1))
        );
    }
    Ok(())
}

/// `at` 2 reads the watermark, naming the higher of the closed epoch and the version's.
#[test]
fn test_the_latest_applied_version_names_the_higher_epoch() -> TestResult {
    let ahead = in_epoch(CLOSED + 1, 2)?;
    assert_eq!(
        scan_version(ScanAt::Latest, 0, closed()?, ahead).ok(),
        Some((ahead, CLOSED + 1))
    );
    let behind = in_epoch(CLOSED - 2, 2)?;
    assert_eq!(
        scan_version(ScanAt::Latest, 0, closed()?, behind).ok(),
        Some((behind, CLOSED))
    );
    Ok(())
}

/// `at` 3 reads the closed epoch's version bound, never above the watermark.
#[test]
fn test_the_latest_closed_version_names_the_closed_epoch() -> TestResult {
    let bound = Version::bound_of(closed()?);
    let idle = in_epoch(CLOSED - 2, 2)?;
    assert_eq!(
        scan_version(ScanAt::LatestClosed, 0, closed()?, idle).ok(),
        Some((idle, CLOSED))
    );
    let ahead = in_epoch(CLOSED + 1, 2)?;
    assert_eq!(
        scan_version(ScanAt::LatestClosed, 0, closed()?, ahead).ok(),
        Some((bound, CLOSED))
    );
    Ok(())
}
