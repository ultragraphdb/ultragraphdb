/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The locks and the flag of the changeset core: std's, and Loom's under `--cfg loom`, which the Loom tests
//! (`tests/loom/`) build the crate with to check rounds, drops and catalog loads under every interleaving of their
//! threads.

#[cfg(loom)]
pub(crate) use loom::sync::atomic::{AtomicBool, Ordering};
#[cfg(loom)]
pub(crate) use loom::sync::{Mutex, MutexGuard};
#[cfg(not(loom))]
pub(crate) use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(not(loom))]
pub(crate) use std::sync::{Mutex, MutexGuard};
