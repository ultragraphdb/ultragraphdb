/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::types::chunked_body::ChunkedBody;
use crate::types::command::Command;
use crate::types::header_view::HeaderView;
use crate::utils::constants::HEADER_SIZE;

/// A received frame that passed every rule before its body's structure, read in place from the receive buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameView<'a> {
    header: HeaderView<'a>,
    command: Command,
    body: ChunkedBody<'a>,
}

impl<'a> FrameView<'a> {
    /// `bytes` checked as one frame for `cluster`, in the protocol's order: the length of a header
    /// ([`HeaderView::new`]); the header's checksum, command, reserved bytes, cluster and size
    /// ([`HeaderView::verify`]); `size` equal to the bytes given, refused as [`ProtocolError::LengthMismatch`]; then
    /// the body's chunk table and chunks ([`ChunkedBody::verify`]). The body's structure is its command's view's to
    /// check.
    pub fn validate(bytes: &'a [u8], cluster: u64) -> Result<Self, ProtocolError> {
        let header = HeaderView::new(bytes)?;
        let command = header.verify(cluster)?.command();
        if header.size() as usize != bytes.len() {
            return Err(ProtocolError::LengthMismatch(header.size(), bytes.len()));
        }
        let body = ChunkedBody::verify(
            bytes.get(HEADER_SIZE..).unwrap_or_default(),
            header.checksum_body(),
        )?;
        Ok(Self {
            header,
            command,
            body,
        })
    }

    pub fn header(&self) -> &HeaderView<'a> {
        &self.header
    }

    pub fn command(&self) -> Command {
        self.command
    }

    /// The body's bytes before its chunk table.
    pub fn body(&self) -> &'a [u8] {
        self.body.data()
    }

    pub fn chunked_body(&self) -> &ChunkedBody<'a> {
        &self.body
    }
}
