/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::types::command::Command;
use crate::types::header::Header;
use crate::types::header_view::HeaderView;
use crate::utils::checksum::xxh3_64;
use crate::utils::constants::{HEADER_SIZE, MAX_FRAME_SIZE};

impl HeaderView<'_> {
    /// The header's rules after the length, in the protocol's order: the checksum over bytes 8 to 256; the command
    /// known; `nonce_reserved` and the reserved bytes zero; `cluster` equal to the receiver's `cluster`; `size` at
    /// least the header and at most 1 MiB. The first rule broken is the error; a header that keeps them all is
    /// returned as the [`Header`] of its fields.
    pub fn verify(&self, cluster: u64) -> Result<Header, ProtocolError> {
        let computed = xxh3_64(self.tail);
        if computed != self.checksum {
            return Err(ProtocolError::HeaderChecksum(self.checksum, computed));
        }
        let command = Command::from_number(self.command)?;
        let nonzero =
            |bytes: &[u8], start: usize| bytes.iter().position(|b| *b != 0).map(|i| start + i);
        if let Some(offset) = nonzero(self.nonce, 16).or_else(|| nonzero(self.reserved, 66)) {
            return Err(ProtocolError::ReservedNotZero(offset));
        }
        if self.cluster != cluster {
            return Err(ProtocolError::WrongCluster(cluster, self.cluster));
        }
        let size = self.size as usize;
        if size < HEADER_SIZE {
            return Err(ProtocolError::SizeBelowHeader(self.size));
        }
        if size > MAX_FRAME_SIZE {
            return Err(ProtocolError::FrameTooLarge(size, MAX_FRAME_SIZE));
        }
        Ok(Header::new(self.cluster, command)
            .with_view(self.view)
            .with_release(self.release)
            .with_protocol(self.protocol)
            .with_stream(self.stream)
            .with_replica(self.replica)
            .with_flags(self.flags)
            .with_command_header(*self.command_header))
    }
}
