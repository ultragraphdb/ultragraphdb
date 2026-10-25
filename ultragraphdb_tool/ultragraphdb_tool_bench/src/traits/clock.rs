/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::time::Duration;

/// The time since a run started: the wall clock in a run, a clock moved by hand in a test.
pub trait Clock {
    fn elapsed(&self) -> Duration;
}
