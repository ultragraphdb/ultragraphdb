/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A log's storage whose file syncs a test can hold: every operation goes to the storage it wraps, and a file's sync waits while
//! the test holds the gate, then syncs. What awaits a log over it waits as it waits on a real sync, so a test can send requests
//! while a sync is truly in flight. The tests run on one thread, so the gate's state is `Rc`'s.

use std::cell::Cell;
use std::io;
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

use crate::traits::durable_file::DurableFile;
use crate::traits::durable_storage::DurableStorage;
use crate::types::aligned_buf::AlignedBuf;
use crate::types::open_mode::OpenMode;

/// How often a held sync looks at the gate.
const LOOK: Duration = Duration::from_millis(1);
/// How long [`SyncGate::until_waiting`] waits for a sync to reach the gate.
const DUE: Duration = Duration::from_secs(10);

/// The gate the syncs of one storage wait at, and the syncs waiting there now.
#[derive(Debug, Clone, Default)]
pub struct SyncGate {
    held: Rc<Cell<bool>>,
    waiting: Rc<Cell<u64>>,
}

impl SyncGate {
    /// Holds every file sync from now on, until [`SyncGate::release`].
    pub fn hold(&self) {
        self.held.set(true);
    }

    /// Lets every sync held go on, and every later one.
    pub fn release(&self) {
        self.held.set(false);
    }

    /// Waits until a sync waits at the gate.
    ///
    /// # Errors
    ///
    /// [`io::ErrorKind::TimedOut`] when none reaches it within [`DUE`].
    pub async fn until_waiting(&self) -> io::Result<()> {
        let mut waited = Duration::ZERO;
        while self.waiting.get() == 0 {
            if waited >= DUE {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!("no sync reached the gate within {DUE:?}"),
                ));
            }
            compio::time::sleep(LOOK).await;
            waited += LOOK;
        }
        Ok(())
    }

    /// Waits while the gate is held, counted among the syncs waiting.
    async fn pass(&self) {
        if !self.held.get() {
            return;
        }
        self.waiting.set(self.waiting.get() + 1);
        while self.held.get() {
            compio::time::sleep(LOOK).await;
        }
        self.waiting.set(self.waiting.get() - 1);
    }
}

/// `S`, its files' syncs held at a [`SyncGate`].
#[derive(Debug)]
pub struct GatedStorage<S> {
    inner: S,
    gate: SyncGate,
}

impl<S> GatedStorage<S> {
    /// `inner`, its files' syncs waiting at `gate`.
    pub fn new(inner: S, gate: SyncGate) -> Self {
        Self { inner, gate }
    }
}

/// A file of a [`GatedStorage`].
#[derive(Debug)]
pub struct GatedFile<F> {
    inner: F,
    gate: SyncGate,
}

impl<S: DurableStorage> DurableStorage for GatedStorage<S> {
    type File = GatedFile<S::File>;

    async fn open(&self, path: &Path, mode: OpenMode) -> io::Result<Self::File> {
        let inner = self.inner.open(path, mode).await?;
        Ok(GatedFile {
            inner,
            gate: self.gate.clone(),
        })
    }

    async fn open_aligned(&self, path: &Path, mode: OpenMode) -> io::Result<Self::File> {
        let inner = self.inner.open_aligned(path, mode).await?;
        Ok(GatedFile {
            inner,
            gate: self.gate.clone(),
        })
    }

    async fn create_directory(&self, path: &Path) -> io::Result<()> {
        self.inner.create_directory(path).await
    }

    async fn sync_directory(&self, path: &Path) -> io::Result<()> {
        self.inner.sync_directory(path).await
    }

    async fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.inner.rename(from, to).await
    }

    async fn remove_file(&self, path: &Path) -> io::Result<()> {
        self.inner.remove_file(path).await
    }

    async fn list(&self, path: &Path) -> io::Result<Vec<String>> {
        self.inner.list(path).await
    }
}

impl<F: DurableFile> DurableFile for GatedFile<F> {
    async fn read_at(&self, offset: u64, len: usize) -> io::Result<AlignedBuf> {
        self.inner.read_at(offset, len).await
    }

    async fn write_at(&mut self, offset: u64, bytes: AlignedBuf) -> io::Result<()> {
        self.inner.write_at(offset, bytes).await
    }

    async fn preallocate(&mut self, len: u64) -> io::Result<()> {
        self.inner.preallocate(len).await
    }

    async fn length(&self) -> io::Result<u64> {
        self.inner.length().await
    }

    async fn truncate(&mut self, len: u64) -> io::Result<()> {
        self.inner.truncate(len).await
    }

    async fn sync(&self) -> io::Result<()> {
        self.gate.pass().await;
        self.inner.sync().await
    }
}
