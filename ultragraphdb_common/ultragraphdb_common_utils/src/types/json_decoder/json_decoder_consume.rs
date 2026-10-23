/*
 * Copyright 2020 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.27, src/encoding/json/jsontext/decode.go (ReadValue, consumeWhitespace, consumeValue,
 * consumeLiteral, consumeString, consumeNumber, consumeObject, consumeArray). Changed for ultragraphdb: translated
 * to Rust; objects and arrays are consumed on a stack of the open ones, not by recursion; positions are absolute
 * offsets in the stream, and a reader is only ever asked for more at the end of the buffer.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::io::Read;

use ultragraphdb_error_json::JsonError;

use super::{JsonDecoder, kind};
use crate::types::scan_error::ScanError;
use crate::utils::json_wire::{self, NumberState};

/// jsontext's `maxNestingDepth`: objects and arrays nest at most this deep.
const MAX_NESTING_DEPTH: usize = 10000;

/// A failed consumption: the absolute offset it reached and why it stopped.
type Failure = (usize, ScanError);

impl<R: Read> JsonDecoder<R> {
    /// jsontext's `ReadValue` at the top level: the absolute start and end of the next value, which becomes the
    /// previous value.
    pub(crate) fn read_value(&mut self) -> Result<(usize, usize), JsonError> {
        self.invalidate_previous_read();
        let pos = match self.consume_whitespace(self.prev_end) {
            Ok(pos) => pos,
            // Nothing but whitespace after the last top-level value: io.EOF.
            Err((_, ScanError::UnexpectedEof)) => return Err(JsonError::Eof()),
            Err((pos, e)) => return Err(e.at(pos)),
        };
        // Top-level values take no delimiter, so a colon or comma at the start of one is invalid.
        if matches!(self.from(pos).first(), Some(b':' | b',')) {
            return Err(ScanError::invalid_character(self.from(pos), "at start of value").at(pos));
        }
        let end = self.consume_value(pos, 1).map_err(|(at, e)| e.at(at))?;
        self.prev_start = pos;
        self.prev_end = end;
        Ok((pos, end))
    }

    /// jsontext's `consumeWhitespace`: the absolute offset of the first byte after the whitespace at `pos`,
    /// reading more as needed. When it succeeds a byte is buffered there.
    pub(super) fn consume_whitespace(&mut self, mut pos: usize) -> Result<usize, Failure> {
        loop {
            pos += json_wire::consume_whitespace(self.from(pos));
            if !self.need_more(pos) {
                return Ok(pos);
            }
            self.fetch().map_err(|e| (pos, e))?;
        }
    }

    /// jsontext's `consumeValue`: the absolute end of the value at `pos`, whose first byte is buffered, nested
    /// `depth` deep. The objects and arrays open around the current value are on a stack.
    fn consume_value(&mut self, mut pos: usize, depth: usize) -> Result<usize, Failure> {
        let mut open: Vec<u8> = Vec::new();
        'value: loop {
            let c = self.from(pos).first().copied().unwrap_or_default();
            match kind(c) {
                b'n' => pos = self.consume_literal(pos, "null")?,
                b'f' => pos = self.consume_literal(pos, "false")?,
                b't' => pos = self.consume_literal(pos, "true")?,
                b'"' => pos = self.consume_string(pos)?,
                b'0' => pos = self.consume_number(pos)?,
                b'{' | b'[' => {
                    if depth + open.len() == MAX_NESTING_DEPTH + 1 {
                        return Err((pos, ScanError::MaxDepth));
                    }
                    pos = self.consume_whitespace(pos + 1)?;
                    let close = if c == b'{' { b'}' } else { b']' };
                    if self.from(pos).first() == Some(&close) {
                        pos += 1;
                    } else {
                        open.push(c);
                        if c == b'{' {
                            pos = self.consume_name(pos)?;
                        }
                        continue 'value;
                    }
                }
                _ => {
                    let e = ScanError::invalid_character(self.from(pos), "at start of value");
                    return Err((pos, e));
                }
            }
            // After a value: the end of the outermost one, or the delimiter of the object or array it is in.
            while let Some(&top) = open.last() {
                pos = self.consume_whitespace(pos)?;
                match (top, self.from(pos).first().copied().unwrap_or_default()) {
                    (_, b',') => {
                        pos = if top == b'{' {
                            self.consume_name(pos + 1)?
                        } else {
                            self.consume_whitespace(pos + 1)?
                        };
                        continue 'value;
                    }
                    (b'{', b'}') | (b'[', b']') => {
                        pos += 1;
                        open.pop();
                    }
                    (b'{', _) => {
                        let where_ = "after object value (expecting ',' or '}')";
                        return Err((pos, ScanError::invalid_character(self.from(pos), where_)));
                    }
                    _ => {
                        let where_ = "after array element (expecting ',' or ']')";
                        return Err((pos, ScanError::invalid_character(self.from(pos), where_)));
                    }
                }
            }
            return Ok(pos);
        }
    }

    /// An object member's name and colon from `pos`, which may be whitespace: the absolute offset of its value,
    /// whose first byte is buffered.
    fn consume_name(&mut self, pos: usize) -> Result<usize, Failure> {
        let pos = self.consume_whitespace(pos)?;
        let pos = self.consume_string(pos)?;
        let pos = self.consume_whitespace(pos)?;
        if self.from(pos).first() != Some(&b':') {
            let e =
                ScanError::invalid_character(self.from(pos), "after object name (expecting ':')");
            return Err((pos, e));
        }
        self.consume_whitespace(pos + 1)
    }

    /// jsontext's `consumeLiteral`.
    fn consume_literal(&mut self, pos: usize, lit: &str) -> Result<usize, Failure> {
        loop {
            match json_wire::consume_literal(self.from(pos), lit) {
                (n, Some(ScanError::UnexpectedEof)) => self.fetch().map_err(|e| (pos + n, e))?,
                (n, Some(e)) => return Err((pos + n, e)),
                (n, None) => return Ok(pos + n),
            }
        }
    }

    /// jsontext's `consumeString`, resuming where the buffer ended.
    fn consume_string(&mut self, pos: usize) -> Result<usize, Failure> {
        let mut resume = 0;
        loop {
            match json_wire::consume_string_resumable(self.from(pos), resume) {
                (n, Some(ScanError::UnexpectedEof)) => {
                    resume = n;
                    self.fetch().map_err(|e| (pos + n, e))?;
                }
                (n, Some(e)) => return Err((pos + n, e)),
                (n, None) => return Ok(pos + n),
            }
        }
    }

    /// jsontext's `consumeNumber`. A number is not self-terminating, so one ending at the end of the buffer ends
    /// only when the stream does; a number cut short there fails at its start.
    fn consume_number(&mut self, pos: usize) -> Result<usize, Failure> {
        let (mut n, mut state) = (0, NumberState::Init);
        loop {
            let (n2, state2, err) = json_wire::consume_number_resumable(self.from(pos), n, state);
            (n, state) = (n2, state2);
            if err == Some(ScanError::UnexpectedEof) || self.need_more(pos + n) {
                let may_terminate = err.is_none();
                match self.fetch() {
                    Ok(()) => continue,
                    Err(ScanError::UnexpectedEof) if may_terminate => return Ok(pos + n),
                    Err(e) => return Err((pos, e)),
                }
            }
            return match err {
                Some(e) => Err((pos + n, e)),
                None => Ok(pos + n),
            };
        }
    }
}
