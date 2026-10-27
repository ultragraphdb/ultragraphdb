/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

pub(crate) mod sim_inode;
pub(crate) mod sim_state;
mod sim_storage_durable_storage;
pub(crate) mod sim_tear;

use std::cell::RefCell;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::types::fault_mode::FaultMode;
use crate::types::sim_storage::sim_state::{SimState, error};
use crate::types::storage_operation::StorageOperation;

/// A deterministic filesystem in memory, Apache Iggy's simulated storage: each file holds buffered and stable
/// contents and each directory buffered and stable entries. A write, create, rename or removal changes the buffered
/// view; a file sync makes a file's contents stable and a directory sync its entries; [`SimStorage::power_loss`]
/// returns every file and directory to its stable view. Faults are injected into one operation by its index in the
/// trace ([`SimStorage::fail_at`]) or at random from the seed ([`SimStorage::fail_randomly`]), and a torn write keeps
/// a prefix whose length is drawn from the seed, so a seed replays.
///
/// Clones share one filesystem, so a test keeps a clone to inject faults and cut the power while the log owns
/// another.
#[derive(Debug, Clone)]
pub struct SimStorage {
    state: Rc<RefCell<SimState>>,
}

impl SimStorage {
    /// An empty filesystem holding only its root directory, `/`, with the fault and tear draws seeded by `seed`.
    pub fn new(seed: u64) -> Self {
        Self {
            state: Rc::new(RefCell::new(SimState::new(seed))),
        }
    }

    /// Runs `f` on the filesystem. The state is borrowed only inside one call of this crate, never across one, so the
    /// borrow is free; were it not, `f` is not run and `None` is returned.
    pub(crate) fn with<T>(&self, f: impl FnOnce(&mut SimState) -> T) -> Option<T> {
        self.state
            .try_borrow_mut()
            .ok()
            .map(|mut state| f(&mut state))
    }

    /// [`SimStorage::with`] for an operation returning an I/O result.
    pub(crate) fn with_io<T>(
        &self,
        f: impl FnOnce(&mut SimState) -> io::Result<T>,
    ) -> io::Result<T> {
        self.with(f).unwrap_or_else(|| {
            Err(error(
                io::ErrorKind::WouldBlock,
                "the simulated storage is in use",
            ))
        })
    }

    pub(crate) fn state(&self) -> Rc<RefCell<SimState>> {
        Rc::clone(&self.state)
    }

    /// Injects `mode` into the operation at index `operation` of the trace, counted from the first operation. A fault
    /// set for an index already passed never fires.
    pub fn fail_at(&self, operation: usize, mode: FaultMode) {
        self.with(|state| state.fail_at(operation, mode));
    }

    /// Faults every later operation with probability one in `one_in`, its mode drawn at equal odds from a fault
    /// before, a fault after, a torn write reported with an error (whole or short at equal odds) and a silent one;
    /// `0` stops random faults. A silent torn write drawn for an operation that is not a write is no fault.
    pub fn fail_randomly(&self, one_in: u32) {
        self.with(|state| state.fail_randomly(one_in));
    }

    /// Makes every later preallocation fail with [`io::ErrorKind::Unsupported`], as on a file system without
    /// `fallocate(2)`.
    pub fn refuse_preallocation(&self) {
        self.with(SimState::refuse_preallocation);
    }

    /// Cuts the power: every file returns to its stable contents and every directory to its stable entries.
    pub fn power_loss(&self) {
        self.with(SimState::power_loss);
    }

    /// Every operation so far, in order, with the path it named (the source of a rename).
    pub fn trace(&self) -> Vec<(StorageOperation, PathBuf)> {
        self.with(|state| state.trace().to_vec())
            .unwrap_or_default()
    }

    /// Every fault that fired, in order: the operation's index in the trace, the operation and the mode. A silent torn
    /// write is reported nowhere else.
    pub fn injected(&self) -> Vec<(usize, StorageOperation, FaultMode)> {
        self.with(|state| state.injected().to_vec())
            .unwrap_or_default()
    }

    /// The buffered contents of the file at `path`, what a read returns now; `None` when there is none.
    pub fn contents(&self, path: &Path) -> Option<Vec<u8>> {
        self.with(|state| {
            let inode = state.lookup(path, false).ok()?;
            state.file(inode).ok().cloned()
        })
        .flatten()
    }

    /// The contents the file at `path` would have after a power loss; `None` when its entry would be gone.
    pub fn stable_contents(&self, path: &Path) -> Option<Vec<u8>> {
        self.with(|state| state.stable_file(path).cloned())
            .flatten()
    }

    /// Writes `bytes` as the file at `path`, durable with its entry and its missing parent directories, replacing a
    /// file there: test setup outside the trace and the faults.
    pub fn put_file(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        self.with_io(|state| state.put_file(path, bytes))
    }

    /// XORs the byte at `offset` of the file at `path` with `mask`, in both its buffered and stable contents: a bit
    /// flip on the medium, outside the trace and the faults.
    pub fn corrupt(&self, path: &Path, offset: usize, mask: u8) -> io::Result<()> {
        self.with_io(|state| state.corrupt(path, offset, mask))
    }
}
