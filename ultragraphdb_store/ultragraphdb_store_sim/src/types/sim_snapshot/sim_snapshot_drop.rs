/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::op_record::OpRecord;
use crate::types::sim_snapshot::SimSnapshot;
use crate::utils::lock::lock;

impl Drop for SimSnapshot {
    /// Closes the snapshot, releasing its hold on the discard timestamp unless the store has been reopened since.
    fn drop(&mut self) {
        let mut inner = lock(&self.shared);
        if inner.generation != self.generation {
            return;
        }
        if let Some(count) = inner.readers.get_mut(&self.read_ts) {
            *count -= 1;
            if *count == 0 {
                inner.readers.remove(&self.read_ts);
            }
        }
        inner.log.push(OpRecord::SnapshotClosed { id: self.id });
    }
}
