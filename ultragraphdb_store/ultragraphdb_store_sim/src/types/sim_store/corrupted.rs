/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeMap;

use crate::types::corruption::Corruption;

/// The stored versions whose bytes are corrupted, and how: corruption at rest, which every later read of the
/// version sees, until the version is written again or removed.
#[derive(Debug, Clone, Default)]
pub struct Corrupted {
    versions: BTreeMap<(Vec<u8>, u64), Corruption>,
}

impl Corrupted {
    pub fn insert(&mut self, key: Vec<u8>, version: u64, corruption: Corruption) {
        self.versions.insert((key, version), corruption);
    }

    pub fn remove(&mut self, key: &[u8], version: u64) {
        self.versions.remove(&(key.to_vec(), version));
    }

    /// The bytes of `key` at `version` as a read finds them.
    pub fn bytes(&self, key: &[u8], version: u64, stored: &[u8]) -> Vec<u8> {
        match self.versions.get(&(key.to_vec(), version)) {
            None => stored.to_vec(),
            Some(Corruption::FlipValueByte) if stored.len() > 2 => {
                let mut bytes = stored.to_vec();
                let at = 2 + (bytes.len() - 2) / 2;
                bytes[at] = !bytes[at];
                bytes
            }
            Some(_) => stored[..stored.len().min(1)].to_vec(),
        }
    }
}
