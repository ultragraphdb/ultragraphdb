/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::{Mutex, MutexGuard};

/// The guard of `mutex`, a poisoned lock's included: the state it guards holds no invariant a panic can break
/// halfway, since every change to it is one assignment.
pub fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
