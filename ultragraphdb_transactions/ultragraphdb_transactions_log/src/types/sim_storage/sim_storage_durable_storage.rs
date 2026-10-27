/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io;
use std::path::Path;

use crate::traits::durable_storage::DurableStorage;
use crate::types::open_mode::OpenMode;
use crate::types::sim_file::SimFile;
use crate::types::sim_storage::SimStorage;
use crate::types::storage_operation::StorageOperation;

impl SimStorage {
    fn open_file(&self, path: &Path, mode: OpenMode, aligned: bool) -> io::Result<SimFile> {
        let (op, create) = match mode {
            OpenMode::Read | OpenMode::ReadWrite => (StorageOperation::Open, false),
            OpenMode::Create | OpenMode::CreateOrOpen => (StorageOperation::Create, true),
        };
        let inode = self.with_io(|state| {
            state.perform(op, path, |state, _| {
                state.open(path, create, mode == OpenMode::Create)
            })
        })?;
        Ok(SimFile::new(
            self.state(),
            inode,
            aligned,
            path.to_path_buf(),
        ))
    }
}

impl DurableStorage for SimStorage {
    type File = SimFile;

    async fn open(&self, path: &Path, mode: OpenMode) -> io::Result<SimFile> {
        self.open_file(path, mode, false)
    }

    async fn open_aligned(&self, path: &Path, mode: OpenMode) -> io::Result<SimFile> {
        self.open_file(path, mode, true)
    }

    async fn create_directory(&self, path: &Path) -> io::Result<()> {
        self.with_io(|state| {
            state.perform(StorageOperation::CreateDirectory, path, |state, _| {
                state.create_directory(path)
            })
        })
    }

    async fn sync_directory(&self, path: &Path) -> io::Result<()> {
        self.with_io(|state| {
            state.perform(StorageOperation::DirectorySync, path, |state, _| {
                state.sync_directory(path)
            })
        })
    }

    async fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        self.with_io(|state| {
            state.perform(StorageOperation::Rename, from, |state, _| {
                state.rename(from, to)
            })
        })
    }

    async fn remove_file(&self, path: &Path) -> io::Result<()> {
        self.with_io(|state| {
            state.perform(StorageOperation::Remove, path, |state, _| {
                state.remove_file(path)
            })
        })
    }

    async fn list(&self, path: &Path) -> io::Result<Vec<String>> {
        self.with_io(|state| {
            state.perform(StorageOperation::List, path, |state, _| {
                let inode = state.lookup(path, false)?;
                Ok(state.entries(inode, false)?.keys().cloned().collect())
            })
        })
    }
}
