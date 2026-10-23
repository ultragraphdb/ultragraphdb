/*
 * Copyright 2020 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.27, src/encoding/json/jsontext/decode.go (fetch, invalidatePreviousRead). Changed for
 * ultragraphdb: translated to Rust; a read of zero bytes is the end of the stream, as Rust's Read reports it.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::io::{ErrorKind, Read};

use super::JsonDecoder;
use crate::types::scan_error::ScanError;

/// jsontext's `maxBufferSize`: the buffer grows by itself up to this size.
const MAX_BUFFER_SIZE: usize = 4 << 10;

impl<R: Read> JsonDecoder<R> {
    /// jsontext's `invalidatePreviousRead`: the previous value is not kept for the next read.
    pub(super) fn invalidate_previous_read(&mut self) {
        if self.stream && self.prev_start < self.prev_end {
            self.prev_start = self.prev_end;
        }
    }

    /// jsontext's `fetch`: reads at least one more byte into the buffer, dropping what precedes the previous value
    /// and growing the buffer when a value fills it.
    ///
    /// # Errors
    ///
    /// [`ScanError::UnexpectedEof`] at the end of the stream, [`ScanError::Read`] when the reader fails.
    pub(super) fn fetch(&mut self) -> Result<(), ScanError> {
        if !self.stream {
            return Err(ScanError::UnexpectedEof);
        }
        if self.cap == 0 {
            self.cap = 64;
        }
        // Grow below the maximum size once enough has been read, or when a value that started the buffer fills
        // three quarters of it.
        let grow = self.cap <= MAX_BUFFER_SIZE / 2 && self.cap < self.prev_end / 2;
        let grow = grow || (self.prev_start == self.base && self.buf.len() >= 3 * self.cap / 4);
        if grow {
            self.cap *= 2;
        }
        // Move the unread part to the front.
        self.buf.drain(..self.prev_start - self.base);
        self.base = self.prev_start;
        // The buffer is never full here: a value filling it from its start grows it.
        let len = self.buf.len();
        self.cap = self.cap.max(len + 1);
        self.buf.resize(self.cap, 0);
        let read = loop {
            match self.rd.read(&mut self.buf[len..]) {
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                other => break other,
            }
        };
        match read {
            Ok(n) => {
                self.buf.truncate(len + n);
                if n > 0 {
                    Ok(())
                } else {
                    Err(ScanError::UnexpectedEof)
                }
            }
            Err(e) => {
                self.buf.truncate(len);
                Err(ScanError::Read(e.to_string()))
            }
        }
    }
}
