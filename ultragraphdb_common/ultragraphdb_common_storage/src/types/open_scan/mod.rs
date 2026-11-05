/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroUsize;
use std::sync::Arc;

use oxrdf::NamedNode;
use ultragraphdb_protocol_wire::ScanAt;
use ultragraphdb_store_kv::{FjallStore, MokaListCache};
use ultragraphdb_store_posting::{CachedList, List, PostingIdentity, ReadPin};
use ultragraphdb_store_quads::KernelScan;

use crate::types::core_message::Pushes;
use crate::types::scan_bound::ScanBound;
use crate::types::scan_read::ScanRead;
use crate::types::scan_state::ScanState;

/// The most keys holding nothing a page of a Scan reads before it hands the turn back (`PROTOCOL.md` §13.14).
const IDLE_KEYS: NonZeroUsize = match NonZeroUsize::new(256) {
    Some(keys) => keys,
    None => NonZeroUsize::MIN,
};

/// A Scan the core is answering (`PROTOCOL.md` §13.13): what it asked, the one version it reads, and where it stands, so one
/// frame is built at a time, each once the one before has left.
pub(crate) struct OpenScan<'a> {
    pub pushes: Pushes,
    pub max_frame: usize,
    pub at: ScanAt,
    pub read_version: u64,
    pub epoch: u64,
    pub limit: Option<NonZeroUsize>,
    pub bound: Option<ScanBound>,
    /// The IRIs its first ranges name.
    pub terms: Vec<NamedNode>,
    pub ranges: Vec<ScanRead>,
    pub state: ScanState,
    /// The version chosen when any wait ended, and the epoch each frame names.
    pub version: u64,
    pub read_epoch: u64,
    /// What keeps the version readable, for a Scan no transaction holds it for.
    pub pin: Option<ReadPin<'a, FjallStore, MokaListCache<CachedList>>>,
    /// The range under way, the key it goes on from, the posting of that key to go on after with the list it read that key
    /// from, and the rows it gave.
    pub range: usize,
    pub from: Vec<u8>,
    pub after: Option<PostingIdentity>,
    pub held: Option<Arc<List>>,
    pub rows: usize,
}

impl OpenScan<'_> {
    /// The request of the range under way's next page, at the Scan's version.
    pub(crate) fn kernel_scan(&self, read: &ScanRead) -> KernelScan {
        let left = self
            .limit
            .map_or(usize::MAX, |limit| limit.get().saturating_sub(self.rows));
        let scan = KernelScan::new(
            read.keyspace,
            read.tenant,
            self.from.clone(),
            self.version,
            NonZeroUsize::new(left).unwrap_or(NonZeroUsize::MIN),
        )
        .resuming(self.after.clone())
        .holding(self.held.clone())
        .yielding(IDLE_KEYS);
        match &read.past {
            Some(past) => scan.to(past.clone()),
            None => scan,
        }
    }

    /// Moves on to the next range, which starts afresh.
    pub(crate) fn next_range(&mut self) {
        self.range += 1;
        self.after = None;
        self.held = None;
        self.rows = 0;
        self.from = self
            .ranges
            .get(self.range)
            .map_or_else(Vec::new, |read| read.first.clone());
    }

    /// Whether every range has answered.
    pub(crate) fn is_done(&self) -> bool {
        self.range >= self.ranges.len()
    }
}
