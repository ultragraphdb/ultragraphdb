/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::{Mutex, MutexGuard, PoisonError};

/// Locks `mutex`, taking its value even when a thread panicked while holding it: the store keeps no half-written
/// state under its locks, since every write is one fjall batch and the in-memory state follows it.
pub fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
