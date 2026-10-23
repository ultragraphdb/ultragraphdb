/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod gc_filter_compaction_filter;

use std::sync::Arc;

use fjall::Slice;
use fjall::compaction::filter::Verdict;

use crate::types::gc_settings::GcSettings;
use crate::utils::stored_key::escaped_part;
use crate::utils::stored_value::decode_value;

/// The compaction filter of the data keyspace: Badger's rule for versions (`badger/levels.go:815-835`) over the
/// port's stored keys. One is made per compaction, which hands it the compaction's items in stored-key order, so it
/// sees each key's versions newest first, and of a stored key written twice the newer copy first.
///
/// Of each key it collects the versions older than the newest discard-flagged version or deletion at or below the
/// discard timestamp: [`Verdict::Remove`] above the last level, which leaves an lsm-tree removal hiding the item's
/// older copies, and [`Verdict::Destroy`] in it. It never collects a deletion, whose key's older versions can sit in a
/// table outside the compaction and would return, nor a version a drop fenced. It is total: an item it cannot
/// decode is kept and counted, because a filter error or panic ends fjall's compaction worker and poisons the
/// database.
#[derive(Debug)]
pub struct GcFilter {
    discard_ts: u64,
    last_level: bool,
    /// The escaped user key of the key being read.
    key: Vec<u8>,
    /// Whether a version of that key at or below the discard timestamp is discard-flagged or a deletion.
    cut: bool,
    /// The previous item's stored key and whether it was collected: an older copy of the same stored key, which
    /// lsm-tree keeps for an open snapshot, shares its verdict.
    previous: Option<(Vec<u8>, bool)>,
    undecodable: u64,
    /// The stored keys collected, handed to the store when the compaction finishes.
    collected: Vec<Slice>,
    settings: Option<Arc<GcSettings>>,
}

impl GcFilter {
    /// The filter of one compaction under `discard_ts`, into the last level when `last_level`.
    pub fn new(discard_ts: u64, last_level: bool) -> Self {
        Self {
            discard_ts,
            last_level,
            key: Vec::new(),
            cut: false,
            previous: None,
            undecodable: 0,
            collected: Vec::new(),
            settings: None,
        }
    }

    /// The filter of a compaction of the store whose `settings` it reads and counts into.
    pub(crate) fn for_store(settings: Arc<GcSettings>, last_level: bool) -> Self {
        let mut filter = Self::new(settings.discard_ts(), last_level);
        filter.settings = Some(settings);
        filter
    }

    /// The items this filter kept because it could not decode them.
    pub fn undecodable(&self) -> u64 {
        self.undecodable
    }

    /// The stored keys this filter collected, each once.
    pub fn collected(&self) -> &[Slice] {
        &self.collected
    }

    /// What becomes of the item with stored key `stored` and stored value `value`, the next item of the compaction.
    pub fn verdict(&mut self, stored: &[u8], value: &[u8]) -> Verdict {
        if let Some((previous, collected)) = &self.previous
            && previous.as_slice() == stored
        {
            return self.to_verdict(*collected);
        }
        let collected = self.decide(stored, value);
        if collected {
            self.collected.push(Slice::from(stored));
        }
        self.previous = Some((stored.to_vec(), collected));
        self.to_verdict(collected)
    }

    fn decide(&mut self, stored: &[u8], value: &[u8]) -> bool {
        let (Some(escaped), Ok(decoded)) = (escaped_part(stored), decode_value(value)) else {
            self.undecodable += 1;
            return false;
        };
        if escaped != self.key.as_slice() {
            self.key = escaped.to_vec();
            self.cut = false;
        }
        let mut version = [0u8; 8];
        version.copy_from_slice(&stored[stored.len() - 8..]);
        let version = !u64::from_be_bytes(version);
        let collected = self.cut && !decoded.deletion && !decoded.fenced;
        if version <= self.discard_ts && (decoded.discard || decoded.deletion) {
            self.cut = true;
        }
        collected
    }

    fn to_verdict(&self, collected: bool) -> Verdict {
        match (collected, self.last_level) {
            (false, _) => Verdict::Keep,
            (true, false) => Verdict::Remove,
            (true, true) => Verdict::Destroy,
        }
    }
}
