/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod subscribe_body_builder;

/// Storage, and each endpoint core of the frontend, asks for the sequencer's epoch closes (`PROTOCOL.md` §13.3),
/// answered with EpochClose frames on the request's stream.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `from` | epoch: the last epoch whose close the subscriber holds; 0 for none |
/// | 8 | `latest` | u8 boolean: 1 begins the stream with the latest durable close; the frontend alone sends 1, and only with `from` 0 |
///
/// No sections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Subscribe {
    from: u64,
    latest: bool,
}

impl Subscribe {
    /// A subscription to the closes after epoch `from`.
    #[must_use]
    pub fn new(from: u64) -> Self {
        Self {
            from,
            latest: false,
        }
    }

    /// A subscription that begins with the latest durable close, which an endpoint core of the frontend opens: `latest`
    /// 1 and `from` 0.
    #[must_use]
    pub fn at_latest() -> Self {
        Self {
            from: 0,
            latest: true,
        }
    }

    /// The last epoch whose close the subscriber holds; 0 for none.
    #[must_use]
    pub fn from(&self) -> u64 {
        self.from
    }

    /// Whether the stream begins with the latest durable close.
    #[must_use]
    pub fn latest(&self) -> bool {
        self.latest
    }
}
