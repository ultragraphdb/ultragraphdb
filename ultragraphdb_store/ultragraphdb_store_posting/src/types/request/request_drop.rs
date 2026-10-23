/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::request::Request;
use crate::utils::lock::lock;

impl<S, C> Drop for Request<'_, S, C> {
    /// Closes the request's read, which no longer bounds the discard timestamp.
    fn drop(&mut self) {
        lock(self.lists.state()).ts.close_read(self.read_ts);
    }
}
