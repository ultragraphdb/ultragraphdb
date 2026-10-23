/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, x/keys.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::fmt::{Display, Formatter, Result};

use ultragraphdb_common_utils::{go_fmt, go_strconv};

use super::{KeyError, KeyErrorEnum};
use crate::keys::NS_SEPARATOR;

impl Display for KeyError {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match &self.0 {
            KeyErrorEnum::KeyTooShort => write!(f, "Key length less than 9"),
            KeyErrorEnum::InvalidFormat { key } => {
                write!(f, "Invalid format for key {}", go_fmt::byte_slice(key))
            }
            KeyErrorEnum::InvalidSize { size, key } => {
                write!(f, "Invalid size {size} for key {}", go_fmt::byte_slice(key))
            }
            KeyErrorEnum::AttributeNotUtf8 { key } => {
                write!(
                    f,
                    "Invalid UTF-8 attribute for key {}",
                    go_fmt::byte_slice(key)
                )
            }
            KeyErrorEnum::UidTooShort { key, parsed } => {
                write!(
                    f,
                    "uid length < 8 for key: {}, parsed key: {parsed}",
                    go_strconv::quote(key)
                )
            }
            KeyErrorEnum::ZeroUid { key } => {
                write!(
                    f,
                    "Invalid UID with value 0 for key: {}",
                    go_fmt::byte_slice(key)
                )
            }
            KeyErrorEnum::StartUidLength { key, parsed } => {
                write!(
                    f,
                    "StartUid length != 8 for key: {}, parsed key: {parsed}",
                    go_strconv::quote(key)
                )
            }
            KeyErrorEnum::StartUidTooShort { key, parsed } => {
                write!(
                    f,
                    "StartUid length < 8 for key: {}, parsed key: {parsed}",
                    go_strconv::quote(key)
                )
            }
            KeyErrorEnum::CountTooShort { key, parsed } => {
                write!(
                    f,
                    "count length < 4 for key: {}, parsed key: {parsed}",
                    go_strconv::quote(key)
                )
            }
            KeyErrorEnum::InvalidDataType => write!(f, "Invalid data type"),
            KeyErrorEnum::SplitNonDefaultPrefix => {
                write!(f, "only keys with default prefix can have a split key")
            }
            KeyErrorEnum::MissingNamespaceSeparator { attr } => write!(
                f,
                "Attribute {} has no namespace separator {}",
                go_strconv::quote(attr),
                go_strconv::quote(NS_SEPARATOR)
            ),
            KeyErrorEnum::InvalidNamespace { attr, error } => write!(
                f,
                "Invalid namespace in attribute {}: {error}",
                go_strconv::quote(attr)
            ),
            KeyErrorEnum::AttributeTooLong { len } => write!(
                f,
                "Attribute of {len} bytes is too long for a key, which holds at most {}",
                u16::MAX - 1
            ),
            KeyErrorEnum::TrustMarkerMissing => write!(
                f,
                "x.RegisterReservedNamespace: ValueLocked is set but TrustMarker is nil; a value-locked predicate \
                 with no TrustMarker is unwritable by everyone, including its owner"
            ),
            KeyErrorEnum::QualifiedReservedName { name } => write!(
                f,
                "x.RegisterReservedNamespace: name {} must be bare, without the {} namespace separator",
                go_strconv::quote(name),
                go_strconv::quote(NS_SEPARATOR)
            ),
            KeyErrorEnum::EmptyValueLockedPrefix => write!(
                f,
                "x.RegisterReservedNamespace: value-locked prefix must not be empty"
            ),
            KeyErrorEnum::AlreadyRegistered { kind, name } => write!(
                f,
                "x.RegisterReservedNamespace: {kind} {} already registered",
                go_strconv::quote(name)
            ),
        }
    }
}
