/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, x/keys.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

mod key_error_display;
mod key_error_error;

use crate::types::parsed_key::ParsedKey;

/// Why a key could not be parsed or split. `Display` gives Dgraph's error text.
///
/// The wrapped classification is private; branch on [`KeyError::kind`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyError(KeyErrorEnum);

/// The classification of a [`KeyError`]. Each `key` is the key past its prefix byte and namespace, which is what
/// Dgraph's messages print; each `parsed` is the key as far as it was parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum KeyErrorEnum {
    /// The key is shorter than its prefix and namespace.
    KeyTooShort,
    /// The key ends before its attribute's length and first byte.
    InvalidFormat { key: Vec<u8> },
    /// The attribute is longer than the rest of the key.
    InvalidSize { size: usize, key: Vec<u8> },
    /// The attribute is not UTF-8. Dgraph's attributes are Go strings, which accept it; this port's are not.
    AttributeNotUtf8 { key: Vec<u8> },
    /// A data or reverse key's uid is shorter than 8 bytes.
    UidTooShort { key: Vec<u8>, parsed: ParsedKey },
    /// A data or reverse key's uid is 0.
    ZeroUid { key: Vec<u8> },
    /// A split data, reverse or count key's start uid is not exactly 8 bytes.
    StartUidLength { key: Vec<u8>, parsed: ParsedKey },
    /// A split index key is shorter than its start uid.
    StartUidTooShort { key: Vec<u8>, parsed: ParsedKey },
    /// A count key's count is shorter than 4 bytes.
    CountTooShort { key: Vec<u8>, parsed: ParsedKey },
    /// The data type byte is none of data, index, reverse, count and reverse count.
    InvalidDataType,
    /// Only a key with the default prefix can be split.
    SplitNonDefaultPrefix,
}

impl KeyError {
    pub(crate) fn new(kind: KeyErrorEnum) -> Self {
        Self(kind)
    }

    /// Classification, for callers that need to branch on the failure.
    pub fn kind(&self) -> &KeyErrorEnum {
        &self.0
    }
}
