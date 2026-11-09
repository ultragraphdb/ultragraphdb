/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io;
use std::path::Path;

use ultragraphdb_transactions_log::{DurableStorage, OpenMode};

use crate::types::timed_file::TimedFile;
use crate::types::timed_storage::TimedStorage;
use crate::utils::micros_since::micros_since;

/// Each future is `inner`'s, so a future dropped at its wait leaves what `inner`'s leaves; a dropped directory sync counts nothing.
impl<S: DurableStorage> DurableStorage for TimedStorage<S> {
    type File = TimedFile<S::File>;

    async fn open(&self, path: &Path, mode: OpenMode) -> io::Result<Self::File> {
        let file = self.inner.open(path, mode).await?;
        Ok(TimedFile::new(file, self.io.clone(), self.origin))
    }

    async fn open_aligned(&self, path: &Path, mode: OpenMode) -> io::Result<Self::File> {
        let file = self.inner.open_aligned(path, mode).await?;
        Ok(TimedFile::new(file, self.io.clone(), self.origin))
    }

    async fn create_directory(&self, path: &Path) -> io::Result<()> {
        self.inner.create_directory(path).await
    }

    async fn sync_directory(&self, path: &Path) -> io::Result<()> {
        let started = micros_since(self.origin);
        let synced = self.inner.sync_directory(path).await;
        self.io
            .synced(micros_since(self.origin).saturating_sub(started));
        synced
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
