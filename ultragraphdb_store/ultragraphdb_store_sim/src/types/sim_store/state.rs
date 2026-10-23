/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};

use ultragraphdb_traits_store::{Entry, Scan};

/// A stored key: the user key, then the version, newest first, which is the order fjall gives the escaped,
/// terminated and complemented keys of `ultragraphdb_store_kv`.
pub type StoredKey = (Vec<u8>, Reverse<u64>);

/// The flag bit of a version whose older versions compaction may collect.
const DISCARD: u8 = 0x01;
/// The flag bit of a deletion.
const TOMBSTONE: u8 = 0x02;

/// What a simulated store holds: the data keyspace, each version's bytes framed as `[meta][flags][value]`, and the
/// metadata keyspace's entries.
#[derive(Debug, Clone, Default)]
pub struct State {
    pub data: BTreeMap<StoredKey, Vec<u8>>,
    pub applied_ts: u64,
    pub banned: BTreeSet<u64>,
    pub discard_ts: u64,
    pub horizon: Option<u64>,
    pub drop_record: Option<DropRecord>,
}

/// A drop in progress, as its record in the metadata keyspace names it: of everything when `prefixes` is `None`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DropRecord {
    pub ts: u64,
    pub prefixes: Option<Vec<Vec<u8>>>,
    pub entries: Vec<Entry>,
}

impl DropRecord {
    /// Whether the drop removes `key`.
    pub fn covers(&self, key: &[u8]) -> bool {
        self.prefixes
            .as_ref()
            .is_none_or(|prefixes| prefixes.iter().any(|p| key.starts_with(p)))
    }
}

/// A version decoded from its framed bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    pub meta: u8,
    pub discard: bool,
    pub deletion: bool,
    pub value: Vec<u8>,
}

/// The framed bytes of `entry`.
pub fn encode(entry: &Entry) -> Vec<u8> {
    let flags =
        if entry.discard() { DISCARD } else { 0 } | if entry.is_deletion() { TOMBSTONE } else { 0 };
    let mut bytes = vec![entry.meta(), flags];
    bytes.extend_from_slice(entry.value().unwrap_or_default());
    bytes
}

/// The version framed in `bytes`, or why it cannot be decoded.
pub fn decode(bytes: &[u8]) -> Result<Decoded, String> {
    match bytes {
        [meta, flags, value @ ..] if flags & !(DISCARD | TOMBSTONE) == 0 => Ok(Decoded {
            meta: *meta,
            discard: flags & DISCARD != 0,
            deletion: flags & TOMBSTONE != 0,
            value: value.to_vec(),
        }),
        [_, flags, ..] => Err(format!("unknown version flags {flags:#04x}")),
        _ => Err("value too short to hold the version prefix".to_string()),
    }
}

impl State {
    /// The stored keys from the first key `scan` may select to the end of its range, in key order.
    pub fn range<'a>(
        &'a self,
        scan: &Scan,
    ) -> impl Iterator<Item = (&'a StoredKey, &'a Vec<u8>)> + 'a {
        let start = (scan.start().to_vec(), Reverse(u64::MAX));
        let end = scan.end().map(<[u8]>::to_vec);
        self.data
            .range(start..)
            .take_while(move |((key, _), _)| end.as_ref().is_none_or(|end| key < end))
    }

    /// The versions of `key`, newest first.
    pub fn versions<'a>(&'a self, key: &[u8]) -> impl Iterator<Item = (u64, &'a Vec<u8>)> + 'a {
        let owned = key.to_vec();
        self.data
            .range((key.to_vec(), Reverse(u64::MAX))..)
            .take_while(move |((k, _), _)| *k == owned)
            .map(|((_, Reverse(version)), bytes)| (*version, bytes))
    }
}
