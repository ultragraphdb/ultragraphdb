/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The types [`crate::ConnectionBound`]'s count shares across cores: std's, and Loom's under `--cfg loom`, which the
//! Loom tests (`tests/loom/`) build the crate with to check the count under every interleaving of its threads.

#[cfg(loom)]
pub(crate) use loom::sync::Arc;
#[cfg(loom)]
pub(crate) use loom::sync::atomic::{AtomicUsize, Ordering};
#[cfg(not(loom))]
pub(crate) use std::sync::Arc;
#[cfg(not(loom))]
pub(crate) use std::sync::atomic::{AtomicUsize, Ordering};
