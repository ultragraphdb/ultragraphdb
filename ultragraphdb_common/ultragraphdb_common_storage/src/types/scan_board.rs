/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::{BTreeMap, VecDeque};

use crate::types::scan_key::ScanKey;

/// The Scans a core answers and those with a frame to build, in turn. Every turn names an open Scan: one that ends takes its
/// turn with it, so a Scan's end leaves nothing that keeps the core awake or serves a later Scan of the same name.
#[derive(Debug)]
pub struct ScanBoard<T> {
    open: BTreeMap<ScanKey, T>,
    turns: VecDeque<ScanKey>,
}

impl<T> Default for ScanBoard<T> {
    fn default() -> Self {
        Self {
            open: BTreeMap::new(),
            turns: VecDeque::new(),
        }
    }
}

impl<T> ScanBoard<T> {
    /// Holds `scan` as `key`, with no turn yet.
    pub fn open(&mut self, key: ScanKey, scan: T) {
        let _none = self.open.insert(key, scan);
    }

    /// Gives the Scan `key` a turn behind those waiting for one.
    pub fn give_turn(&mut self, key: ScanKey) {
        if self.open.contains_key(&key) {
            self.turns.push_back(key);
        }
    }

    /// The Scan whose turn it is, taken out while its frame is built.
    pub fn take_turn(&mut self) -> Option<(ScanKey, T)> {
        let key = self.turns.pop_front()?;
        self.open.remove(&key).map(|scan| (key, scan))
    }

    /// Ends the Scan `key`, its turn with it: the Scan, when it was open.
    pub fn remove(&mut self, key: ScanKey) -> Option<T> {
        let scan = self.open.remove(&key)?;
        self.turns.retain(|turn| *turn != key);
        Some(scan)
    }

    /// Ends every Scan of `connection`, their turns with them.
    pub fn close(&mut self, connection: u64) {
        self.open.retain(|(of, _), _| *of != connection);
        self.turns.retain(|(of, _)| *of != connection);
    }

    /// The Scan `key`, when it is open.
    pub fn get(&self, key: ScanKey) -> Option<&T> {
        self.open.get(&key)
    }

    /// The Scan `key`, when it is open.
    pub fn get_mut(&mut self, key: ScanKey) -> Option<&mut T> {
        self.open.get_mut(&key)
    }

    /// Every open Scan with its key.
    pub fn iter(&self) -> impl Iterator<Item = (&ScanKey, &T)> {
        self.open.iter()
    }

    /// Whether no Scan is open.
    pub fn is_empty(&self) -> bool {
        self.open.is_empty()
    }

    /// Whether a Scan waits for its turn.
    pub fn has_turns(&self) -> bool {
        !self.turns.is_empty()
    }
}
