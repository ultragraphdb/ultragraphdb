/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::ops::Range;

/// A warning a reply carries, as its sender holds it (`PROTOCOL.md` §4.9): what a compilation reported without
/// refusing the request, such as a retired name resolved during its migration's deprecation window.
///
/// | Warning offset | Field | Type |
/// |---|---|---|
/// | 0 | `diagnostic` | u32: the diagnostic's number in bits 0–15, its language in bits 16–23 (section 9.4), bits 24–31 zero |
/// | 4 | `source` | u32: in an Install's reply, the source's place among the request's sources, from 0; else 0 |
/// | 8 | `offset` | u32: the location's first byte within that source's text |
/// | 12 | `length` | u32: the location's length in bytes |
/// | 16 | `message` | u32: the message's index in the warnings' string list |
/// | 20 | zero | 0 |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    diagnostic: u32,
    source: u32,
    location: Range<u32>,
    message: String,
}

impl Warning {
    /// The warning of `diagnostic`, a number and its language as code 29's `detail` encodes them, with its `message`:
    /// the diagnostic's code, then its reason and, where one exists, its fix. Its source is 0 and its location empty
    /// until set.
    #[must_use]
    pub fn new(diagnostic: u32, message: impl Into<String>) -> Self {
        Self {
            diagnostic,
            source: 0,
            location: 0..0,
            message: message.into(),
        }
    }

    /// The warning located in source `source`, its place among an Install's sources, from 0.
    #[must_use]
    pub fn with_source(self, source: u32) -> Self {
        Self { source, ..self }
    }

    /// The warning located at the bytes `location` of its source's text.
    #[must_use]
    pub fn with_location(self, location: Range<u32>) -> Self {
        Self { location, ..self }
    }

    /// The diagnostic: its number in bits 0–15 and its language in bits 16–23.
    #[must_use]
    pub fn diagnostic(&self) -> u32 {
        self.diagnostic
    }

    /// The source the location lies in.
    #[must_use]
    pub fn source(&self) -> u32 {
        self.source
    }

    /// The location's bytes within its source's text.
    #[must_use]
    pub fn location(&self) -> Range<u32> {
        self.location.clone()
    }

    /// The message, for people.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}
