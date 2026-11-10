/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::fmt;

use crate::errors::body_error::{BodyError, BodyErrorEnum};

impl fmt::Display for BodyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            BodyErrorEnum::BodyTooShort { len, need } => write!(
                f,
                "a body of {len} bytes is shorter than its {need}-byte fixed part and section table"
            ),
            BodyErrorEnum::SectionOutOfBounds {
                section,
                offset,
                len,
                body,
            } => write!(
                f,
                "section {section}, {len} bytes at offset {offset}, runs past the body's {body} bytes"
            ),
            BodyErrorEnum::SectionOffset {
                section,
                offset,
                expected,
            } => write!(
                f,
                "section {section} starts at offset {offset}, not at {expected}"
            ),
            BodyErrorEnum::PaddingNotZero { offset } => {
                write!(f, "padding byte {offset} of a body is not zero")
            }
            BodyErrorEnum::TrailingBytes { end, len } => write!(
                f,
                "a body holds {len} bytes, past the end of its last section at {end}"
            ),
            BodyErrorEnum::SectionLength { section, len, unit } => write!(
                f,
                "section {section}, {len} bytes, is not a whole number of {unit}-byte records"
            ),
            BodyErrorEnum::CountMismatch {
                field,
                count,
                expected,
            } => write!(f, "{field} counts {count}, and the body holds {expected}"),
            BodyErrorEnum::NotUtf8 { section, offset } => write!(
                f,
                "the text of section {section} is not UTF-8 at offset {offset}"
            ),
            BodyErrorEnum::IndexOutOfRange {
                section,
                at,
                index,
                limit,
            } => write!(
                f,
                "entry {at} of section {section} is {index}, past its bound {limit}"
            ),
        }
    }
}
