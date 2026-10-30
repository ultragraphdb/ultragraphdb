/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::types::command::Command;
use crate::types::header::Header;
use crate::types::header_view::HeaderView;
use crate::utils::checksum::xxh3_64;
use crate::utils::constants::{FLAG_LAST, HEADER_SIZE, MAX_FRAME_SIZE};

impl HeaderView<'_> {
    /// The header's rules after the length, in the protocol's order (`PROTOCOL.md` §2.5, steps 2 to 8): the checksum
    /// over bytes 8 to 256; the command known; the reserved bytes zero; `cluster` equal to the receiver's `cluster`;
    /// `flags` bits 1 to 7 zero; `size` at least the header and at most `max_frame`, the largest frame the receiver
    /// announced, itself at most 1 MiB. The first rule broken is the error; a header that keeps them all is returned
    /// as the [`Header`] of its fields. For a frame a server or the log sent; [`HeaderView::verify_client`] checks a
    /// client's.
    pub fn verify(&self, cluster: u64, max_frame: usize) -> Result<Header, ProtocolError> {
        self.check(cluster, max_frame, false)
    }

    /// [`HeaderView::verify`] for a frame a client sent, whose `view` and `replica` are 0 besides, checked after the
    /// flags.
    pub fn verify_client(&self, cluster: u64, max_frame: usize) -> Result<Header, ProtocolError> {
        self.check(cluster, max_frame, true)
    }

    fn check(&self, cluster: u64, max_frame: usize, client: bool) -> Result<Header, ProtocolError> {
        let computed = xxh3_64(self.tail);
        if computed != self.checksum {
            return Err(ProtocolError::HeaderChecksum(self.checksum, computed));
        }
        let command = Command::from_number(self.command)?;
        if let Some(i) = self.reserved.iter().position(|b| *b != 0) {
            return Err(ProtocolError::ReservedNotZero(66 + i));
        }
        if self.cluster != cluster {
            return Err(ProtocolError::WrongCluster(cluster, self.cluster));
        }
        if self.flags & !FLAG_LAST != 0 {
            return Err(ProtocolError::FieldValue(
                self.command,
                "flags",
                self.flags.into(),
            ));
        }
        if client && self.view != 0 {
            return Err(ProtocolError::FieldValue(
                self.command,
                "view",
                self.view.into(),
            ));
        }
        if client && self.replica != 0 {
            return Err(ProtocolError::FieldValue(
                self.command,
                "replica",
                self.replica.into(),
            ));
        }
        let size = self.size as usize;
        if size < HEADER_SIZE {
            return Err(ProtocolError::SizeBelowHeader(self.size));
        }
        let limit = max_frame.min(MAX_FRAME_SIZE);
        if size > limit {
            return Err(ProtocolError::FrameTooLarge(size, limit));
        }
        Ok(Header::new(self.cluster, command)
            .with_sequence(self.sequence)
            .with_view(self.view)
            .with_release(self.release)
            .with_protocol(self.protocol)
            .with_stream(self.stream)
            .with_replica(self.replica)
            .with_flags(self.flags)
            .with_command_header(*self.command_header))
    }
}
