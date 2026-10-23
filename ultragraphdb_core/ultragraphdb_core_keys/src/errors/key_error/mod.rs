/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, x/keys.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

mod key_error_display;
mod key_error_error;

use ultragraphdb_error_num::NumError;

use crate::types::parsed_key::ParsedKey;

/// Why a key could not be built, parsed or split, a namespaced attribute could not be parsed, or a reserved
/// namespace could not be registered. `Display` gives Dgraph's error text, or for a failure on which Dgraph panics,
/// its panic text or a text naming the input and the reason.
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
    /// A namespaced attribute has no namespace separator, on which Dgraph panics indexing past its split.
    MissingNamespaceSeparator { attr: String },
    /// A namespaced attribute's namespace is not a hexadecimal u64, on which Dgraph's `strToUint` exits the process;
    /// `error` is Go's `strconv` error.
    InvalidNamespace { attr: String, error: NumError },
    /// A predicate of `len` bytes, `u16::MAX` or more, does not fit a key's attribute length, on which Dgraph's
    /// `AssertTrue` exits the process.
    AttributeTooLong { len: usize },
    /// A reserved namespace locks values without a trust marker. Dgraph panics with the same text.
    TrustMarkerMissing,
    /// A reserved namespace names a predicate, type, prefix or lock with the namespace separator. Dgraph panics with
    /// the same text.
    QualifiedReservedName { name: String },
    /// A reserved namespace locks the empty prefix. Dgraph panics with the same text.
    EmptyValueLockedPrefix,
    /// A reserved name is registered twice; `kind` is `predicate`, `type`, `value-locked predicate` or
    /// `value-locked prefix`. Dgraph panics with the same text.
    AlreadyRegistered { kind: &'static str, name: String },
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

    #[allow(non_snake_case)]
    pub fn MissingNamespaceSeparator(attr: String) -> Self {
        Self(KeyErrorEnum::MissingNamespaceSeparator { attr })
    }

    #[allow(non_snake_case)]
    pub fn InvalidNamespace(attr: String, error: NumError) -> Self {
        Self(KeyErrorEnum::InvalidNamespace { attr, error })
    }

    #[allow(non_snake_case)]
    pub fn AttributeTooLong(len: usize) -> Self {
        Self(KeyErrorEnum::AttributeTooLong { len })
    }

    #[allow(non_snake_case)]
    pub fn TrustMarkerMissing() -> Self {
        Self(KeyErrorEnum::TrustMarkerMissing)
    }

    #[allow(non_snake_case)]
    pub fn QualifiedReservedName(name: String) -> Self {
        Self(KeyErrorEnum::QualifiedReservedName { name })
    }

    #[allow(non_snake_case)]
    pub fn EmptyValueLockedPrefix() -> Self {
        Self(KeyErrorEnum::EmptyValueLockedPrefix)
    }

    #[allow(non_snake_case)]
    pub fn AlreadyRegistered(kind: &'static str, name: String) -> Self {
        Self(KeyErrorEnum::AlreadyRegistered { kind, name })
    }
}
