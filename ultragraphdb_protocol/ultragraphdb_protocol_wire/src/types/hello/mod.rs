/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod hello_body_builder;

/// A client opens a connection: the protocols it speaks, its release and its name. The cluster it connects to is the
/// frame header's `cluster`.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `protocol_min` | u16, at least 1 |
/// | 2 | `protocol_max` | u16, at least `protocol_min` |
/// | 4 | `release` | u32 |
///
/// | Section | Contents |
/// |---|---|
/// | 0 | the client's name, UTF-8 |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hello {
    protocol_min: u16,
    protocol_max: u16,
    release: u32,
    client: String,
}

impl Hello {
    pub fn new(
        protocol_min: u16,
        protocol_max: u16,
        release: u32,
        client: impl Into<String>,
    ) -> Self {
        Self {
            protocol_min,
            protocol_max,
            release,
            client: client.into(),
        }
    }

    pub fn protocol_min(&self) -> u16 {
        self.protocol_min
    }

    pub fn protocol_max(&self) -> u16 {
        self.protocol_max
    }

    pub fn release(&self) -> u32 {
        self.release
    }

    pub fn client(&self) -> &str {
        &self.client
    }
}
