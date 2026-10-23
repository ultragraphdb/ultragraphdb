/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::cache_miss::CacheMiss;
use crate::utils::lock::lock;

impl Drop for CacheMiss<'_> {
    /// Ends the fill: once no fill is under way, the commits written since need not be remembered.
    fn drop(&mut self) {
        let mut state = lock(self.state);
        if let Some(count) = state.fills.get_mut(&self.base) {
            *count -= 1;
            if *count == 0 {
                state.fills.remove(&self.base);
            }
        }
        if state.fills.is_empty() {
            state.touched.clear();
        }
    }
}
