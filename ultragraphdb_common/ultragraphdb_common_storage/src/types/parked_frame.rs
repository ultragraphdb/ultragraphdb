/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_role::ParkedBytes;

/// A frame copied out of its connection's receive buffer, counted among what that connection holds until it is dropped, waiting for
/// its core or parked, which is what lets the connection read on (`PROTOCOL.md` §11.12, *Parked requests*).
#[derive(Debug)]
pub struct ParkedFrame {
    bytes: Vec<u8>,
    parked: ParkedBytes,
}

impl ParkedFrame {
    /// A copy of `bytes`, counted in `parked`.
    #[must_use]
    pub fn copy(bytes: &[u8], parked: ParkedBytes) -> Self {
        parked.hold(bytes.len());
        Self {
            bytes: bytes.to_vec(),
            parked,
        }
    }

    /// The frame's bytes, header and body.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The frame's bytes, no longer counted.
    #[must_use]
    pub fn into_bytes(mut self) -> Vec<u8> {
        let bytes = std::mem::take(&mut self.bytes);
        self.parked.release(bytes.len());
        bytes
    }
}

impl Drop for ParkedFrame {
    fn drop(&mut self) {
        self.parked.release(self.bytes.len());
    }
}
