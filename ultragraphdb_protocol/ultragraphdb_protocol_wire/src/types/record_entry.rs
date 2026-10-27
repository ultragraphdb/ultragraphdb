/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// An entry of a key-value section: a key and its value, `None` for a removal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RecordEntry<'a> {
    key: &'a [u8],
    value: Option<&'a [u8]>,
}

impl<'a> RecordEntry<'a> {
    pub fn new(key: &'a [u8], value: Option<&'a [u8]>) -> Self {
        Self { key, value }
    }

    pub fn key(&self) -> &'a [u8] {
        self.key
    }

    /// The value, `None` when the entry removes its key.
    pub fn value(&self) -> Option<&'a [u8]> {
        self.value
    }
}
