/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// One stored version of a key as [`crate::Contents`] lists it: a value with its meta byte and discard flag, a
/// deletion, or bytes the store cannot decode.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StoredVersion {
    version: u64,
    meta: u8,
    discard: bool,
    deletion: bool,
    decodable: bool,
    value: Vec<u8>,
}

impl StoredVersion {
    pub(crate) fn new(
        version: u64,
        meta: u8,
        discard: bool,
        deletion: bool,
        value: Vec<u8>,
    ) -> Self {
        Self {
            version,
            meta,
            discard,
            deletion,
            decodable: true,
            value,
        }
    }

    pub(crate) fn undecodable(version: u64, bytes: Vec<u8>) -> Self {
        Self {
            version,
            meta: 0,
            discard: false,
            deletion: false,
            decodable: false,
            value: bytes,
        }
    }

    /// The version.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// The value's meta byte; 0 for a deletion or undecodable bytes.
    pub fn meta(&self) -> u8 {
        self.meta
    }

    /// Whether the value carries the discard flag.
    pub fn discard(&self) -> bool {
        self.discard
    }

    /// Whether the version is a deletion.
    pub fn is_deletion(&self) -> bool {
        self.deletion
    }

    /// Whether the store can decode the version prefix; the value holds the raw bytes when it cannot.
    pub fn is_decodable(&self) -> bool {
        self.decodable
    }

    /// The value, or the bytes the store cannot decode.
    pub fn value(&self) -> &[u8] {
        &self.value
    }
}
