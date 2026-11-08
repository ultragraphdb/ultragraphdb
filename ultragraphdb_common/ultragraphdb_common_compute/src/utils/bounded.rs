/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::future::Future;
use std::time::Duration;

use ultragraphdb_protocol_wire::ErrorReply;

use crate::utils::refusals::unavailable;

/// `work`'s outcome, or code 17 for a peer hung past `wait`, naming `what` compute waited for.
/// A dropped future drops `work` with it.
pub(crate) async fn bounded<T>(
    wait: Duration,
    what: impl FnOnce() -> String,
    work: impl Future<Output = Result<T, ErrorReply>>,
) -> Result<T, ErrorReply> {
    compio::time::timeout(wait, work).await.unwrap_or_else(|_| {
        Err(unavailable(format!(
            "{} did not come within {wait:?}",
            what()
        )))
    })
}
