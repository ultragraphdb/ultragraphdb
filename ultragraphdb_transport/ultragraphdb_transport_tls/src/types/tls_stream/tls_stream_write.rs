/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io::{self, Read, Write};

use super::TlsStream;

impl<S: Read + Write> Write for TlsStream<S> {
    /// Sends `buf` whole as records, written to the stream before this returns.
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.session
            .send(buf)
            .map_err(|e| io::Error::other(e.to_string()))?;
        self.write_records()?;
        Ok(buf.len())
    }

    /// Writes every record waiting and flushes the stream.
    fn flush(&mut self) -> io::Result<()> {
        self.write_records()
    }
}
