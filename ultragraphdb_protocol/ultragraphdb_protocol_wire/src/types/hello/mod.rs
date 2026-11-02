/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod hello_body_builder;

/// A client opens a connection: the protocols it speaks, its release, the largest frame it takes and its name. The
/// cluster it connects to is the frame header's `cluster`.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `protocol_min` | u16, at least 1 |
/// | 2 | `protocol_max` | u16, at least `protocol_min` |
/// | 4 | `release` | u32 |
/// | 8 | `max_frame` | u32, [`crate::MAX_FRAME_FLOOR`] to [`crate::MAX_FRAME_SIZE`] |
///
/// | Section | Contents |
/// |---|---|
/// | 0 | the client's name, UTF-8 |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hello {
    protocol_min: u16,
    protocol_max: u16,
    release: u32,
    max_frame: u32,
    client: String,
}

impl Hello {
    /// A Hello speaking protocols `protocol_min` to `protocol_max`, from the client `client` at `release`, taking frames up to `max_frame` bytes.
    pub fn new(
        protocol_min: u16,
        protocol_max: u16,
        release: u32,
        max_frame: u32,
        client: impl Into<String>,
    ) -> Self {
        Self {
            protocol_min,
            protocol_max,
            release,
            max_frame,
            client: client.into(),
        }
    }

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

    /// The largest frame the client takes: no server frame is larger.
    pub fn max_frame(&self) -> u32 {
        self.max_frame
    }

    /// The client's name.
    pub fn client(&self) -> &str {
        &self.client
    }
}
