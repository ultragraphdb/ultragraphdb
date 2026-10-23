/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};

/// Locks `mutex`, taking its value even when a thread panicked while holding it: every critical section of this
/// crate leaves its state whole before it can fail, so a panic leaves nothing half-written.
pub fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Waits on `changed`, as [`lock`] takes a guard back after a panic.
pub fn wait<'a, T>(changed: &Condvar, guard: MutexGuard<'a, T>) -> MutexGuard<'a, T> {
    changed.wait(guard).unwrap_or_else(PoisonError::into_inner)
}
