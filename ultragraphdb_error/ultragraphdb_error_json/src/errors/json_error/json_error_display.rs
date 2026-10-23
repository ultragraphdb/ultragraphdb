/*
 * Copyright 2010 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.27, src/encoding/json/v2_decode.go (UnmarshalTypeError.Error). Changed for ultragraphdb:
 * translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::fmt;

use super::{JsonError, JsonErrorEnum};

impl fmt::Display for JsonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            JsonErrorEnum::Eof => f.write_str("EOF"),
            JsonErrorEnum::UnexpectedEof => f.write_str("unexpected EOF"),
            JsonErrorEnum::Syntax { msg, .. } | JsonErrorEnum::Read(msg) => f.write_str(msg),
            JsonErrorEnum::UnmarshalType {
                value,
                go_type,
                struct_name,
                field,
            } if !struct_name.is_empty() || !field.is_empty() => {
                // A path whose last token is an index into a JSON array likely leads into a Go slice or array.
                let last = field.rsplit('.').next().unwrap_or_default();
                let into = if !last.is_empty() && last.bytes().all(|c| c.is_ascii_digit()) {
                    ""
                } else {
                    "Go struct field "
                };
                write!(
                    f,
                    "json: cannot unmarshal {value} into {into}{struct_name}.{field} of type {go_type}"
                )
            }
            JsonErrorEnum::UnmarshalType { value, go_type, .. } => {
                write!(
                    f,
                    "json: cannot unmarshal {value} into Go value of type {go_type}"
                )
            }
        }
    }
}
