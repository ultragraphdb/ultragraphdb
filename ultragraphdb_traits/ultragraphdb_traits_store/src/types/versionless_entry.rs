/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// One write of a [`crate::WriteBatch`] to the keyspace without versions: a key's one value, or its removal. The
/// keyspace holds what never changes once written, such as the term dictionary, so a read needs no timestamp.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VersionlessEntry {
    key: Vec<u8>,
    value: Option<Vec<u8>>,
}

impl VersionlessEntry {
    /// `value` as the value of `key`, replacing any it had.
    pub fn put(key: Vec<u8>, value: Vec<u8>) -> Self {
        Self {
            key,
            value: Some(value),
        }
    }

    /// The removal of `key`.
    pub fn removal(key: Vec<u8>) -> Self {
        Self { key, value: None }
    }

    pub fn key(&self) -> &[u8] {
        &self.key
    }

    /// The value; `None` for a removal.
    pub fn value(&self) -> Option<&[u8]> {
        self.value.as_deref()
    }

    pub fn is_removal(&self) -> bool {
        self.value.is_none()
    }
}
