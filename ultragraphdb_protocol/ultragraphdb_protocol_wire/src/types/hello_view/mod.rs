/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod hello_view_body_view;

/// The body of a received Hello frame, read in place; see [`crate::Hello`] for its layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HelloView<'a> {
    protocol_min: u16,
    protocol_max: u16,
    release: u32,
    max_frame: u32,
    client: &'a str,
}

impl<'a> HelloView<'a> {
    /// The lowest protocol version the client speaks.
    pub fn protocol_min(&self) -> u16 {
        self.protocol_min
    }

    /// The highest protocol version the client speaks.
    pub fn protocol_max(&self) -> u16 {
        self.protocol_max
    }

    /// The client's release.
    pub fn release(&self) -> u32 {
        self.release
    }

    /// The largest frame the sender takes, in bytes.
    pub fn max_frame(&self) -> u32 {
        self.max_frame
    }

    /// The client's name.
    pub fn client(&self) -> &'a str {
        self.client
    }
}
