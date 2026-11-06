/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use super::ComputeCore;

/// A dial under way: dropped, whoever waits on it dials for itself.
pub(super) struct DialUnderWay<'a> {
    pub(crate) core: &'a ComputeCore,
    pub(crate) partition: u32,
}

impl Drop for DialUnderWay<'_> {
    fn drop(&mut self) {
        drop(self.core.dialling.borrow_mut().remove(&self.partition));
    }
}
