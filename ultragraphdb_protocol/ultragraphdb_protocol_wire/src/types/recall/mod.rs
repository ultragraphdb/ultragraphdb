/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod recall_body_builder;

/// The frontend asks a partition for a session's stored reply (`PROTOCOL.md` §13.11), for a retried Commit or Drop it
/// cannot answer itself. Answered on its stream with the stored Commit or Drop frame, or an Error.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `partition` | u32, at least 1 |
/// | 8 | `session` | 16 bytes, not all zero |
/// | 24 | `request` | u64, at least 1 |
///
/// No sections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Recall {
    partition: u32,
    session: [u8; 16],
    request: u64,
}

// A copy moves at most one cache line (Ultra rule 9).
const _: () = assert!(size_of::<Recall>() <= 64);

impl Recall {
    /// The recall of `session`'s reply to `request` from `partition`.
    #[must_use]
    pub fn new(partition: u32, session: [u8; 16], request: u64) -> Self {
        Self {
            partition,
            session,
            request,
        }
    }

    /// The partition asked.
    #[must_use]
    pub fn partition(&self) -> u32 {
        self.partition
    }

    /// The session.
    #[must_use]
    pub fn session(&self) -> [u8; 16] {
        self.session
    }

    /// The request number whose reply is recalled.
    #[must_use]
    pub fn request(&self) -> u64 {
        self.request
    }
}
