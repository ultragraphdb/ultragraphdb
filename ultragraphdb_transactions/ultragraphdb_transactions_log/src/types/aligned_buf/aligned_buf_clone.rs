/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::aligned_buf::AlignedBuf;

/// A derived clone would copy the vector to a new allocation and keep the old offset, which is aligned only by luck.
impl Clone for AlignedBuf {
    fn clone(&self) -> Self {
        Self::from_slice(self.as_slice())
    }
}
