/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// One version of a key as a read returns it, Badger's `Item`: the key, the version, the meta byte, the discard
/// flag and the value. A read never returns a deletion.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Item {
    key: Vec<u8>,
    version: u64,
    meta: u8,
    discard: bool,
    value: Vec<u8>,
}

impl Item {
    pub fn new(key: Vec<u8>, version: u64, meta: u8, discard: bool, value: Vec<u8>) -> Self {
        Self {
            key,
            version,
            meta,
            discard,
            value,
        }
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

    pub fn value(&self) -> &[u8] {
        &self.value
    }

    /// The value, taken out of the item.
    pub fn into_value(self) -> Vec<u8> {
        self.value
    }
}
