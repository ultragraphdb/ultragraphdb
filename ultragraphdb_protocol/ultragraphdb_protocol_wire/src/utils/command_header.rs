/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The command header's fields: little-endian at their offsets within its 128 bytes, every byte no field uses zero.

use ultragraphdb_error_protocol::ProtocolError;

use crate::utils::constants::COMMAND_HEADER_SIZE;
use crate::utils::le::bytes_at;

/// The command header's 128 bytes, read field by field.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CommandHeader<'a>(pub(crate) &'a [u8; COMMAND_HEADER_SIZE]);

impl CommandHeader<'_> {
    pub(crate) fn u8_at(&self, at: usize) -> u8 {
        self.0.get(at).copied().unwrap_or(0)
    }

    pub(crate) fn u16_at(&self, at: usize) -> u16 {
        bytes_at(self.0, at).map_or(0, u16::from_le_bytes)
    }

    pub(crate) fn u32_at(&self, at: usize) -> u32 {
        bytes_at(self.0, at).map_or(0, u32::from_le_bytes)
    }

    pub(crate) fn u64_at(&self, at: usize) -> u64 {
        bytes_at(self.0, at).map_or(0, u64::from_le_bytes)
    }

    /// Refused as [`ProtocolError::ReservedNotZero`], naming the header offset (128 + its offset here), at the first
    /// nonzero byte outside `used`, each a field's start and end.
    pub(crate) fn unused_zero(&self, used: &[(usize, usize)]) -> Result<(), ProtocolError> {
        match self.0.iter().enumerate().find(|(at, byte)| {
            **byte != 0 && !used.iter().any(|(start, end)| (*start..*end).contains(at))
        }) {
            Some((at, _)) => Err(ProtocolError::ReservedNotZero(COMMAND_HEADER_SIZE + at)),
            None => Ok(()),
        }
    }
}

/// The command header holding each `(offset, bytes)` field, every other byte zero.
pub(crate) fn command_header(fields: &[(usize, &[u8])]) -> [u8; COMMAND_HEADER_SIZE] {
    let mut header = [0u8; COMMAND_HEADER_SIZE];
    for (offset, bytes) in fields {
        header
            .iter_mut()
            .skip(*offset)
            .zip(bytes.iter())
            .for_each(|(out, byte)| *out = *byte);
    }
    header
}
