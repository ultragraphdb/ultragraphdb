/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod quad_key_error_display;
mod quad_key_error_error;

use ultragraphdb_core_ids::IdError;
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
    /// Bytes follow the fields its keyspace defines, or a split key's start is neither 8 nor 16 bytes.
    TrailingBytes {
        /// The key's bytes.
        key: Vec<u8>,
    },
    /// The first byte is no keyspace's, or carries the split flag on a keyspace without lists.
    UnknownKeyspace {
        /// The key's bytes.
        key: Vec<u8>,
    },
    /// An ID other than the default graph's is not a valid system ID; `field` names it.
    InvalidId {
        /// The key's bytes.
        key: Vec<u8>,
        /// The field holding the ID.
        field: &'static str,
        /// Why the ID is not valid.
        error: IdError,
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
    /// A mark key's bucket group is 2^18 or more.
    GroupRange {
        /// The key's bytes.
        key: Vec<u8>,
        /// The group the key holds.
        group: u32,
    },
    /// A value-index key splits at an ID alone, since its token runs to its start.
    FingerprintStart {
        /// The key's bytes.
        key: Vec<u8>,
    },
}

impl QuadKeyError {
    /// A [`QuadKeyErrorEnum::Truncated`] for `key`.
    #[must_use]
    pub fn truncated(key: &[u8]) -> Self {
        Self(QuadKeyErrorEnum::Truncated { key: key.to_vec() })
    }

    /// A [`QuadKeyErrorEnum::TrailingBytes`] for `key`.
    #[must_use]
    pub fn trailing_bytes(key: &[u8]) -> Self {
        Self(QuadKeyErrorEnum::TrailingBytes { key: key.to_vec() })
    }

    /// A [`QuadKeyErrorEnum::UnknownKeyspace`] for `key`.
    #[must_use]
    pub fn unknown_keyspace(key: &[u8]) -> Self {
        Self(QuadKeyErrorEnum::UnknownKeyspace { key: key.to_vec() })
    }

    /// A [`QuadKeyErrorEnum::InvalidId`] for `key`, naming the `field` and why its ID is not valid.
    #[must_use]
    pub fn invalid_id(key: &[u8], field: &'static str, error: IdError) -> Self {
        Self(QuadKeyErrorEnum::InvalidId {
            key: key.to_vec(),
            field,
            error,
        })
    }

    /// A [`QuadKeyErrorEnum::UnknownDirection`] for `key`.
    #[must_use]
    pub fn unknown_direction(key: &[u8]) -> Self {
        Self(QuadKeyErrorEnum::UnknownDirection { key: key.to_vec() })
    }

    /// A [`QuadKeyErrorEnum::BadObject`] for `key`, with the object's `error`.
    #[must_use]
    pub fn bad_object(key: &[u8], error: TermError) -> Self {
        Self(QuadKeyErrorEnum::BadObject {
            key: key.to_vec(),
            error,
        })
    }

    /// A [`QuadKeyErrorEnum::VectorObject`] for `key`.
    #[must_use]
    pub fn vector_object(key: &[u8]) -> Self {
        Self(QuadKeyErrorEnum::VectorObject { key: key.to_vec() })
    }

    /// A [`QuadKeyErrorEnum::NotAList`] for `key`.
    #[must_use]
    pub fn not_a_list(key: &[u8]) -> Self {
        Self(QuadKeyErrorEnum::NotAList { key: key.to_vec() })
    }

    /// A [`QuadKeyErrorEnum::GroupRange`] for `key`, holding `group`.
    #[must_use]
    pub fn group_range(key: &[u8], group: u32) -> Self {
        Self(QuadKeyErrorEnum::GroupRange {
            key: key.to_vec(),
            group,
        })
    }

    /// A [`QuadKeyErrorEnum::FingerprintStart`] for `key`.
    #[must_use]
    pub fn fingerprint_start(key: &[u8]) -> Self {
        Self(QuadKeyErrorEnum::FingerprintStart { key: key.to_vec() })
    }
}
