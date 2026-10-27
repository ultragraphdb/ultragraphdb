/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::future::Future;
use std::io;
use std::path::Path;

use crate::traits::durable_file::DurableFile;
use crate::types::open_mode::OpenMode;

/// The filesystem operations whose completion and durability decide what the log recovers, Apache Iggy's
/// `DurableStorage`: implemented over compio and by the simulated storage.
///
/// A completed operation is not durable. A file's contents are durable once its [`DurableFile::sync`] completes; a
/// directory's entries (a created, renamed or removed file) once [`sync_directory`](Self::sync_directory) of that
/// directory completes. Syncing a directory does not sync the directories inside it.
pub trait DurableStorage {
    type File: DurableFile;

    /// Opens the file at `path` as `mode` says, for reads and writes at any offset.
    fn open(&self, path: &Path, mode: OpenMode) -> impl Future<Output = io::Result<Self::File>>;

    /// Opens the file at `path` as `mode` says, for reads and writes at offsets and lengths that are multiples of
    /// 4,096 bytes, with direct I/O where the platform offers it.
    fn open_aligned(
        &self,
        path: &Path,
        mode: OpenMode,
    ) -> impl Future<Output = io::Result<Self::File>>;

    /// Creates the directory `path`, whose parent exists; refused with [`io::ErrorKind::AlreadyExists`] when it
    /// exists.
    fn create_directory(&self, path: &Path) -> impl Future<Output = io::Result<()>>;

    /// Makes the entries of the directory `path` durable.
    fn sync_directory(&self, path: &Path) -> impl Future<Output = io::Result<()>>;

    /// Renames the file `from` to `to`, replacing a file at `to`.
    fn rename(&self, from: &Path, to: &Path) -> impl Future<Output = io::Result<()>>;

    /// Removes the file `path`.
    fn remove_file(&self, path: &Path) -> impl Future<Output = io::Result<()>>;

    /// The names of the entries of the directory `path`, in name order.
    fn list(&self, path: &Path) -> impl Future<Output = io::Result<Vec<String>>>;
}
