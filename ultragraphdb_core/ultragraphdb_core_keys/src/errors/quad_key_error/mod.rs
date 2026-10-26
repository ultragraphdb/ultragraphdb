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
    Truncated { key: Vec<u8> },
    /// Bytes follow the fields its keyspace defines.
    TrailingBytes { key: Vec<u8> },
    /// The first byte is no keyspace's.
    UnknownKeyspace { key: Vec<u8> },
    /// A UID other than the default graph's, or a statement ID, is 0; `field` names it.
    ZeroUid { key: Vec<u8>, field: &'static str },
    /// A count index key's direction byte is neither direction.
    UnknownDirection { key: Vec<u8> },
    /// An object's encoding cannot be read.
    BadObject { key: Vec<u8>, error: TermError },
    /// A vector names no object of a key.
    VectorObject { key: Vec<u8> },
    /// Only a key of a list keyspace splits.
    NotAList { key: Vec<u8> },
}

impl QuadKeyError {
    #[allow(non_snake_case)]
    pub fn Truncated(key: &[u8]) -> Self {
        Self(QuadKeyErrorEnum::Truncated { key: key.to_vec() })
    }

    #[allow(non_snake_case)]
    pub fn TrailingBytes(key: &[u8]) -> Self {
        Self(QuadKeyErrorEnum::TrailingBytes { key: key.to_vec() })
    }

    #[allow(non_snake_case)]
    pub fn UnknownKeyspace(key: &[u8]) -> Self {
        Self(QuadKeyErrorEnum::UnknownKeyspace { key: key.to_vec() })
    }

    #[allow(non_snake_case)]
    pub fn ZeroUid(key: &[u8], field: &'static str) -> Self {
        Self(QuadKeyErrorEnum::ZeroUid {
            key: key.to_vec(),
            field,
        })
    }

    #[allow(non_snake_case)]
    pub fn UnknownDirection(key: &[u8]) -> Self {
        Self(QuadKeyErrorEnum::UnknownDirection { key: key.to_vec() })
    }

    #[allow(non_snake_case)]
    pub fn BadObject(key: &[u8], error: TermError) -> Self {
        Self(QuadKeyErrorEnum::BadObject {
            key: key.to_vec(),
            error,
        })
    }

    #[allow(non_snake_case)]
    pub fn VectorObject(key: &[u8]) -> Self {
        Self(QuadKeyErrorEnum::VectorObject { key: key.to_vec() })
    }

    #[allow(non_snake_case)]
    pub fn NotAList(key: &[u8]) -> Self {
        Self(QuadKeyErrorEnum::NotAList { key: key.to_vec() })
    }
}
