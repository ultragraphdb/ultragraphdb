/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// A size two ways: the bytes the files hold and the bytes allocated to them, which `du` reports.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Size {
    apparent: u64,
    allocated: u64,
}

impl Size {
    /// A size of `apparent` bytes held and `allocated` bytes allocated.
    pub fn new(apparent: u64, allocated: u64) -> Self {
        Self {
            apparent,
            allocated,
        }
    }

    /// The bytes the files hold.
    pub fn apparent(&self) -> u64 {
        self.apparent
    }

    /// The bytes allocated to the files, as `du` reports them.
    pub fn allocated(&self) -> u64 {
        self.allocated
    }

    /// The sum of this size and `other`, each way.
    pub fn plus(self, other: Size) -> Size {
        Size::new(
            self.apparent + other.apparent,
            self.allocated + other.allocated,
        )
    }
}
