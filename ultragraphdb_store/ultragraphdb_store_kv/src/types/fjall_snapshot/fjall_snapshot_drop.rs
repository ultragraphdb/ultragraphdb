/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::fjall_snapshot::FjallSnapshot;
use crate::utils::lock::lock;

impl Drop for FjallSnapshot {
    /// Closes the snapshot, releasing its hold on the discard timestamp.
    fn drop(&mut self) {
        let mut view = lock(&self.shared.view);
        if let Some(count) = view.readers.get_mut(&self.read_ts) {
            *count -= 1;
            if *count == 0 {
                view.readers.remove(&self.read_ts);
            }
        }
        self.shared.publish_discard(&view);
    }
}
