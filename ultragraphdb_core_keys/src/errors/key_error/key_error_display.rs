/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, x/keys.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::fmt::{Display, Formatter, Result};

use super::{KeyError, KeyErrorEnum};
use crate::utils::go_fmt::{go_bytes, go_quote};

impl Display for KeyError {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self.kind() {
            KeyErrorEnum::KeyTooShort => write!(f, "Key length less than 9"),
            KeyErrorEnum::InvalidFormat { key } => {
                write!(f, "Invalid format for key {}", go_bytes(key))
            }
            KeyErrorEnum::InvalidSize { size, key } => {
                write!(f, "Invalid size {size} for key {}", go_bytes(key))
            }
            KeyErrorEnum::AttributeNotUtf8 { key } => {
                write!(f, "Invalid UTF-8 attribute for key {}", go_bytes(key))
            }
            KeyErrorEnum::UidTooShort { key, parsed } => {
                write!(
                    f,
                    "uid length < 8 for key: {}, parsed key: {parsed}",
                    go_quote(key)
                )
            }
            KeyErrorEnum::ZeroUid { key } => {
                write!(f, "Invalid UID with value 0 for key: {}", go_bytes(key))
            }
            KeyErrorEnum::StartUidLength { key, parsed } => {
                write!(
                    f,
                    "StartUid length != 8 for key: {}, parsed key: {parsed}",
                    go_quote(key)
                )
            }
            KeyErrorEnum::StartUidTooShort { key, parsed } => {
                write!(
                    f,
                    "StartUid length < 8 for key: {}, parsed key: {parsed}",
                    go_quote(key)
                )
            }
            KeyErrorEnum::CountTooShort { key, parsed } => {
                write!(
                    f,
                    "count length < 4 for key: {}, parsed key: {parsed}",
                    go_quote(key)
                )
            }
            KeyErrorEnum::InvalidDataType => write!(f, "Invalid data type"),
            KeyErrorEnum::SplitNonDefaultPrefix => {
                write!(f, "only keys with default prefix can have a split key")
            }
        }
    }
}
