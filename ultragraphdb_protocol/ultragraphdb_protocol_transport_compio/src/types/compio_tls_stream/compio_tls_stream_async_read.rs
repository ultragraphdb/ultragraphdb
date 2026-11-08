/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io;

use compio::BufResult;
use compio::buf::IoBufMut;
use compio::io::{AsyncRead, AsyncWrite};
use ultragraphdb_transport_tls::{Plaintext, READ_TURNS_MAX};

use super::CompioTlsStream;

impl<S: AsyncRead + AsyncWrite> AsyncRead for CompioTlsStream<S> {
    /// The peer's plaintext, reading records from the stream until some arrives: 0 once the peer sent its
    /// `close_notify`, `UnexpectedEof` when the stream closed without it. A future dropped at a wait leaves what
    /// `CompioTlsStream::write_records` or `CompioTlsStream::read_records` says; plaintext is taken from the session
    /// only when it is copied into `buf` in the same poll.
    async fn read<B: IoBufMut>(&mut self, mut buf: B) -> BufResult<usize, B> {
        // A compio read fills the buffer from its start, as the reader of a byte slice below does.
        let room = buf.buf_capacity();
        if room == 0 {
            return BufResult(Ok(0), buf);
        }
        for _ in 0..READ_TURNS_MAX {
            match self.take_plaintext(room) {
                // compio's own reader of a byte slice copies into the buffer; the plaintext was cut to its room.
                Plaintext::Read(n) => {
                    let mut source: &[u8] = self.plaintext(n);
                    return source.read(buf).await;
                }
                Plaintext::Closed => return BufResult(Ok(0), buf),
                Plaintext::Truncated => {
                    return BufResult(Err(io::ErrorKind::UnexpectedEof.into()), buf);
                }
                Plaintext::Pending => {}
            }
            // What the session owes the peer, such as an answer to a key update, goes out before it waits.
            if let Err(e) = self.write_records().await {
                return BufResult(Err(e), buf);
            }
            if let Err(e) = self.read_records().await {
                return BufResult(Err(e), buf);
            }
        }
        let exhausted = io::Error::other(format!("no plaintext after {READ_TURNS_MAX} reads"));
        BufResult(Err(exhausted), buf)
    }
}
