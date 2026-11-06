/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

use compio::runtime::Runtime;
use ultragraphdb_common_role::ParkedBytes;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

const BOUND: usize = 8;
const SHORT: Duration = Duration::from_millis(5);

/// Whether `room` resolves within a short wait.
async fn resolves(parked: &ParkedBytes) -> bool {
    compio::time::timeout(SHORT, parked.room()).await.is_ok()
}

/// The count holds what is held less what is let go, and room stands below the bound.
#[test]
fn test_room_stands_below_the_bound() -> TestResult {
    Runtime::new()?.block_on(async {
        let parked = ParkedBytes::new(BOUND);
        assert!(resolves(&parked).await, "nothing held");
        parked.hold(BOUND - 1);
        assert_eq!(parked.count(), BOUND - 1);
        assert!(resolves(&parked).await, "one byte below the bound");
        parked.hold(1);
        assert!(!resolves(&parked).await, "at the bound");
        parked.release(1);
        assert!(resolves(&parked).await, "released below the bound");
        Ok(())
    })
}

/// A release wakes a wait already asleep, and the clone counts in one place.
#[test]
fn test_a_release_wakes_a_waiter() -> TestResult {
    Runtime::new()?.block_on(async {
        let parked = ParkedBytes::new(BOUND);
        parked.hold(BOUND);
        let held = parked.clone();
        let waiter = compio::runtime::spawn(async move {
            held.room().await;
        });
        compio::time::sleep(SHORT).await;
        parked.release(BOUND);
        compio::time::timeout(Duration::from_secs(5), waiter)
            .await
            .map_err(|_| TestError::new("no wake within 5 s of a release"))?
            .map_err(|_| TestError::new("the waiter panicked"))
            .context("the waiter")
    })
}
