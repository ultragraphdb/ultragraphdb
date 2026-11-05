/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

use compio::runtime::JoinHandle;

use crate::types::open_requests::OpenRequests;

/// How often a stop looks at what is still open.
const POLL: Duration = Duration::from_millis(5);

/// The first half of a role's stop: cancelling `accepting`, the tasks accepting its connections, closes their listeners, and `open`
/// then has until `limit` to finish, its answers written. A peer reading no answer holds a stop no longer than `limit`. Returns how
/// many are still open by then, which end with the process.
pub async fn finish_open<T>(
    accepting: Vec<JoinHandle<T>>,
    open: &OpenRequests,
    limit: Duration,
) -> usize {
    drop(accepting);
    tracing::info!(open = open.count(), "stopped accepting");
    let looks = limit.as_millis() / POLL.as_millis();
    for _ in 0..looks {
        if open.count() == 0 {
            break;
        }
        compio::time::sleep(POLL).await;
    }
    let left = open.count();
    if left > 0 {
        tracing::warn!(left, "requests still open at the limit of a stop");
    }
    left
}
