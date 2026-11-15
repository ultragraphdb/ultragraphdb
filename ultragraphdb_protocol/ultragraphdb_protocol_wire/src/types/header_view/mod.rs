/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod header_view_verify;

use ultragraphdb_error_protocol::ProtocolError;

use crate::utils::constants::{COMMAND_HEADER_SIZE, HEADER_SIZE};
use crate::utils::le::bytes_at;

/// A received header, its fields read in place from the first 256 bytes of a frame. Constructing it checks the
/// length and nothing else; [`HeaderView::verify`] checks the rest, so a field read before that is unchecked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeaderView<'a> {
    tail: &'a [u8],
    reserved: &'a [u8],
    command_header: &'a [u8; COMMAND_HEADER_SIZE],
    checksum: u64,
    checksum_body: u64,
    sequence: u128,
    cluster: u64,
    size: u32,
    view: u32,
    release: u32,
    protocol: u16,
    command: u16,
    stream: u64,
    replica: u8,
    flags: u8,
}

impl<'a> HeaderView<'a> {
    /// The header at the start of `bytes`. Bytes past the header are not read.
    ///
    /// # Errors
    ///
    /// [`ProtocolError::short_header`] when `bytes` holds fewer than 256 bytes.
    pub fn new(bytes: &'a [u8]) -> Result<Self, ProtocolError> {
        let short = || ProtocolError::short_header(bytes.len());
        Ok(Self {
            tail: bytes.get(8..HEADER_SIZE).ok_or_else(short)?,
            reserved: bytes.get(66..128).ok_or_else(short)?,
            command_header: bytes
                .get(128..HEADER_SIZE)
                .and_then(|s| s.try_into().ok())
                .ok_or_else(short)?,
            checksum: u64::from_le_bytes(bytes_at(bytes, 0).ok_or_else(short)?),
            checksum_body: u64::from_le_bytes(bytes_at(bytes, 8).ok_or_else(short)?),
            sequence: u128::from_le_bytes(bytes_at(bytes, 16).ok_or_else(short)?),
            cluster: u64::from_le_bytes(bytes_at(bytes, 32).ok_or_else(short)?),
            size: u32::from_le_bytes(bytes_at(bytes, 40).ok_or_else(short)?),
            view: u32::from_le_bytes(bytes_at(bytes, 44).ok_or_else(short)?),
            release: u32::from_le_bytes(bytes_at(bytes, 48).ok_or_else(short)?),
            protocol: u16::from_le_bytes(bytes_at(bytes, 52).ok_or_else(short)?),
            command: u16::from_le_bytes(bytes_at(bytes, 54).ok_or_else(short)?),
            stream: u64::from_le_bytes(bytes_at(bytes, 56).ok_or_else(short)?),
            replica: u8::from_le_bytes(bytes_at(bytes, 64).ok_or_else(short)?),
            flags: u8::from_le_bytes(bytes_at(bytes, 65).ok_or_else(short)?),
        })
    }

    /// The header's checksum: XXH3-64 of its bytes 8 to 256.
    #[must_use]
    pub fn checksum(&self) -> u64 {
        self.checksum
    }

    /// The checksum of the body's chunk table.
    #[must_use]
    pub fn checksum_body(&self) -> u64 {
        self.checksum_body
    }

    /// The frame's place among its sender's frames on the connection, from 1.
    #[must_use]
    pub fn sequence(&self) -> u128 {
        self.sequence
    }

    /// The cluster the frame belongs to.
    #[must_use]
    pub fn cluster(&self) -> u64 {
        self.cluster
    }

    /// The frame's size in bytes, header and body.
    #[must_use]
    pub fn size(&self) -> u32 {
        self.size
    }

    /// The VSR view; 0 from a client.
    #[must_use]
    pub fn view(&self) -> u32 {
        self.view
    }

    /// The sender's release.
    #[must_use]
    pub fn release(&self) -> u32 {
        self.release
    }

    /// The protocol version.
    #[must_use]
    pub fn protocol(&self) -> u16 {
        self.protocol
    }

    /// The `command` field as a number; [`HeaderView::verify`] maps it to a [`crate::Command`].
    #[must_use]
    pub fn command(&self) -> u16 {
        self.command
    }

    /// The stream the frame belongs to; 0 for the connection.
    #[must_use]
    pub fn stream(&self) -> u64 {
        self.stream
    }

    /// The sending replica; 0 from a client.
    #[must_use]
    pub fn replica(&self) -> u8 {
        self.replica
    }

    /// The header's flags: bit 0 marks the stream's last frame.
    #[must_use]
    pub fn flags(&self) -> u8 {
        self.flags
    }

    /// The 62 reserved bytes at 66.
    #[must_use]
    pub fn reserved(&self) -> &'a [u8] {
        self.reserved
    }

    /// The command header's 128 bytes.
    #[must_use]
    pub fn command_header(&self) -> &'a [u8; COMMAND_HEADER_SIZE] {
        self.command_header
    }
}
