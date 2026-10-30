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
    /// ([`HeaderView::new`]); the header's checksum, command, reserved bytes, cluster, flags and size within
    /// `max_frame` ([`HeaderView::verify`]); `size` equal to the bytes given, refused as
    /// [`ProtocolError::LengthMismatch`]; then the body's chunk table and chunks ([`ChunkedBody::verify`]). For a frame
    /// a server or the log sent. The connection's sequence and protocol are its receiver's to check, and the body's
    /// structure its command's view's.
    pub fn validate(
        bytes: &'a [u8],
        cluster: u64,
        max_frame: usize,
    ) -> Result<Self, ProtocolError> {
        let header = HeaderView::new(bytes)?;
        let command = header.verify(cluster, max_frame)?.command();
        Self::validate_body(bytes, header, command)
    }

    /// [`FrameView::validate`] for a frame a client sent ([`HeaderView::verify_client`]).
    pub fn validate_client(
        bytes: &'a [u8],
        cluster: u64,
        max_frame: usize,
    ) -> Result<Self, ProtocolError> {
        let header = HeaderView::new(bytes)?;
        let command = header.verify_client(cluster, max_frame)?.command();
        Self::validate_body(bytes, header, command)
    }

    /// The frame of a verified `header`: its length, then its chunks.
    fn validate_body(
        bytes: &'a [u8],
        header: HeaderView<'a>,
        command: Command,
    ) -> Result<Self, ProtocolError> {
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
