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

    /// The frame of this header and `body`: the header, the body, and the body's chunk table when it has more than
    /// one chunk, with `size`, `checksum_body` and `checksum` filled in. Refused as
    /// [`ProtocolError::FrameTooLarge`] when the frame would exceed [`crate::MAX_FRAME_SIZE`].
    pub fn encode_frame(&self, body: &[u8]) -> Result<Vec<u8>, ProtocolError> {
        let size = HEADER_SIZE + ChunkLayout::of_data(body.len()).body_len();
        if size > MAX_FRAME_SIZE {
            return Err(ProtocolError::FrameTooLarge(size, MAX_FRAME_SIZE));
        }
        let mut chunked = Vec::with_capacity(size - HEADER_SIZE);
        let checksum_body = ChunkedBody::encode(body, &mut chunked);
        // size is at most MAX_FRAME_SIZE, 2^20, so it fits a u32.
        let header = self.encode(size as u32, checksum_body);
        Ok([&header[..], &chunked].concat())
    }
}
