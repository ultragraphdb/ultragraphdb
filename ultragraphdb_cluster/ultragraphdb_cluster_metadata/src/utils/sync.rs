/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The types [`crate::LeasedBlocks`] shares between a partition's store and its host: std's, and Loom's under
//! `--cfg loom`, which the Loom tests (`tests/loom/`) build the crate with to check the queue under every interleaving
//! of its threads.

#[cfg(loom)]
pub(crate) use loom::sync::{Arc, Mutex, MutexGuard};
#[cfg(not(loom))]
pub(crate) use std::sync::{Arc, Mutex, MutexGuard};
