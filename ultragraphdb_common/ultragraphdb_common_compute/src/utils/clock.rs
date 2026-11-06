/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Instant;

/// The time now.
#[expect(
    clippy::disallowed_methods,
    reason = "a read's deadline in the shell, which no simulation replays"
)]
pub(crate) fn now() -> Instant {
    Instant::now()
}
