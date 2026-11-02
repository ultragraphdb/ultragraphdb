/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod quad_key_error_display;
mod quad_key_error_error;

use ultragraphdb_core_terms::TermError;

/// Why a quad key could not be parsed or split. Every variant names the key's bytes. The classification is the public
/// field `.0`; branch on it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QuadKeyError(pub QuadKeyErrorEnum);

/// The classification of a [`QuadKeyError`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum QuadKeyErrorEnum {
    /// The key ends before the fields its keyspace defines.
    Truncated {
        /// The key's bytes.
        key: Vec<u8>,
    },
    /// Bytes follow the fields its keyspace defines.
    TrailingBytes {
        /// The key's bytes.
        key: Vec<u8>,
    },
    /// The first byte is no keyspace's.
    UnknownKeyspace {
        /// The key's bytes.
        key: Vec<u8>,
    },
    /// A UID other than the default graph's, or a statement ID, is 0; `field` names it.
    ZeroUid {
        /// The key's bytes.
        key: Vec<u8>,
        /// The field that is 0.
        field: &'static str,
    },
    /// A count index key's direction byte is neither direction.
    UnknownDirection {
        /// The key's bytes.
        key: Vec<u8>,
    },
    /// An object's encoding cannot be read.
    BadObject {
        /// The key's bytes.
        key: Vec<u8>,
        /// Why the object's encoding cannot be read.
        error: TermError,
    },
    /// A vector names no object of a key.
    VectorObject {
        /// The key's bytes.
        key: Vec<u8>,
    },
    /// Only a key of a list keyspace splits.
    NotAList {
        /// The key's bytes.
        key: Vec<u8>,
    },
}

impl QuadKeyError {
    /// A [`QuadKeyErrorEnum::Truncated`] for `key`.
    pub fn truncated(key: &[u8]) -> Self {
        Self(QuadKeyErrorEnum::Truncated { key: key.to_vec() })
    }

    /// A [`QuadKeyErrorEnum::TrailingBytes`] for `key`.
    pub fn trailing_bytes(key: &[u8]) -> Self {
        Self(QuadKeyErrorEnum::TrailingBytes { key: key.to_vec() })
    }

    /// A [`QuadKeyErrorEnum::UnknownKeyspace`] for `key`.
    pub fn unknown_keyspace(key: &[u8]) -> Self {
        Self(QuadKeyErrorEnum::UnknownKeyspace { key: key.to_vec() })
    }

    /// A [`QuadKeyErrorEnum::ZeroUid`] for `key`, naming the zero `field`.
    pub fn zero_uid(key: &[u8], field: &'static str) -> Self {
        Self(QuadKeyErrorEnum::ZeroUid {
            key: key.to_vec(),
            field,
        })
    }

    /// A [`QuadKeyErrorEnum::UnknownDirection`] for `key`.
    pub fn unknown_direction(key: &[u8]) -> Self {
        Self(QuadKeyErrorEnum::UnknownDirection { key: key.to_vec() })
    }

    /// A [`QuadKeyErrorEnum::BadObject`] for `key`, with the object's `error`.
    pub fn bad_object(key: &[u8], error: TermError) -> Self {
        Self(QuadKeyErrorEnum::BadObject {
            key: key.to_vec(),
            error,
        })
    }

    /// A [`QuadKeyErrorEnum::VectorObject`] for `key`.
    pub fn vector_object(key: &[u8]) -> Self {
        Self(QuadKeyErrorEnum::VectorObject { key: key.to_vec() })
    }

    /// A [`QuadKeyErrorEnum::NotAList`] for `key`.
    pub fn not_a_list(key: &[u8]) -> Self {
        Self(QuadKeyErrorEnum::NotAList { key: key.to_vec() })
    }
}
