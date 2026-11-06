/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;

use crate::types::chunk_layout::ChunkLayout;
use crate::types::chunked_body::ChunkedBody;
use crate::types::header::Header;
use crate::utils::checksum::xxh3_64;
use crate::utils::constants::{HEADER_SIZE, MAX_FRAME_SIZE};

/// Where a header holds its `stream`.
const STREAM_OFFSET: usize = 56;

/// Where a header's command header begins.
const COMMAND_HEADER_OFFSET: usize = 128;

impl Header {
    /// The header's 256 bytes with `size` and `checksum_body` as given, and `checksum` computed over bytes 8 to 256.
    pub fn encode(&self, size: u32, checksum_body: u64) -> [u8; HEADER_SIZE] {
        // Bytes 8 to 256 in field order, then the checksum over them in front: no index into the result.
        let tail: Vec<u8> = [
            &checksum_body.to_le_bytes()[..],
            &self.sequence().to_le_bytes(),
            &self.cluster.to_le_bytes(),
            &size.to_le_bytes(),
            &self.view.to_le_bytes(),
            &self.release.to_le_bytes(),
            &self.protocol.to_le_bytes(),
            &self.command.number().to_le_bytes(),
            &self.stream.to_le_bytes(),
            &[self.replica, self.flags],
            &[0u8; 62],
            &self.command_header,
        ]
        .concat();
        let checksum = xxh3_64(&tail);
        let mut header = [0u8; HEADER_SIZE];
        header
            .iter_mut()
            .zip(checksum.to_le_bytes().iter().chain(tail.iter()))
            .for_each(|(out, byte)| *out = *byte);
        header
    }

    /// Writes `sequence` into the header of `frame`, a whole frame, and recomputes its `checksum`: a sender numbers a
    /// frame as it leaves, after the frame was built. Refused as [`ProtocolError::short_header`] for fewer than 256
    /// bytes.
    pub fn stamp_sequence(frame: &mut [u8], sequence: u128) -> Result<(), ProtocolError> {
        let len = frame.len();
        let header = frame
            .get_mut(..HEADER_SIZE)
            .ok_or(ProtocolError::short_header(len))?;
        let (checksum, tail) = header.split_at_mut(8);
        tail.iter_mut()
            .skip(8)
            .zip(sequence.to_le_bytes())
            .for_each(|(out, byte)| *out = byte);
        checksum.copy_from_slice(&xxh3_64(tail).to_le_bytes());
        Ok(())
    }

    /// Writes `stream` into the header of `frame`, a whole frame, and recomputes its `checksum`: a frame a role relays keeps its
    /// body and takes the stream of the connection it goes out on. Refused as [`ProtocolError::short_header`] for fewer than
    /// 256 bytes.
    pub fn stamp_stream(frame: &mut [u8], stream: u64) -> Result<(), ProtocolError> {
        Self::stamp(frame, STREAM_OFFSET, &stream.to_le_bytes())
    }

    /// Writes `bytes` into the command header of `frame`, a whole frame, at `offset` from the command header's start, and
    /// recomputes its `checksum`: what a relay fills in, a one-frame write's `principal`. Refused as
    /// [`ProtocolError::short_header`] for fewer than 256 bytes; bytes past the header are not written.
    pub fn stamp_command_bytes(
        frame: &mut [u8],
        offset: usize,
        bytes: &[u8],
    ) -> Result<(), ProtocolError> {
        Self::stamp(frame, COMMAND_HEADER_OFFSET + offset, bytes)
    }

    /// Writes `bytes` at `at` in the header of `frame` and recomputes its `checksum`.
    fn stamp(frame: &mut [u8], at: usize, bytes: &[u8]) -> Result<(), ProtocolError> {
        let len = frame.len();
        let header = frame
            .get_mut(..HEADER_SIZE)
            .ok_or(ProtocolError::short_header(len))?;
        header
            .iter_mut()
            .skip(at)
            .zip(bytes)
            .for_each(|(out, byte)| *out = *byte);
        let (checksum, tail) = header.split_at_mut(8);
        checksum.copy_from_slice(&xxh3_64(tail).to_le_bytes());
        Ok(())
    }

    /// The frame of this header and `body`: the header, the body, and the body's chunk table when it has more than
    /// one chunk, with `size`, `checksum_body` and `checksum` filled in. Refused as
    /// [`ProtocolError::frame_too_large`] when the frame would exceed [`crate::MAX_FRAME_SIZE`].
    pub fn encode_frame(&self, body: &[u8]) -> Result<Vec<u8>, ProtocolError> {
        let size = HEADER_SIZE + ChunkLayout::of_data(body.len()).body_len();
        if size > MAX_FRAME_SIZE {
            return Err(ProtocolError::frame_too_large(size, MAX_FRAME_SIZE));
        }
        let mut chunked = Vec::with_capacity(size - HEADER_SIZE);
        let checksum_body = ChunkedBody::encode(body, &mut chunked);
        // size is at most MAX_FRAME_SIZE, 2^20, so it fits a u32.
        let header = self.encode(size as u32, checksum_body);
        Ok([&header[..], &chunked].concat())
    }
}
