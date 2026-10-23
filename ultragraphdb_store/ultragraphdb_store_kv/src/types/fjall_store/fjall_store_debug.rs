/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;
use std::sync::atomic::Ordering;

use crate::types::fjall_store::FjallStore;
use crate::utils::lock::lock;

impl fmt::Debug for FjallStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let shared = &self.shared;
        f.debug_struct("FjallStore")
            .field("durability", &shared.durability)
            .field("applied_ts", &shared.applied_ts.load(Ordering::Acquire))
            .field("poisoned", &shared.poisoned.load(Ordering::Acquire))
            .field("view", &*lock(&shared.view))
            .finish_non_exhaustive()
    }
}
