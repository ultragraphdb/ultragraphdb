/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::TypePrefix;
use ultragraphdb_traits_blocks::{BlockSource, SequenceBlock};

use crate::types::leased_blocks::LeasedBlocks;

impl BlockSource for LeasedBlocks {
    /// The oldest block of `prefix` supplied and not yet taken, `None` when none is: the group unreachable or the
    /// host not yet topped up.
    fn lease(&self, prefix: TypePrefix) -> Option<SequenceBlock> {
        self.lock().get_mut(&prefix.get())?.pop_front()
    }
}
