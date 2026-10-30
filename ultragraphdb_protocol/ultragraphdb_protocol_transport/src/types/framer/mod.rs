/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_protocol::ProtocolError;
use ultragraphdb_protocol_wire::{HEADER_SIZE, HeaderView, PROTOCOL_VERSION};

use crate::errors::transport_error::TransportError;
use crate::types::frame_step::FrameStep;
use crate::types::pooled_buffer::PooledBuffer;
use crate::types::received_frame::ReceivedFrame;

/// Frames assembled from a byte stream, sans-I/O and pull-style: the framer tells its shell how many bytes the frame's
/// pooled buffer must hold before it looks again ([`Framer::want`]), so a shell reads straight into the buffer the
/// frame stays in, with an owned-buffer read (io_uring) or a `&mut [u8]` read alike, and nothing is copied. First
/// the 256-byte header, checked by `PROTOCOL.md` §2.5's steps 1 to 8 for the framer's cluster and the largest frame
/// its connection announced before its `size` is trusted, and refused when the frame would not fit the buffer; then
/// the rest of the frame into the same buffer. The body's chunks, the frame's sequence and its protocol are checked
/// when the frame is viewed ([`ReceivedFrame::view`]), each frame expecting the sequence one above the last.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Framer {
    cluster: u64,
    max_frame: usize,
    client: bool,
    want: usize,
    sequence: u128,
    protocol: u16,
}

impl Framer {
    /// A client's framer of a connection to a node of `cluster`, reading a server's frames of at most `max_frame`
    /// bytes, the Hello's, waiting for a frame's header.
    pub fn new(cluster: u64, max_frame: usize) -> Self {
        Self {
            cluster,
            max_frame,
            client: false,
            want: HEADER_SIZE,
            sequence: 1,
            protocol: PROTOCOL_VERSION,
        }
    }

    /// A server's framer of a connection from a client of `cluster`, reading a client's frames of at most `max_frame`
    /// bytes, the Welcome's, whose view and replica are 0.
    pub fn from_client(cluster: u64, max_frame: usize) -> Self {
        Self {
            client: true,
            ..Self::new(cluster, max_frame)
        }
    }

    /// The frames after the handshake read at `protocol`, the Welcome's.
    pub fn set_protocol(&mut self, protocol: u16) {
        self.protocol = protocol;
    }

    pub fn cluster(&self) -> u64 {
        self.cluster
    }

    /// The bytes the frame's buffer must hold before [`Framer::advance`]: the header's 256, then the frame's `size`.
    pub fn want(&self) -> usize {
        self.want
    }

    /// Whether the framer waits for a frame's header, so a close now falls between frames.
    pub fn between_frames(&self) -> bool {
        self.want == HEADER_SIZE
    }

    /// Looks at `buffer`, filled to [`Framer::want`]: at the header, verifies it for the cluster and the largest frame
    /// ([`TransportError::Protocol`] naming the rule) and refuses a frame larger than the buffer's capacity
    /// ([`ProtocolError::FrameTooLarge`]); then asks for the frame's size, or reports the frame whole.
    pub fn advance(&mut self, buffer: &PooledBuffer) -> Result<FrameStep, TransportError> {
        let bytes = buffer.as_slice();
        if bytes.len() < self.want {
            return Ok(FrameStep::Read(self.want));
        }
        if self.want == HEADER_SIZE {
            let size = HeaderView::new(bytes)
                .and_then(|view| {
                    let header = if self.client {
                        view.verify_client(self.cluster, self.max_frame)
                    } else {
                        view.verify(self.cluster, self.max_frame)
                    };
                    header.map(|_| view.size() as usize)
                })
                .map_err(TransportError::Protocol)?;
            if size > buffer.capacity() {
                return Err(TransportError::Protocol(ProtocolError::FrameTooLarge(
                    size,
                    buffer.capacity(),
                )));
            }
            if size > HEADER_SIZE {
                self.want = size;
                return Ok(FrameStep::Read(size));
            }
        }
        Ok(FrameStep::Frame)
    }

    /// The error of a peer that closed with `received` bytes of the current frame read: [`TransportError::Closed`]
    /// between frames, [`TransportError::Truncated`] inside one. The framer waits for a header again.
    pub fn closed(&mut self, received: usize) -> TransportError {
        let expected = std::mem::replace(&mut self.want, HEADER_SIZE);
        if received == 0 && expected == HEADER_SIZE {
            TransportError::Closed()
        } else {
            TransportError::Truncated(received, expected)
        }
    }

    /// Forgets a frame in progress after a failed read; the framer waits for a header again.
    pub fn reset(&mut self) {
        self.want = HEADER_SIZE;
    }

    /// The whole frame in `buffer`, after [`FrameStep::Frame`], expecting the framer's next sequence; the framer waits
    /// for the next header and expects the sequence after.
    pub fn finish(&mut self, buffer: PooledBuffer) -> ReceivedFrame {
        self.want = HEADER_SIZE;
        let sequence = self.sequence;
        self.sequence = self.sequence.saturating_add(1);
        ReceivedFrame::new(
            buffer,
            self.cluster,
            self.max_frame,
            self.client,
            sequence,
            self.protocol,
        )
    }
}
