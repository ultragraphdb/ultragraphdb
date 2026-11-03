/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io::{self, ErrorKind, Read, Write};

use super::{READ_TURNS_MAX, TlsStream};
use crate::types::plaintext::Plaintext;

impl<S: Read + Write> Read for TlsStream<S> {
    /// The peer's plaintext, reading records from the stream until some arrives: 0 once the peer sent its
    /// `close_notify`, and `UnexpectedEof` when the stream closed without it.
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        for _ in 0..READ_TURNS_MAX {
            match self.session.read(buf) {
                Plaintext::Read(n) => return Ok(n),
                Plaintext::Closed => return Ok(0),
                Plaintext::Truncated => return Err(ErrorKind::UnexpectedEof.into()),
                Plaintext::Pending => {}
            }
            // What the session owes the peer, such as an answer to a key update, goes out before it waits.
            self.write_records()?;
            self.read_records()?;
        }
        Err(io::Error::other(format!(
            "no plaintext after {READ_TURNS_MAX} reads"
        )))
    }
}
