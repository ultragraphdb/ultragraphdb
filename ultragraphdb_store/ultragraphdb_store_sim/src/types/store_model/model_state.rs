/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::{BTreeMap, BTreeSet};

use ultragraphdb_traits_store::{Direction, Entry, Item, Scan, key_namespace};

use crate::types::contents::Contents;
use crate::types::stored_version::StoredVersion;

/// One version as the model keeps it: `value` is `None` for a deletion; `fenced` when a prefix drop wrote it, which
/// compaction never collects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelVersion {
    pub meta: u8,
    pub discard: bool,
    pub fenced: bool,
    pub value: Option<Vec<u8>>,
}

/// What the store should hold, by the rules of the store's specification.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModelState {
    pub keys: BTreeMap<Vec<u8>, BTreeMap<u64, ModelVersion>>,
    pub applied_ts: u64,
    pub banned: BTreeSet<u64>,
    pub discard_ts: u64,
    pub horizon: Option<u64>,
}

/// A write the model replays when it rebuilds what survived a crash or a power loss.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelOp {
    Batch {
        entries: Vec<Entry>,
        applied_ts: Option<u64>,
    },
    Drop {
        prefixes: Option<Vec<Vec<u8>>>,
        entries: Vec<Entry>,
    },
}

/// Whether a drop of `prefixes`, of everything when `None`, removes `key`.
pub fn covers(prefixes: &Option<Vec<Vec<u8>>>, key: &[u8]) -> bool {
    prefixes
        .as_ref()
        .is_none_or(|ps| ps.iter().any(|p| key.starts_with(p)))
}

impl ModelState {
    fn is_banned(&self, key: &[u8]) -> bool {
        key_namespace(key).is_some_and(|ns| self.banned.contains(&ns))
    }

    /// `entries` without those addressed to a banned namespace.
    pub fn admitted(&self, entries: &[Entry]) -> Vec<Entry> {
        entries
            .iter()
            .filter(|e| !self.is_banned(e.key()))
            .cloned()
            .collect()
    }

    pub fn apply(&mut self, op: &ModelOp) {
        match op {
            ModelOp::Batch {
                entries,
                applied_ts,
            } => {
                self.write(entries, false);
                if let Some(ts) = applied_ts {
                    self.applied_ts = self.applied_ts.max(*ts);
                }
            }
            ModelOp::Drop { prefixes, entries } => {
                self.keys.retain(|key, _| !covers(prefixes, key));
                if prefixes.is_none() {
                    self.applied_ts = 0;
                    self.banned.clear();
                    self.discard_ts = 0;
                    self.horizon = None;
                }
                let admitted = self.admitted(entries);
                self.write(&admitted, prefixes.is_some());
            }
        }
    }

    fn write(&mut self, entries: &[Entry], fenced: bool) {
        for e in entries {
            let version = ModelVersion {
                meta: e.meta(),
                discard: e.discard(),
                fenced,
                value: e.value().map(<[u8]>::to_vec),
            };
            self.keys
                .entry(e.key().to_vec())
                .or_default()
                .insert(e.version(), version);
        }
    }

    /// Compaction under discard timestamp `discard`: each key loses the values older than its newest
    /// discard-flagged value or deletion at or below `discard`; deletions and the values a prefix drop fenced stay.
    pub fn compact(&mut self, discard: u64) {
        for versions in self.keys.values_mut() {
            let cut = versions
                .iter()
                .rev()
                .find(|(v, m)| **v <= discard && (m.discard || m.value.is_none()))
                .map(|(v, _)| *v);
            if let Some(cut) = cut {
                versions.retain(|v, m| *v >= cut || m.value.is_none() || m.fenced);
            }
        }
    }

    /// The sweep under tombstone horizon `horizon`: a deletion at or below it with no version below it goes.
    pub fn sweep(&mut self, horizon: u64) {
        for versions in self.keys.values_mut() {
            if let Some((oldest, m)) = versions.iter().next()
                && *oldest <= horizon
                && m.value.is_none()
            {
                let oldest = *oldest;
                versions.remove(&oldest);
            }
        }
        self.keys.retain(|_, versions| !versions.is_empty());
    }

    fn item(key: &[u8], version: u64, m: &ModelVersion) -> Option<Item> {
        m.value
            .as_ref()
            .map(|value| Item::new(key.to_vec(), version, m.meta, m.discard, value.clone()))
    }

    /// The newest version of `key` at or below `ts`, as a read must return it.
    pub fn get(&self, key: &[u8], ts: u64) -> Option<Item> {
        if self.is_banned(key) {
            return None;
        }
        let (version, m) = self.keys.get(key)?.range(..=ts).next_back()?;
        Self::item(key, *version, m)
    }

    /// The items `scan` must report at `ts`.
    pub fn scan(&self, scan: &Scan, ts: u64) -> Vec<Item> {
        let mut per_key: Vec<Vec<Item>> = self
            .keys
            .iter()
            .filter(|(key, _)| scan.selects(key) && !self.is_banned(key))
            .map(|(key, versions)| {
                let visible = versions
                    .range(..=ts)
                    .rev()
                    .map_while(|(v, m)| Self::item(key, *v, m));
                if scan.is_all_versions() {
                    visible.collect()
                } else {
                    visible.take(1).collect()
                }
            })
            .collect();
        if scan.direction() == Direction::Reverse {
            per_key.reverse();
        }
        per_key.into_iter().flatten().collect()
    }

    pub fn contents(&self) -> Contents {
        let keys = self
            .keys
            .iter()
            .map(|(key, versions)| {
                let stored = versions
                    .iter()
                    .rev()
                    .map(|(v, m)| {
                        StoredVersion::new(
                            *v,
                            m.meta,
                            m.discard,
                            m.value.is_none(),
                            m.value.clone().unwrap_or_default(),
                        )
                    })
                    .collect();
                (key.clone(), stored)
            })
            .collect();
        Contents::new(
            keys,
            self.applied_ts,
            self.banned.clone(),
            self.discard_ts,
            self.horizon,
        )
    }
}
