/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The locks of [`crate::Lists`]: std's, and Loom's under `--cfg loom`, which the Loom tests (`tests/loom/`) build the
//! crate with to check the read gate, the writer and the watermark under every interleaving of their threads.

#[cfg(loom)]
pub(crate) use loom::sync::{Condvar, Mutex, MutexGuard};
#[cfg(not(loom))]
pub(crate) use std::sync::{Condvar, Mutex, MutexGuard};
