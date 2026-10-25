/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeMap;

use crate::types::corruption::Corruption;

/// A corruption of a version's bytes, and the heal that wrote the version again or removed it, if one has.
type Healed = (Corruption, Option<u64>);

/// The stored versions whose bytes are corrupted, and how: corruption at rest, which every later read of the
/// version sees until the version is written again or removed. That write or removal heals the version for the
/// reads of what the store holds after it; a snapshot opened before it still reads the bytes it opened on, as a
/// fjall snapshot does, corrupted as they were.
#[derive(Debug, Clone, Default)]
pub struct Corrupted {
    /// Each version's corruptions, oldest first: only the newest can be unhealed.
    versions: BTreeMap<(Vec<u8>, u64), Vec<Healed>>,
    /// How many heals there have been: a snapshot opened after `heals` of them reads every corruption healed after
    /// those, or not healed at all.
    heals: u64,
}

impl Corrupted {
    pub fn insert(&mut self, key: Vec<u8>, version: u64, corruption: Corruption) {
        let corruptions = self.versions.entry((key, version)).or_default();
        match corruptions.last_mut() {
            Some((kind, None)) => *kind = corruption,
            _ => corruptions.push((corruption, None)),
        }
    }

    /// Heals `key` at `version`, written again or removed.
    pub fn remove(&mut self, key: &[u8], version: u64) {
        if let Some((_, healed @ None)) = self
            .versions
            .get_mut(&(key.to_vec(), version))
            .and_then(|corruptions| corruptions.last_mut())
        {
            self.heals += 1;
            *healed = Some(self.heals);
        }
    }

    /// Heals every version at once, as a drop of everything removes them.
    pub fn remove_all(&mut self) {
        self.heals += 1;
        let heal = self.heals;
        self.versions
            .values_mut()
            .filter_map(|corruptions| corruptions.last_mut())
            .for_each(|(_, healed)| {
                healed.get_or_insert(heal);
            });
    }

    /// How many heals there have been, which a snapshot opened now reads past.
    pub fn heals(&self) -> u64 {
        self.heals
    }

    /// The bytes of `key` at `version` as a read of what the store holds finds them.
    pub fn bytes(&self, key: &[u8], version: u64, stored: &[u8]) -> Vec<u8> {
        self.bytes_since(self.heals, key, version, stored)
    }

    /// The bytes of `key` at `version` as a snapshot opened after `heals` heals finds them: corrupted as they were
    /// when it opened, unless healed before, or as corrupted since.
    pub fn bytes_since(&self, heals: u64, key: &[u8], version: u64, stored: &[u8]) -> Vec<u8> {
        let corruption = self
            .versions
            .get(&(key.to_vec(), version))
            .and_then(|corruptions| {
                corruptions
                    .iter()
                    .find(|(_, healed)| healed.is_none_or(|heal| heal > heals))
            })
            .map(|(corruption, _)| *corruption);
        match corruption {
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
