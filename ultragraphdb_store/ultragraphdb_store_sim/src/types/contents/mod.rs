/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::{BTreeMap, BTreeSet};

use crate::types::stored_version::StoredVersion;
use crate::utils::hex::hex;

/// Everything a simulated store holds, read without faults: every version of every key, deletions included, and
/// the metadata the store keeps beside them, its rebuild records included. The shadow model builds the same from what it was told, and the two
/// must be equal.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Contents {
    keys: BTreeMap<Vec<u8>, Vec<StoredVersion>>,
    applied_ts: u64,
    banned: BTreeSet<u64>,
    discard_ts: u64,
    tombstone_horizon: Option<u64>,
    rebuild_records: BTreeMap<u64, Vec<Vec<u8>>>,
}

impl Contents {
    pub(crate) fn new(
        keys: BTreeMap<Vec<u8>, Vec<StoredVersion>>,
        applied_ts: u64,
        banned: BTreeSet<u64>,
        discard_ts: u64,
        tombstone_horizon: Option<u64>,
    ) -> Self {
        Self {
            keys,
            applied_ts,
            banned,
            discard_ts,
            tombstone_horizon,
            rebuild_records: BTreeMap::new(),
        }
    }

    /// The contents holding `records`, the rebuild records by number with their prefixes.
    pub(crate) fn with_rebuild_records(mut self, records: BTreeMap<u64, Vec<Vec<u8>>>) -> Self {
        self.rebuild_records = records;
        self
    }

    /// Every key with its versions, newest first.
    pub fn keys(&self) -> &BTreeMap<Vec<u8>, Vec<StoredVersion>> {
        &self.keys
    }

    /// The versions of `key`, newest first; empty when it has none.
    pub fn versions(&self, key: &[u8]) -> &[StoredVersion] {
        self.keys.get(key).map_or(&[], Vec::as_slice)
    }

    pub fn applied_ts(&self) -> u64 {
        self.applied_ts
    }

    pub fn banned(&self) -> &BTreeSet<u64> {
        &self.banned
    }

    /// The discard timestamp asked for, which the store keeps; open snapshots can hold the one in effect lower.
    pub fn discard_ts(&self) -> u64 {
        self.discard_ts
    }

    /// The tombstone horizon set, if one was.
    pub fn tombstone_horizon(&self) -> Option<u64> {
        self.tombstone_horizon
    }

    /// The rebuild records, by number, with their prefixes.
    pub fn rebuild_records(&self) -> &BTreeMap<u64, Vec<Vec<u8>>> {
        &self.rebuild_records
    }

    /// The first difference from `other`, ignoring the keys `skip` holds; `None` when there is none.
    pub(crate) fn difference(&self, other: &Contents, skip: &BTreeSet<Vec<u8>>) -> Option<String> {
        if self.applied_ts != other.applied_ts {
            return Some(format!(
                "applied timestamp {} where {} was expected",
                self.applied_ts, other.applied_ts
            ));
        }
        if (&self.banned, self.discard_ts, self.tombstone_horizon)
            != (&other.banned, other.discard_ts, other.tombstone_horizon)
        {
            return Some(format!(
                "banned {:?}, discard {}, horizon {:?} where {:?}, {}, {:?} were expected",
                self.banned,
                self.discard_ts,
                self.tombstone_horizon,
                other.banned,
                other.discard_ts,
                other.tombstone_horizon
            ));
        }
        if self.rebuild_records != other.rebuild_records {
            return Some(format!(
                "rebuild records {:?} where {:?} were expected",
                self.rebuild_records.keys().collect::<Vec<_>>(),
                other.rebuild_records.keys().collect::<Vec<_>>()
            ));
        }
        let keys: BTreeSet<&Vec<u8>> = self.keys.keys().chain(other.keys.keys()).collect();
        keys.into_iter()
            .filter(|k| !skip.contains(*k))
            .find_map(|key| {
                let (held, expected) = (self.versions(key), other.versions(key));
                (held != expected).then(|| {
                    format!(
                        "key {} holds {:?} where {:?} was expected",
                        hex(key),
                        held,
                        expected
                    )
                })
            })
    }
}
