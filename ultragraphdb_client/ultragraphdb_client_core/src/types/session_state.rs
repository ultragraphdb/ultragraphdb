/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// A session's request numbers: the next commit's, whether a commit is in flight, and the request in doubt after its
/// retries ran out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SessionState {
    pub next_request: u64,
    pub in_flight: bool,
    pub in_doubt: Option<u64>,
}
