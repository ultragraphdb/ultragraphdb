/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// One write of a [`crate::WriteBatch`]: a version of a key, with a value or as a deletion.
///
/// A value carries Badger's per-version metadata: the meta byte (Dgraph writes schema `0x01`, delta `0x04`,
/// complete `0x08`, empty `0x10`) and the discard flag, Badger's `bitDiscardEarlierVersions`, which lets compaction
/// collect the key's older versions once the discard timestamp passes it. A deletion hides the key's older versions
/// for reads at and above its version.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Entry {
    key: Vec<u8>,
    version: u64,
    meta: u8,
    discard: bool,
    value: Option<Vec<u8>>,
}

impl Entry {
    /// `value` as version `version` of `key`, with meta byte `meta`.
    pub fn put(key: Vec<u8>, version: u64, meta: u8, value: Vec<u8>) -> Self {
        Self {
            key,
            version,
            meta,
            discard: false,
            value: Some(value),
        }
    }

    /// A deletion of `key` at version `version`.
    pub fn deletion(key: Vec<u8>, version: u64) -> Self {
        Self {
            key,
            version,
            meta: 0,
            discard: false,
            value: None,
        }
    }

    /// The entry with the discard flag set.
    pub fn with_discard(mut self) -> Self {
        self.discard = true;
        self
    }

    pub fn key(&self) -> &[u8] {
        &self.key
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn meta(&self) -> u8 {
        self.meta
    }

    /// Whether compaction may collect the key's versions older than this one.
    pub fn discard(&self) -> bool {
        self.discard
    }

    /// The value; `None` for a deletion.
    pub fn value(&self) -> Option<&[u8]> {
        self.value.as_deref()
    }

    pub fn is_deletion(&self) -> bool {
        self.value.is_none()
    }
}
