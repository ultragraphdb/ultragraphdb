/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// A partition's preferred leader in a Placement reply (`PROTOCOL.md` §13.6), 8 bytes: `partition` u32, then
/// `replica` u32, a replica numbered from 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PreferredLeader {
    partition: u32,
    replica: u32,
}

impl PreferredLeader {
    /// A leader's size in bytes.
    pub const SIZE: usize = 8;

    /// `partition`'s preferred leader, `replica`.
    #[must_use]
    pub fn new(partition: u32, replica: u32) -> Self {
        Self { partition, replica }
    }

    /// The partition.
    #[must_use]
    pub fn partition(&self) -> u32 {
        self.partition
    }

    /// The preferred leader's replica.
    #[must_use]
    pub fn replica(&self) -> u32 {
        self.replica
    }

    /// The leader's 8 bytes.
    #[must_use]
    pub fn encode(&self) -> [u8; PreferredLeader::SIZE] {
        let [p0, p1, p2, p3] = self.partition.to_le_bytes();
        let [r0, r1, r2, r3] = self.replica.to_le_bytes();
        [p0, p1, p2, p3, r0, r1, r2, r3]
    }

    /// The leader `bytes` hold.
    pub(crate) fn decode(bytes: [u8; PreferredLeader::SIZE]) -> Self {
        let [p0, p1, p2, p3, r0, r1, r2, r3] = bytes;
        Self::new(
            u32::from_le_bytes([p0, p1, p2, p3]),
            u32::from_le_bytes([r0, r1, r2, r3]),
        )
    }
}
