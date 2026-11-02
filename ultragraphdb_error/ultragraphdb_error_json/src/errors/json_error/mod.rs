/*
 * Copyright 2010 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.27, src/encoding/json/v2_scanner.go (SyntaxError), v2_decode.go (UnmarshalTypeError) and
 * v2_stream.go (the io errors Decoder.Decode returns). Changed for ultragraphdb: translated to Rust; the errors are
 * one enum.
 * Copyright (c) 2026. Marvin Hansen.
 */

mod json_error_display;
mod json_error_error;

/// Why Go's `encoding/json` refused its input. `Display` gives Go's text, which Dgraph returns to clients.
///
/// The classification is the public field `.0`; branch on it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct JsonError(pub JsonErrorEnum);

/// The classification of a [`JsonError`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum JsonErrorEnum {
    /// Go's `io.EOF` from `Decoder.Decode`: the stream holds no further value, `EOF`.
    Eof,
    /// Go's `io.ErrUnexpectedEOF` from `Decoder.Decode`: the stream ended inside a value, `unexpected EOF`.
    UnexpectedEof,
    /// Go's `SyntaxError`: the text is not JSON. `msg` is Go's text, e.g. `invalid character 'x' looking for
    /// beginning of value`; the error occurred after reading `offset` bytes.
    Syntax {
        /// Go's text of the syntax error.
        msg: String,
        /// The number of bytes read before the error.
        offset: i64,
    },
    /// Go's `UnmarshalTypeError`: a value that does not fit the Go type it is decoded into. `value` is the JSON
    /// kind, with the number's text for a number out of range (`number 1e400`); `struct_name` and `field` locate
    /// it below a struct, both empty at the root.
    UnmarshalType {
        /// The JSON kind of the value, with a number's text for a number out of range.
        value: String,
        /// The Go type the value was decoded into.
        go_type: String,
        /// The struct holding the field, empty at the root.
        struct_name: String,
        /// The field's path below the struct, empty at the root.
        field: String,
    },
    /// The reader failed; its error's text, which Go returns as it is.
    Read(String),
}

impl JsonError {
    /// The stream holds no further value: Go's `io.EOF`.
    #[allow(non_snake_case)]
    pub fn Eof() -> Self {
        Self(JsonErrorEnum::Eof)
    }

    /// The stream ended inside a value: Go's `io.ErrUnexpectedEOF`.
    #[allow(non_snake_case)]
    pub fn UnexpectedEof() -> Self {
        Self(JsonErrorEnum::UnexpectedEof)
    }

    /// The text is not JSON: Go's `SyntaxError` with its text `msg`, after `offset` bytes.
    #[allow(non_snake_case)]
    pub fn Syntax(msg: String, offset: i64) -> Self {
        Self(JsonErrorEnum::Syntax { msg, offset })
    }

    /// A value of JSON kind `value` does not fit the Go type `go_type`: Go's `UnmarshalTypeError`.
    #[allow(non_snake_case)]
    pub fn UnmarshalType(
        value: String,
        go_type: String,
        struct_name: String,
        field: String,
    ) -> Self {
        Self(JsonErrorEnum::UnmarshalType {
            value,
            go_type,
            struct_name,
            field,
        })
    }

    /// The reader failed with the text `msg`.
    #[allow(non_snake_case)]
    pub fn Read(msg: String) -> Self {
        Self(JsonErrorEnum::Read(msg))
    }
}
