/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod slot;

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use ultragraphdb_cluster_partition::BoundKey;

use self::slot::Slot;
use crate::types::scan_key::ScanKey;

/// The Scans a core answers: those with a frame to build, in turn, and those waiting on a time, on an epoch's close or on their
/// transaction, each found by what it waits on, so a turn costs what is ready and not what is open. Every turn names an open Scan:
/// one that ends takes its turn and its waits with it, so a Scan's end leaves nothing that keeps the core awake or serves a later
/// Scan of the same name.
#[derive(Debug)]
pub struct ScanBoard<T> {
    open: BTreeMap<ScanKey, Slot<T>>,
    turns: VecDeque<ScanKey>,
    timers: BTreeSet<(u64, ScanKey)>,
    epochs: BTreeSet<(u64, ScanKey)>,
    /// The Scans each transaction opened. One ended stays until its transaction does, whose limit bounds it and whose read set holds
    /// more of it than this.
    transactions: BTreeSet<(BoundKey, ScanKey)>,
}

impl<T> Default for ScanBoard<T> {
    fn default() -> Self {
        Self {
            open: BTreeMap::new(),
            turns: VecDeque::new(),
            timers: BTreeSet::new(),
            epochs: BTreeSet::new(),
            transactions: BTreeSet::new(),
        }
    }
}

impl<T> ScanBoard<T> {
    /// Holds `scan` as `key`, with no turn yet and waiting on nothing.
    pub fn open(&mut self, key: ScanKey, scan: T) {
        let slot = Slot {
            scan,
            timer: None,
            epoch: None,
        };
        if let Some(old) = self.open.insert(key, slot) {
            self.forget_waits(key, &old);
        }
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
        let slot = self.open.remove(&key)?;
        self.forget_waits(key, &slot);
        Some((key, slot.scan))
    }

    /// How many Scans wait for their turn.
    pub fn turns_waiting(&self) -> usize {
        self.turns.len()
    }

    /// Ends the Scan `key`, its turn and its waits with it: the Scan, when it was open.
    pub fn remove(&mut self, key: ScanKey) -> Option<T> {
        let slot = self.open.remove(&key)?;
        self.forget_waits(key, &slot);
        self.turns.retain(|turn| *turn != key);
        Some(slot.scan)
    }

    /// Ends every Scan of `connection`, their turns and waits with them.
    pub fn close(&mut self, connection: u64) {
        let of_connection: Vec<ScanKey> = self
            .open
            .range((connection, 0)..=(connection, u64::MAX))
            .map(|(key, _)| *key)
            .collect();
        for key in of_connection {
            if let Some(slot) = self.open.remove(&key) {
                self.forget_waits(key, &slot);
            }
        }
        self.turns.retain(|(of, _)| *of != connection);
    }

    /// The Scan `key`, when it is open.
    pub fn get(&self, key: ScanKey) -> Option<&T> {
        self.open.get(&key).map(|slot| &slot.scan)
    }

    /// The Scan `key`, when it is open.
    pub fn get_mut(&mut self, key: ScanKey) -> Option<&mut T> {
        self.open.get_mut(&key).map(|slot| &mut slot.scan)
    }

    /// Whether no Scan is open.
    pub fn is_empty(&self) -> bool {
        self.open.is_empty()
    }

    /// Whether a Scan waits for its turn.
    pub fn has_turns(&self) -> bool {
        !self.turns.is_empty()
    }

    /// The Scan `key` is looked at once `at_us` has passed, in place of any time set before.
    pub fn set_timer(&mut self, key: ScanKey, at_us: u64) {
        if let Some(slot) = self.open.get_mut(&key) {
            if let Some(before) = slot.timer.replace(at_us) {
                let _was = self.timers.remove(&(before, key));
            }
            let _new = self.timers.insert((at_us, key));
        }
    }

    /// The Scan `key` no longer waits on a time.
    pub fn clear_timer(&mut self, key: ScanKey) {
        if let Some(before) = self.open.get_mut(&key).and_then(|slot| slot.timer.take()) {
            let _was = self.timers.remove(&(before, key));
        }
    }

    /// A Scan whose time has passed at `now_us`, its time spent.
    pub fn next_due(&mut self, now_us: u64) -> Option<ScanKey> {
        let (at_us, key) = *self.timers.first()?;
        if at_us > now_us {
            return None;
        }
        let _taken = self.timers.pop_first();
        if let Some(slot) = self.open.get_mut(&key) {
            slot.timer = None;
        }
        Some(key)
    }

    /// The Scan `key` waits for the close of `epoch`.
    pub fn wait_for_epoch(&mut self, key: ScanKey, epoch: u64) {
        if let Some(slot) = self.open.get_mut(&key) {
            if let Some(before) = slot.epoch.replace(epoch) {
                let _was = self.epochs.remove(&(before, key));
            }
            let _new = self.epochs.insert((epoch, key));
        }
    }

    /// A Scan whose epoch is at or below `closed`, its wait over.
    pub fn next_closed(&mut self, closed: u64) -> Option<ScanKey> {
        let (epoch, key) = *self.epochs.first()?;
        if epoch > closed {
            return None;
        }
        let _taken = self.epochs.pop_first();
        if let Some(slot) = self.open.get_mut(&key) {
            slot.epoch = None;
        }
        Some(key)
    }

    /// The Scan `key` belongs to the transaction `transaction`.
    pub fn join_transaction(&mut self, transaction: BoundKey, key: ScanKey) {
        let _new = self.transactions.insert((transaction, key));
    }

    /// The open Scans of `transaction`.
    pub fn of_transaction(&self, transaction: BoundKey) -> Vec<ScanKey> {
        self.named_by(transaction)
            .filter(|key| self.open.contains_key(key))
            .collect()
    }

    /// `transaction` ended: its open Scans, which the board no longer finds by it.
    pub fn end_transaction(&mut self, transaction: BoundKey) -> Vec<ScanKey> {
        let named: Vec<ScanKey> = self.named_by(transaction).collect();
        for key in &named {
            let _was = self.transactions.remove(&(transaction, *key));
        }
        named
            .into_iter()
            .filter(|key| self.open.contains_key(key))
            .collect()
    }

    fn named_by(&self, transaction: BoundKey) -> impl Iterator<Item = ScanKey> + '_ {
        self.transactions
            .range((transaction, (0, 0))..=(transaction, (u64::MAX, u64::MAX)))
            .map(|(_, key)| *key)
    }

    /// Lets go of what `slot`, no longer open as `key`, waited on.
    fn forget_waits(&mut self, key: ScanKey, slot: &Slot<T>) {
        if let Some(at_us) = slot.timer {
            let _was = self.timers.remove(&(at_us, key));
        }
        if let Some(epoch) = slot.epoch {
            let _was = self.epochs.remove(&(epoch, key));
        }
    }
}
