/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

/// A bucket range of a Placement reply (`PROTOCOL.md` §13.6), 16 bytes: `start` u32, the range's first bucket; `end`
/// u32, one past its last; `partition` u32, the partition owning it, from 1; 4 zero bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BucketRange {
    start: u32,
    end: u32,
    partition: u32,
}

impl BucketRange {
    /// A range's size in bytes.
    pub const SIZE: usize = 16;

    /// The buckets `start` to `end`, exclusive, owned by `partition`.
    #[must_use]
    pub fn new(start: u32, end: u32, partition: u32) -> Self {
        Self {
            start,
            end,
            partition,
        }
    }

    /// The range's first bucket.
    #[must_use]
    pub fn start(&self) -> u32 {
        self.start
    }

    /// One past the range's last bucket.
    #[must_use]
    pub fn end(&self) -> u32 {
        self.end
    }

    /// The partition owning the range.
    #[must_use]
    pub fn partition(&self) -> u32 {
        self.partition
    }

    /// The range's 16 bytes.
    #[must_use]
    pub fn encode(&self) -> [u8; BucketRange::SIZE] {
        let fields = self
            .start
            .to_le_bytes()
            .into_iter()
            .chain(self.end.to_le_bytes())
            .chain(self.partition.to_le_bytes());
        let mut out = [0u8; BucketRange::SIZE];
        out.iter_mut()
            .zip(fields)
            .for_each(|(byte, field)| *byte = field);
        out
    }

    /// The range `bytes` hold, at body offset `at` of a body of `command`: bytes 12 to 15 zero, refused as
    /// [`ProtocolError::padding_not_zero`] at the first nonzero one.
    pub(crate) fn decode(
        command: u16,
        at: usize,
        bytes: &[u8; BucketRange::SIZE],
    ) -> Result<Self, ProtocolError> {
        let [
            s0,
            s1,
            s2,
            s3,
            e0,
            e1,
            e2,
            e3,
            p0,
            p1,
            p2,
            p3,
            z0,
            z1,
            z2,
            z3,
        ] = *bytes;
        if let Some(i) = [z0, z1, z2, z3].iter().position(|b| *b != 0) {
            return Err(ProtocolError::padding_not_zero(command, at + 12 + i));
        }
        Ok(Self::new(
            u32::from_le_bytes([s0, s1, s2, s3]),
            u32::from_le_bytes([e0, e1, e2, e3]),
            u32::from_le_bytes([p0, p1, p2, p3]),
        ))
    }
}
