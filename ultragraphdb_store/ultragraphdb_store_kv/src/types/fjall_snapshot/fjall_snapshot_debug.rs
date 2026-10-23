/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::types::fjall_snapshot::FjallSnapshot;

impl fmt::Debug for FjallSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FjallSnapshot")
            .field("read_ts", &self.read_ts)
            .field("banned", &self.banned)
            .finish_non_exhaustive()
    }
}
