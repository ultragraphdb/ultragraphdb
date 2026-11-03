/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

/// A namespace's run of a Placement reply (`PROTOCOL.md` §13.6), 16 bytes: `namespace` u64; `run` u8, the top 8 bits of
/// the namespace's buckets; 7 zero bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NamespaceRun {
    namespace: u64,
    run: u8,
}

impl NamespaceRun {
    /// A run's size in bytes.
    pub const SIZE: usize = 16;

    /// `namespace`'s buckets in `run`, `run × 65,536` to `run × 65,536 + 65,535`.
    pub fn new(namespace: u64, run: u8) -> Self {
        Self { namespace, run }
    }

    /// The namespace.
    pub fn namespace(&self) -> u64 {
        self.namespace
    }

    /// The run, the top 8 bits of the namespace's buckets.
    pub fn run(&self) -> u8 {
        self.run
    }

    /// The run's 16 bytes.
    pub fn encode(&self) -> [u8; NamespaceRun::SIZE] {
        let mut out = [0u8; NamespaceRun::SIZE];
        out.iter_mut()
            .zip(self.namespace.to_le_bytes().into_iter().chain([self.run]))
            .for_each(|(byte, field)| *byte = field);
        out
    }

    /// The run `bytes` hold, at body offset `at` of a body of `command`: bytes 9 to 15 zero, refused as
    /// [`ProtocolError::padding_not_zero`] at the first nonzero one.
    pub(crate) fn decode(
        command: u16,
        at: usize,
        bytes: &[u8; NamespaceRun::SIZE],
    ) -> Result<Self, ProtocolError> {
        let (namespace, rest) = bytes.split_at(8);
        let (run, zero) = rest.split_at(1);
        if let Some(i) = zero.iter().position(|b| *b != 0) {
            return Err(ProtocolError::padding_not_zero(command, at + 9 + i));
        }
        Ok(Self::new(
            namespace.try_into().map_or(0, u64::from_le_bytes),
            run.first().copied().unwrap_or(0),
        ))
    }
}
