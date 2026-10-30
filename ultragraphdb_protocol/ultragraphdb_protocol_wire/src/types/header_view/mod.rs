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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    /// The header at the start of `bytes`, refused as [`ProtocolError::ShortHeader`] when `bytes` holds fewer than
    /// 256 bytes. Bytes past the header are not read.
    pub fn new(bytes: &'a [u8]) -> Result<Self, ProtocolError> {
        let short = || ProtocolError::ShortHeader(bytes.len());
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

    pub fn checksum(&self) -> u64 {
        self.checksum
    }

    pub fn checksum_body(&self) -> u64 {
        self.checksum_body
    }

    pub fn sequence(&self) -> u128 {
        self.sequence
    }

    pub fn cluster(&self) -> u64 {
        self.cluster
    }

    pub fn size(&self) -> u32 {
        self.size
    }

    pub fn view(&self) -> u32 {
        self.view
    }

    pub fn release(&self) -> u32 {
        self.release
    }

    pub fn protocol(&self) -> u16 {
        self.protocol
    }

    /// The `command` field as a number; [`HeaderView::verify`] maps it to a [`crate::Command`].
    pub fn command(&self) -> u16 {
        self.command
    }

    pub fn stream(&self) -> u64 {
        self.stream
    }

    pub fn replica(&self) -> u8 {
        self.replica
    }

    pub fn flags(&self) -> u8 {
        self.flags
    }

    /// The 62 reserved bytes at 66.
    pub fn reserved(&self) -> &'a [u8] {
        self.reserved
    }

    pub fn command_header(&self) -> &'a [u8; COMMAND_HEADER_SIZE] {
        self.command_header
    }
}
