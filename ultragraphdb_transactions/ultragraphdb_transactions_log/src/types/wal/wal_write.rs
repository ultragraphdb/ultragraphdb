/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::btree_map::Entry;
use std::path::Path;

use ultragraphdb_error_log::LogError;
use ultragraphdb_traits_log::Position;

use crate::traits::durable_file::DurableFile;
use crate::traits::durable_storage::DurableStorage;
use crate::types::aligned_buf::AlignedBuf;
use crate::types::open_mode::OpenMode;
use crate::types::superblock::Superblock;
use crate::types::wal::Wal;
use crate::types::wal::wal_entry::WalEntry;
use crate::utils::io_error::io_error;

impl<S: DurableStorage> Wal<S> {
    /// Writes a staged round: its segment created, preallocated, synced and its directory synced when new, then the
    /// round's one write and one sync.
    pub(crate) async fn write_round(
        &mut self,
        entry: WalEntry,
        frame: AlignedBuf,
    ) -> Result<(), LogError> {
        let path = self.segment_path(entry.segment);
        let file = match self.files.entry(entry.segment) {
            Entry::Occupied(occupied) => occupied.into_mut(),
            Entry::Vacant(vacant) => vacant.insert(
                create_segment(&self.storage, &path, &self.dir, self.config.segment_size()).await?,
            ),
        };
        file.write_at(entry.offset, frame)
            .await
            .map_err(io_error("write"))?;
        file.sync().await.map_err(io_error("file sync"))
    }

    /// Records `position`, a committed round above the checkpoint, as the checkpoint, then removes the segments whose
    /// rounds all lie at or below it, short of the segment the next round is written to.
    pub(crate) async fn checkpoint_at(&mut self, position: Position) -> Result<(), LogError> {
        let Some(checksum) = self
            .index
            .iter()
            .find(|entry| entry.position == position)
            .map(|entry| entry.checksum)
        else {
            // Every committed round above the checkpoint is indexed.
            return Err(LogError::PositionMismatch(
                self.committed.0.get(),
                position.get(),
            ));
        };
        let keep = self
            .index
            .iter()
            .chain(self.staged.iter().map(|(entry, _)| entry))
            .find(|entry| entry.position > position)
            .map_or(self.cursor.0, |entry| entry.segment);
        let record = Superblock::new(
            self.superblock.sequence() + 1,
            self.config.cluster(),
            self.superblock.incarnation(),
            position,
            checksum,
            keep,
        );
        record.write(&self.storage, &self.dir).await?;
        self.superblock = record;
        self.remove_segments_below(keep).await
    }

    /// Removes every segment below `first`, the WAL's own files, then syncs the directory when any was removed.
    pub(crate) async fn remove_segments_below(&mut self, first: u64) -> Result<(), LogError> {
        let below: Vec<u64> = self.files.range(..first).map(|(index, _)| *index).collect();
        for index in &below {
            self.files.remove(index);
            let path = self.segment_path(*index);
            self.storage
                .remove_file(&path)
                .await
                .map_err(io_error("remove"))?;
        }
        self.index.retain(|entry| entry.segment >= first);
        if below.is_empty() {
            return Ok(());
        }
        self.storage
            .sync_directory(&self.dir)
            .await
            .map_err(io_error("directory sync"))
    }
}

/// A new segment at `path` in `dir`: created, preallocated to `size` and synced, its entry made durable.
async fn create_segment<S: DurableStorage>(
    storage: &S,
    path: &Path,
    dir: &Path,
    size: u64,
) -> Result<S::File, LogError> {
    let mut file = storage
        .open_aligned(path, OpenMode::Create)
        .await
        .map_err(io_error("create"))?;
    file.preallocate(size)
        .await
        .map_err(io_error("preallocate"))?;
    file.sync().await.map_err(io_error("file sync"))?;
    storage
        .sync_directory(dir)
        .await
        .map_err(io_error("directory sync"))?;
    Ok(file)
}
