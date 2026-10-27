/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, posting/list.go (rollupOutput, shouldSplit, updateSplits, recursiveSplit,
 * splitUpList, binSplit, removeEmptySplits, splits, marshalPostingListPart, MarshalPostingList). Changed for
 * ultragraphdb: translated to Rust; the split threshold is a parameter, where Dgraph's tests change a package
 * variable; a rollup's parts are written at its main key's version, where Dgraph writes them one version below
 * (`list.go:1455-1458`, `1539`), and the parts it removes are deleted at that version, where Dgraph leaves them;
 * Dgraph's debug-build checks (`VerifyPack`, `VerifyPostingSplits`), no-ops in a release build, are not ported.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::collections::BTreeMap;

use crate::utils::list_keys::split_key_of;
use ultragraphdb_core_codec::{PostingList, UidPack};
use ultragraphdb_traits_store::Entry;

use crate::errors::posting_error::PostingError;
use crate::utils::constants::{BIT_COMPLETE_POSTING, BIT_EMPTY_POSTING};
use crate::utils::posting_codec::marshal;
use crate::utils::postings::is_plist_empty;

/// What a rollup makes of a list: the complete list, or for a split list its splits, the parts by start uid, and
/// the timestamp of the newest commit it holds. Dgraph's `rollupOutput`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RollupOutput {
    plist: PostingList,
    parts: BTreeMap<u64, PostingList>,
    new_min_ts: u64,
}

impl RollupOutput {
    pub(crate) fn new(plist: PostingList, new_min_ts: u64) -> Self {
        Self {
            plist,
            parts: BTreeMap::new(),
            new_min_ts,
        }
    }

    /// The complete list, or the splits of a split list.
    pub fn plist(&self) -> &PostingList {
        &self.plist
    }

    /// The parts of a split list by start uid.
    pub fn parts(&self) -> &BTreeMap<u64, PostingList> {
        &self.parts
    }

    /// The largest of the list's complete version, its delete-all and its commits that the rollup read.
    pub fn new_min_ts(&self) -> u64 {
        self.new_min_ts
    }

    pub(crate) fn plist_mut(&mut self) -> &mut PostingList {
        &mut self.plist
    }

    pub(crate) fn set_new_min_ts(&mut self, ts: u64) {
        self.new_min_ts = ts;
    }

    pub(crate) fn insert_part(&mut self, start_uid: u64, part: PostingList) {
        self.parts.insert(start_uid, part);
    }

    /// The start uids of the parts, ascending: the parts are the record of the splits.
    pub fn splits(&self) -> Vec<u64> {
        self.parts.keys().copied().collect()
    }

    /// Splits the list, and then its parts, while one of them is at least `max_list_size` bytes and spans more
    /// than one block: Dgraph's `recursiveSplit`.
    pub(crate) fn recursive_split(&mut self, max_list_size: usize) {
        self.split_up_list(max_list_size);
        while self.parts.values().any(|p| should_split(p, max_list_size)) {
            self.split_up_list(max_list_size);
        }
    }

    /// Splits each list that is too large in two: Dgraph's `splitUpList`.
    fn split_up_list(&mut self, max_list_size: usize) {
        let lists: Vec<(u64, PostingList)> = if self.parts.is_empty() {
            vec![(1, self.plist.clone())]
        } else {
            self.parts
                .iter()
                .map(|(start, part)| (*start, part.clone()))
                .collect()
        };
        for (start_uid, list) in lists {
            if should_split(&list, max_list_size) {
                let (starts, halves) = bin_split(start_uid, &list);
                starts.into_iter().zip(halves).for_each(|(start, half)| {
                    self.parts.insert(start, half);
                });
            }
        }
        self.update_splits();
    }

    /// Records the parts' start uids as the list's splits: Dgraph's `updateSplits`.
    fn update_splits(&mut self) {
        if !self.parts.is_empty() {
            self.plist = PostingList::default();
        }
        self.plist.set_splits(self.splits());
    }

    /// Removes every empty part but the first, and the first when it is the last left, which unsplits the list:
    /// Dgraph's `removeEmptySplits`.
    pub(crate) fn remove_empty_splits(&mut self) {
        self.parts
            .retain(|&start, part| start == 1 || !is_plist_empty(part));
        self.update_splits();
        if self.parts.len() == 1 && self.parts.get(&1).is_some_and(is_plist_empty) {
            self.parts.clear();
            self.plist.set_splits(Vec::new());
        }
    }

    /// The writes of the rollup of `key` at `version`: the main key, each part, and a deletion of every part of
    /// `old_splits` the rollup no longer has, every one at `version`, complete and empty versions with the discard
    /// flag.
    pub fn entries(
        &self,
        key: &[u8],
        old_splits: &[u64],
        version: u64,
    ) -> Result<Vec<Entry>, PostingError> {
        let mut entries = vec![posting_entry(key.to_vec(), &self.plist, version)];
        for (start, part) in &self.parts {
            entries.push(posting_entry(split_key_of(key, *start)?, part, version));
        }
        for start in old_splits.iter().filter(|s| !self.parts.contains_key(s)) {
            entries.push(Entry::deletion(split_key_of(key, *start)?, version));
        }
        Ok(entries)
    }
}

/// A complete or empty version of a posting list with the discard flag: Dgraph's `MarshalPostingList`, written as
/// `TxnWriter.SetAt` writes those kinds.
fn posting_entry(key: Vec<u8>, plist: &PostingList, version: u64) -> Entry {
    if is_plist_empty(plist) {
        Entry::put(key, version, BIT_EMPTY_POSTING, Vec::new()).with_discard()
    } else {
        Entry::put(key, version, BIT_COMPLETE_POSTING, marshal(plist)).with_discard()
    }
}

/// Whether `plist` should be split in two: at least `max_list_size` bytes encoded, over more than one block.
pub fn should_split(plist: &PostingList, max_list_size: usize) -> bool {
    plist.encoded_len() >= max_list_size && plist.pack().is_some_and(|p| p.blocks().len() > 1)
}

/// Splits `plist`, starting at `low_uid`, at its middle block's base: the start uids of the two halves and the
/// halves, each with its blocks and postings. Dgraph's `binSplit`.
pub fn bin_split(low_uid: u64, plist: &PostingList) -> ([u64; 2], [PostingList; 2]) {
    let pack = plist.pack().cloned().unwrap_or_default();
    let mid_block = pack.blocks().len() / 2;
    let mid_uid = pack.blocks().get(mid_block).map_or(u64::MAX, |b| b.base());
    let half = |blocks: &[_]| UidPack::new(pack.block_size(), blocks.to_vec());
    let pidx = plist.postings().partition_point(|p| p.uid() < mid_uid);
    let low = PostingList::default()
        .with_pack(half(&pack.blocks()[..mid_block]))
        .with_postings(plist.postings()[..pidx].to_vec());
    let high = PostingList::default()
        .with_pack(half(&pack.blocks()[mid_block..]))
        .with_postings(plist.postings()[pidx..].to_vec());
    ([low_uid, mid_uid], [low, high])
}
