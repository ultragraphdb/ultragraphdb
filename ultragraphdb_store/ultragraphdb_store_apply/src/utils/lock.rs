/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::{Mutex, MutexGuard, PoisonError};

/// Locks `m`, taking the data of a lock a panicking thread left poisoned: every critical section here leaves its
/// data whole before anything in it can panic.
pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}
