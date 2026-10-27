/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::BTreeMap;
use std::io;
use std::path::{Component, Path, PathBuf};

use crate::types::fault_mode::FaultMode;
use crate::types::sim_storage::sim_inode::SimInode;
use crate::types::sim_storage::sim_tear::SimTear;
use crate::types::storage_operation::StorageOperation;

/// The simulated filesystem behind every clone of a [`crate::SimStorage`] and its files: the inodes (0 is the root
/// directory), the trace, the faults set and fired, and the seeded generator every draw comes from.
#[derive(Debug)]
pub(crate) struct SimState {
    inodes: Vec<SimInode>,
    trace: Vec<(StorageOperation, PathBuf)>,
    injected: Vec<(usize, StorageOperation, FaultMode)>,
    faults: BTreeMap<usize, FaultMode>,
    one_in: u32,
    draws: u64,
    preallocation_refused: bool,
}

pub(crate) fn error(kind: io::ErrorKind, message: &'static str) -> io::Error {
    io::Error::new(kind, message)
}

fn missing() -> io::Error {
    error(io::ErrorKind::NotFound, "no such file or directory")
}

impl SimState {
    pub(crate) fn new(seed: u64) -> Self {
        Self {
            inodes: vec![SimInode::directory()],
            trace: Vec::new(),
            injected: Vec::new(),
            faults: BTreeMap::new(),
            one_in: 0,
            draws: seed,
            preallocation_refused: false,
        }
    }

    /// The next draw of SplitMix64 (Steele, Lea and Flood, OOPSLA 2014), seeded by the storage's seed.
    fn draw(&mut self) -> u64 {
        self.draws = self.draws.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.draws;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A draw below `bound`, which is not zero.
    fn draw_below(&mut self, bound: u64) -> u64 {
        self.draw() % bound.max(1)
    }

    pub(crate) fn fail_at(&mut self, operation: usize, mode: FaultMode) {
        self.faults.insert(operation, mode);
    }

    pub(crate) fn fail_randomly(&mut self, one_in: u32) {
        self.one_in = one_in;
    }

    pub(crate) fn refuse_preallocation(&mut self) {
        self.preallocation_refused = true;
    }

    pub(crate) fn preallocation_refused(&self) -> bool {
        self.preallocation_refused
    }

    pub(crate) fn power_loss(&mut self) {
        self.inodes.iter_mut().for_each(SimInode::restore);
    }

    pub(crate) fn trace(&self) -> &[(StorageOperation, PathBuf)] {
        &self.trace
    }

    pub(crate) fn injected(&self) -> &[(usize, StorageOperation, FaultMode)] {
        &self.injected
    }

    /// The names of an absolute path without `.` or `..`.
    fn names(path: &Path) -> io::Result<Vec<String>> {
        let mut components = path.components();
        if components.next() != Some(Component::RootDir) {
            return Err(error(io::ErrorKind::InvalidInput, "not an absolute path"));
        }
        components
            .map(|component| match component {
                Component::Normal(name) => Ok(name.to_string_lossy().into_owned()),
                _ => Err(error(io::ErrorKind::InvalidInput, "not a plain path")),
            })
            .collect()
    }

    /// The entries of directory `inode`, buffered or stable.
    pub(crate) fn entries(
        &self,
        inode: usize,
        stable: bool,
    ) -> io::Result<&BTreeMap<String, usize>> {
        // Every inode index comes from an entry, so only a file is not a directory.
        match self.inodes.get(inode) {
            Some(SimInode::Directory { entries, stable: s }) => {
                Ok(if stable { s } else { entries })
            }
            _ => Err(error(io::ErrorKind::NotADirectory, "not a directory")),
        }
    }

    fn entries_mut(&mut self, inode: usize) -> io::Result<&mut BTreeMap<String, usize>> {
        match self.inodes.get_mut(inode) {
            Some(SimInode::Directory { entries, .. }) => Ok(entries),
            _ => Err(error(io::ErrorKind::NotADirectory, "not a directory")),
        }
    }

    /// The inode at `path`, through the buffered or the stable entries.
    pub(crate) fn lookup(&self, path: &Path, stable: bool) -> io::Result<usize> {
        Self::names(path)?.iter().try_fold(0, |inode, name| {
            self.entries(inode, stable)?
                .get(name)
                .copied()
                .ok_or_else(missing)
        })
    }

    /// The directory holding `path` and the name `path` has in it.
    fn parent(&self, path: &Path) -> io::Result<(usize, String)> {
        let mut names = Self::names(path)?;
        let name = names
            .pop()
            .ok_or_else(|| error(io::ErrorKind::InvalidInput, "the root has no parent"))?;
        let dir = names.iter().try_fold(0, |inode, n| {
            self.entries(inode, false)?
                .get(n)
                .copied()
                .ok_or_else(missing)
        })?;
        self.entries(dir, false)?;
        Ok((dir, name))
    }

    /// The buffered contents of file `inode`.
    pub(crate) fn file(&self, inode: usize) -> io::Result<&Vec<u8>> {
        match self.inodes.get(inode) {
            Some(SimInode::File { buffered, .. }) => Ok(buffered),
            _ => Err(error(io::ErrorKind::IsADirectory, "a directory")),
        }
    }

    pub(crate) fn file_mut(&mut self, inode: usize) -> io::Result<&mut Vec<u8>> {
        match self.inodes.get_mut(inode) {
            Some(SimInode::File { buffered, .. }) => Ok(buffered),
            _ => Err(error(io::ErrorKind::IsADirectory, "a directory")),
        }
    }

    /// The stable contents of the file at `path` through the stable entries.
    pub(crate) fn stable_file(&self, path: &Path) -> Option<&Vec<u8>> {
        match self.inodes.get(self.lookup(path, true).ok()?) {
            Some(SimInode::File { stable, .. }) => Some(stable),
            _ => None,
        }
    }

    /// The file at `path`: an existing one, or with `create` a new empty one, cut to empty with `cut`.
    pub(crate) fn open(&mut self, path: &Path, create: bool, cut: bool) -> io::Result<usize> {
        if !create {
            let inode = self.lookup(path, false)?;
            self.file(inode)?;
            return Ok(inode);
        }
        let (dir, name) = self.parent(path)?;
        let inode = match self.entries(dir, false)?.get(&name).copied() {
            Some(inode) => inode,
            None => self.insert(dir, name, SimInode::empty_file())?,
        };
        let file = self.file_mut(inode)?;
        if cut {
            file.clear();
        }
        Ok(inode)
    }

    fn insert(&mut self, dir: usize, name: String, inode: SimInode) -> io::Result<usize> {
        self.entries(dir, false)?;
        self.inodes.push(inode);
        let index = self.inodes.len() - 1;
        self.entries_mut(dir)?.insert(name, index);
        Ok(index)
    }

    pub(crate) fn create_directory(&mut self, path: &Path) -> io::Result<()> {
        let (dir, name) = self.parent(path)?;
        if self.entries(dir, false)?.contains_key(&name) {
            return Err(error(io::ErrorKind::AlreadyExists, "the entry exists"));
        }
        self.insert(dir, name, SimInode::directory()).map(|_| ())
    }

    pub(crate) fn sync_directory(&mut self, path: &Path) -> io::Result<()> {
        let inode = self.lookup(path, false)?;
        match self.inodes.get_mut(inode) {
            Some(SimInode::Directory { entries, stable }) => {
                stable.clone_from(entries);
                Ok(())
            }
            _ => Err(error(io::ErrorKind::NotADirectory, "not a directory")),
        }
    }

    pub(crate) fn sync_file(&mut self, inode: usize) -> io::Result<()> {
        // A file handle's inode is a file.
        if let Some(SimInode::File { buffered, stable }) = self.inodes.get_mut(inode) {
            stable.clone_from(buffered);
        }
        Ok(())
    }

    pub(crate) fn rename(&mut self, from: &Path, to: &Path) -> io::Result<()> {
        let (dir, name) = self.parent(from)?;
        let inode = self
            .entries(dir, false)?
            .get(&name)
            .copied()
            .ok_or_else(missing)?;
        let (to_dir, to_name) = self.parent(to)?;
        if let Some(target) = self.entries(to_dir, false)?.get(&to_name).copied() {
            self.file(target)?;
        }
        self.entries_mut(dir)?.remove(&name);
        self.entries_mut(to_dir)?.insert(to_name, inode);
        Ok(())
    }

    pub(crate) fn remove_file(&mut self, path: &Path) -> io::Result<()> {
        let (dir, name) = self.parent(path)?;
        let inode = self
            .entries(dir, false)?
            .get(&name)
            .copied()
            .ok_or_else(missing)?;
        self.file(inode)?;
        self.entries_mut(dir)?.remove(&name);
        Ok(())
    }

    /// `bytes` as the file at `path`, durable with every directory on its way, created where missing.
    pub(crate) fn put_file(&mut self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        let mut names = Self::names(path)?;
        let name = names
            .pop()
            .ok_or_else(|| error(io::ErrorKind::InvalidInput, "the root is not a file"))?;
        let mut dir = 0;
        for n in names {
            let next = match self.entries(dir, false)?.get(&n).copied() {
                Some(inode) => inode,
                None => self.insert(dir, n.clone(), SimInode::directory())?,
            };
            self.stabilise_entry(dir, &n, next);
            dir = next;
        }
        let file = self.insert(dir, name.clone(), SimInode::file(bytes.to_vec()))?;
        self.stabilise_entry(dir, &name, file);
        Ok(())
    }

    fn stabilise_entry(&mut self, dir: usize, name: &str, inode: usize) {
        if let Some(SimInode::Directory { stable, .. }) = self.inodes.get_mut(dir) {
            stable.insert(name.to_string(), inode);
        }
    }

    /// XORs byte `offset` of the file at `path` with `mask` in both views: a flip on the medium.
    pub(crate) fn corrupt(&mut self, path: &Path, offset: usize, mask: u8) -> io::Result<()> {
        let inode = self.lookup(path, false)?;
        match self.inodes.get_mut(inode) {
            Some(SimInode::File { buffered, stable }) => {
                let byte = buffered.get_mut(offset).ok_or_else(|| {
                    error(io::ErrorKind::InvalidInput, "past the end of the file")
                })?;
                *byte ^= mask;
                if let Some(byte) = stable.get_mut(offset) {
                    *byte ^= mask;
                }
                Ok(())
            }
            _ => Err(error(io::ErrorKind::IsADirectory, "a directory")),
        }
    }

    /// Runs `action` as operation `op` naming `path`: recorded in the trace, then faulted when a fault is set for its
    /// index or drawn. `action` is told whether, and how, to tear its write.
    pub(crate) fn perform<T>(
        &mut self,
        op: StorageOperation,
        path: &Path,
        action: impl FnOnce(&mut Self, Option<SimTear>) -> io::Result<T>,
    ) -> io::Result<T> {
        let index = self.trace.len();
        self.trace.push((op, path.to_path_buf()));
        let mode = self
            .fault_for(index)
            .filter(|mode| *mode != FaultMode::SilentTornWrite || op == StorageOperation::Write);
        if let Some(mode) = mode {
            self.injected.push((index, op, mode));
        }
        if mode == Some(FaultMode::Before) {
            return Err(error(
                io::ErrorKind::Other,
                "injected fault before the operation",
            ));
        }
        let tear = match mode {
            _ if op != StorageOperation::Write => None,
            Some(FaultMode::TornWrite | FaultMode::SilentTornWrite) => Some(SimTear::WholeLength),
            Some(FaultMode::ShortTornWrite) => Some(SimTear::ShortLength),
            _ => None,
        };
        let value = action(self, tear)?;
        if matches!(
            mode,
            Some(FaultMode::After | FaultMode::TornWrite | FaultMode::ShortTornWrite)
        ) {
            return Err(error(
                io::ErrorKind::Other,
                "injected fault after the operation",
            ));
        }
        Ok(value)
    }

    fn fault_for(&mut self, index: usize) -> Option<FaultMode> {
        if let Some(mode) = self.faults.remove(&index) {
            return Some(mode);
        }
        if self.one_in == 0 || self.draw_below(u64::from(self.one_in)) != 0 {
            return None;
        }
        Some(match self.draw_below(4) {
            0 => FaultMode::Before,
            1 => FaultMode::After,
            2 if self.draw_below(2) == 0 => FaultMode::TornWrite,
            2 => FaultMode::ShortTornWrite,
            _ => FaultMode::SilentTornWrite,
        })
    }

    /// How many bytes of a write of `len` a tear keeps: whole 512-byte sectors, at least one and never all, drawn from
    /// the seed; inside a write of at most one sector, at least one byte and never all.
    pub(crate) fn tear(&mut self, len: usize) -> usize {
        let len = len as u64;
        let kept = if len > 512 {
            512 * (1 + self.draw_below((len - 1) / 512))
        } else if len > 1 {
            1 + self.draw_below(len - 1)
        } else {
            0
        };
        kept as usize
    }
}
