/*
 * Copyright 2010 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.27, src/encoding/json/v2_stream.go (Decoder) and jsontext/decode.go (the decoder state and
 * buffer beneath it). Changed for ultragraphdb: translated to Rust; numbers decode as with UseNumber, into a
 * JsonValue; nesting is tracked on a stack rather than by recursion.
 * Copyright (c) 2026. Marvin Hansen.
 */

mod json_decoder_consume;
mod json_decoder_fetch;

use std::io::Read;

use ultragraphdb_error_json::{JsonError, JsonErrorEnum};

use crate::types::json_value::JsonValue;
use crate::types::scan_error::ScanError;
use crate::utils::json_wire;

/// Go's `json.Decoder` with `UseNumber`: reads JSON values one after the other from a stream, each separated by
/// optional whitespace, and decodes each as Go decodes it into an `interface{}`, a number keeping its text.
///
/// As Go 1.27's (whose `encoding/json` runs on `encoding/json/jsontext`), it reads into a buffer of 64 bytes that
/// grows as values need it, so it may read beyond the value it decodes; [`JsonDecoder::buffered`] is what it read
/// and has not decoded. A syntax error, or the end of the stream, is returned again by every later call.
#[derive(Debug)]
pub struct JsonDecoder<R: Read> {
    rd: R,
    /// Whether `rd` is the input; Go's buffered decoder (`rd == nil`) holds all input in `buf` from the start.
    stream: bool,
    /// The bytes read and not dropped, from absolute offset `base`; `cap` is Go's `cap(buf)`, what a read fills.
    buf: Vec<u8>,
    cap: usize,
    base: usize,
    /// The absolute offsets of the previous value: Go's `prevStart` and `prevEnd`.
    prev_start: usize,
    prev_end: usize,
    err: Option<JsonError>,
}

impl<R: Read> JsonDecoder<R> {
    /// Go's `json.NewDecoder(r)` followed by `UseNumber`.
    pub fn new(rd: R) -> Self {
        Self {
            rd,
            stream: true,
            buf: Vec::new(),
            cap: 0,
            base: 0,
            prev_start: 0,
            prev_end: 0,
            err: None,
        }
    }

    /// Go's `Decoder.Decode` into an `interface{}`: the next JSON value of the stream.
    ///
    /// # Errors
    ///
    /// [`JsonErrorEnum::Eof`] when only whitespace is left, [`JsonErrorEnum::UnexpectedEof`] when the stream ends
    /// inside a value, [`JsonErrorEnum::Syntax`] with Go's text and offset when the value is not JSON, and
    /// [`JsonErrorEnum::Read`] when the reader fails.
    pub fn decode(&mut self) -> Result<JsonValue, JsonError> {
        if let Some(e) = &self.err {
            return Err(e.clone());
        }
        match self.read_value() {
            Ok((start, end)) => Ok(JsonValue::from_valid(
                &self.buf[start - self.base..end - self.base],
            )),
            Err(e) => {
                // Decode has always reported truncated input as io.ErrUnexpectedEOF, unlike Unmarshal.
                let e = match e.0 {
                    JsonErrorEnum::Syntax { ref msg, .. }
                        if msg == "unexpected end of JSON input" =>
                    {
                        JsonError::UnexpectedEof()
                    }
                    _ => e,
                };
                self.err = Some(e.clone());
                Err(e)
            }
        }
    }

    /// jsontext's `Decoder.CheckEOF`, which `json.Unmarshal` applies after its value: whether nothing but whitespace
    /// follows the values decoded so far. The character it refuses is named whole, as `json.Unmarshal` names it,
    /// though the stream may have cut it.
    ///
    /// # Errors
    ///
    /// The error [`JsonDecoder::decode`] returned, if it failed; otherwise [`JsonErrorEnum::Syntax`], `invalid
    /// character 'x' after top-level value`, at what follows, or [`JsonErrorEnum::Read`].
    pub fn check_eof(&mut self) -> Result<(), JsonError> {
        if let Some(e) = &self.err {
            return Err(e.clone());
        }
        match self.consume_whitespace(self.prev_end) {
            Ok(pos) => {
                while !json_wire::full_rune(self.from(pos)) && self.fetch().is_ok() {}
                Err(ScanError::invalid_character(self.from(pos), "after top-level value").at(pos))
            }
            Err((_, ScanError::UnexpectedEof)) => Ok(()),
            Err((pos, e)) => Err(e.at(pos)),
        }
    }

    /// Go's `Decoder.Buffered`: the bytes read from the stream and not yet decoded.
    pub fn buffered(&self) -> &[u8] {
        &self.buf[self.prev_end - self.base..]
    }

    /// Go's `Decoder.InputOffset`: the offset in the stream just after the last value decoded.
    pub fn input_offset(&self) -> i64 {
        self.prev_end as i64
    }

    /// The buffer from the absolute offset `pos`.
    fn from(&self, pos: usize) -> &[u8] {
        &self.buf[pos - self.base..]
    }

    /// Go's `needMore`: whether no byte is buffered at the absolute offset `pos`.
    fn need_more(&self, pos: usize) -> bool {
        pos >= self.base + self.buf.len()
    }
}

impl<'a> JsonDecoder<&'a [u8]> {
    /// Go's buffered decoder of `json.Unmarshal` and `json.Valid`: all of `data` in its buffer, nothing to read.
    pub(crate) fn of_bytes(data: &'a [u8]) -> Self {
        let mut d = Self::new(&data[..0]);
        d.stream = false;
        d.buf = data.to_vec();
        d.cap = data.len();
        d
    }
}

/// jsontext's `Kind.normalize`: a number's first byte as `0`, any byte that starts no value as zero.
fn kind(c: u8) -> u8 {
    match c {
        b'n' | b'f' | b't' | b'"' | b'{' | b'}' | b'[' | b']' => c,
        b'-' | b'0'..=b'9' => b'0',
        _ => 0,
    }
}
