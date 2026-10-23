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
/// The classification is the public field `.0`; branch on it.
///
/// No `Hash`: [`ParsedKey`], which several variants keep, does not derive it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyError(pub KeyErrorEnum);

/// The classification of a [`KeyError`]. Each `key` is the key past its prefix byte and namespace, which is what
/// Dgraph's messages print; each `parsed` is the key as far as it was parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
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

    #[allow(non_snake_case)]
    pub fn KeyTooShort() -> Self {
        Self(KeyErrorEnum::KeyTooShort)
    }

    #[allow(non_snake_case)]
    pub fn InvalidFormat(key: Vec<u8>) -> Self {
        Self(KeyErrorEnum::InvalidFormat { key })
    }

    #[allow(non_snake_case)]
    pub fn InvalidSize(size: usize, key: Vec<u8>) -> Self {
        Self(KeyErrorEnum::InvalidSize { size, key })
    }

    #[allow(non_snake_case)]
    pub fn AttributeNotUtf8(key: Vec<u8>) -> Self {
        Self(KeyErrorEnum::AttributeNotUtf8 { key })
    }

    #[allow(non_snake_case)]
    pub fn UidTooShort(key: Vec<u8>, parsed: ParsedKey) -> Self {
        Self(KeyErrorEnum::UidTooShort { key, parsed })
    }

    #[allow(non_snake_case)]
    pub fn ZeroUid(key: Vec<u8>) -> Self {
        Self(KeyErrorEnum::ZeroUid { key })
    }

    #[allow(non_snake_case)]
    pub fn StartUidLength(key: Vec<u8>, parsed: ParsedKey) -> Self {
        Self(KeyErrorEnum::StartUidLength { key, parsed })
    }

    #[allow(non_snake_case)]
    pub fn StartUidTooShort(key: Vec<u8>, parsed: ParsedKey) -> Self {
        Self(KeyErrorEnum::StartUidTooShort { key, parsed })
    }

    #[allow(non_snake_case)]
    pub fn CountTooShort(key: Vec<u8>, parsed: ParsedKey) -> Self {
        Self(KeyErrorEnum::CountTooShort { key, parsed })
    }

    #[allow(non_snake_case)]
    pub fn InvalidDataType() -> Self {
        Self(KeyErrorEnum::InvalidDataType)
    }

    #[allow(non_snake_case)]
    pub fn SplitNonDefaultPrefix() -> Self {
        Self(KeyErrorEnum::SplitNonDefaultPrefix)
    }
}
