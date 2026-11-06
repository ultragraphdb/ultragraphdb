/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_role::ParkedBytes;

/// A frame on its way to a role, its bytes counted against its client connection's bound until it is let go, whichever way that
/// goes (`PROTOCOL.md` §11.12, *Frames on their way*).
#[derive(Debug)]
pub(crate) struct CountedFrame {
    bytes: Vec<u8>,
    count: Option<ParkedBytes>,
    counted: usize,
}

impl CountedFrame {
    /// `bytes`, which `count` counts from here on.
    pub(crate) fn new(bytes: Vec<u8>, count: &ParkedBytes) -> Self {
        let counted = bytes.len();
        count.hold(counted);
        Self {
            bytes,
            count: Some(count.clone()),
            counted,
        }
    }

    /// `bytes`, which nothing counts: a frame whose bound lies elsewhere.
    pub(crate) fn uncounted(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            count: None,
            counted: 0,
        }
    }

    /// The frame's bytes.
    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The frame's bytes, to stamp.
    pub(crate) fn bytes_mut(&mut self) -> &mut Vec<u8> {
        &mut self.bytes
    }

    /// The frame's bytes, no longer counted.
    pub(crate) fn into_bytes(mut self) -> Vec<u8> {
        std::mem::take(&mut self.bytes)
    }
}

impl Drop for CountedFrame {
    fn drop(&mut self) {
        if let Some(count) = &self.count {
            count.release(self.counted);
        }
    }
}
