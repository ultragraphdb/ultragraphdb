/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod wal_entry;
mod wal_open;
mod wal_ordered_log;
mod wal_recovery;
mod wal_stop;
mod wal_write;

use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;

use ultragraphdb_traits_log::Position;

use crate::traits::durable_storage::DurableStorage;
use crate::types::aligned_buf::AlignedBuf;
use crate::types::superblock::Superblock;
use crate::types::wal::wal_entry::WalEntry;
use crate::types::wal_config::WalConfig;

/// The write-ahead log, the local [`ultragraphdb_traits_log::OrderedLog`]: a directory of preallocated segment files
/// of a fixed size, `{index:020}.segment`, holding rounds as Round frames at 4,096-byte boundaries, each padded to
/// whole blocks and never spanning two segments, and the two superblock copies. A round is written with one write and
/// synced with one `fdatasync`. The commit step owns the log and the log owns its files, so every method that
/// changes it takes `&mut self`.
pub struct Wal<S: DurableStorage> {
    storage: S,
    dir: PathBuf,
    config: WalConfig,
    /// The newest superblock record, written or read.
    superblock: Superblock,
    /// Every segment kept, open.
    files: BTreeMap<u64, S::File>,
    /// The durable rounds kept, in position order.
    index: VecDeque<WalEntry>,
    /// Rounds appended and not yet synced, with their padded frames.
    staged: VecDeque<(WalEntry, AlignedBuf)>,
    /// The last durable round's position and checksum, or the checkpoint's.
    committed: (Position, u64),
    /// The last appended round's position and checksum, or the committed one's.
    tail: (Position, u64),
    /// The segment and offset the next round is placed at, unless it does not fit there.
    cursor: (u64, u64),
    sealed: bool,
    poisoned: bool,
}

impl<S: DurableStorage> Wal<S> {
    pub fn config(&self) -> &WalConfig {
        &self.config
    }

    /// The log's incarnation, from its superblock.
    pub fn incarnation(&self) -> u64 {
        self.superblock.incarnation()
    }

    /// The last checkpoint's position, from the superblock.
    pub fn checkpoint(&self) -> Position {
        self.superblock.checkpoint()
    }

    /// The file of segment `index`.
    fn segment_path(&self, index: u64) -> PathBuf {
        self.dir.join(segment_name(index))
    }
}

/// The file name of segment `index`.
fn segment_name(index: u64) -> String {
    format!("{index:020}.segment")
}

/// The index of a segment's file name, `None` for any other name.
fn segment_index(name: &str) -> Option<u64> {
    let digits = name.strip_suffix(".segment")?;
    if digits.len() == 20 && digits.bytes().all(|byte| byte.is_ascii_digit()) {
        digits.parse().ok()
    } else {
        None
    }
}
