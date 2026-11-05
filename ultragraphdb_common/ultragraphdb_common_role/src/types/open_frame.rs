/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// One frame of [`crate::OpenRequests`]'s count, whichever way its connection goes on from here.
#[derive(Debug)]
pub(crate) struct OpenFrame(Arc<AtomicUsize>);

impl OpenFrame {
    /// Counts one more in `open`, until this is dropped.
    pub(crate) fn new(open: Arc<AtomicUsize>) -> Self {
        let _before = open.fetch_add(1, Ordering::AcqRel);
        Self(open)
    }
}

impl Drop for OpenFrame {
    fn drop(&mut self) {
        let _before = self.0.fetch_sub(1, Ordering::AcqRel);
    }
}
