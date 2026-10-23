/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::types::moka_list_cache::MokaListCache;

impl<V> fmt::Debug for MokaListCache<V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MokaListCache")
            .field("entries", &self.cache.entry_count())
            .field("weight", &self.cache.weighted_size())
            .finish()
    }
}
