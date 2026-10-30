/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io;

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
use crate::utils::zeros::write_zeros;

impl<S: DurableStorage> Wal<S> {
    /// Writes a staged round: its segment made when new, then the round's one write and one sync.
    pub(crate) async fn write_round(
        &mut self,
        entry: WalEntry,
        frame: AlignedBuf,
    ) -> Result<(), LogError> {
        let file = match self.files.remove(&entry.segment) {
            Some(file) => file,
            None => self.new_segment(entry.segment).await?,
        };
        let file = self.files.entry(entry.segment).or_insert(file);
        file.write_at(entry.offset, frame)
            .await
            .map_err(io_error("write"))?;
        file.sync().await.map_err(io_error("file sync"))
    }

    /// Records `position`, a committed round above the checkpoint, and `version`, the last commit version it covers, as
    /// the checkpoint, then removes the segments whose rounds all lie at or below it, short of the segment the next
    /// round is written to.
    pub(crate) async fn checkpoint_at(
        &mut self,
        position: Position,
        version: u64,
    ) -> Result<(), LogError> {
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
            version,
            keep,
        );
        record.write(&self.storage, &self.dir).await?;
        self.superblock = record;
        self.recycle_segments_below(keep).await
    }

    /// Recycles every segment below `first`, all of whose rounds the superblock's checkpoint covers: each is zeroed
    /// whole to the segment size and synced, then renamed a spare; the directory is synced when any was. The zeros are
    /// durable before the spare's name, so a spare never holds an old round, and a crash before the rename leaves a
    /// segment below the first kept, which the next open recycles again. The superblock is written first: zeroing a
    /// segment it still keeps would destroy rounds above its checkpoint.
    pub(crate) async fn recycle_segments_below(&mut self, first: u64) -> Result<(), LogError> {
        let below: Vec<u64> = self.files.range(..first).map(|(index, _)| *index).collect();
        let size = self.config.segment_size();
        for &index in &below {
            let Some(mut file) = self.files.remove(&index) else {
                continue;
            };
            write_zeros(&mut file, 0, size)
                .await
                .map_err(io_error("write"))?;
            file.sync().await.map_err(io_error("file sync"))?;
            self.storage
                .rename(&self.segment_path(index), &self.spare_path(index))
                .await
                .map_err(io_error("rename"))?;
            self.spares.insert(index);
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

    /// Segment `index`, new: the lowest spare renamed to it, its entry made durable, and opened, so nothing is
    /// allocated and the rounds written into it change no metadata; with no spare, a file created, its blocks
    /// reserved, synced and its entry made durable.
    async fn new_segment(&mut self, index: u64) -> Result<S::File, LogError> {
        let path = self.segment_path(index);
        if let Some(spare) = self.spares.pop_first() {
            self.storage
                .rename(&self.spare_path(spare), &path)
                .await
                .map_err(io_error("rename"))?;
            self.sync_dir().await?;
            return self
                .storage
                .open_aligned(&path, OpenMode::ReadWrite)
                .await
                .map_err(io_error("open"));
        }
        let mut file = self
            .storage
            .open_aligned(&path, OpenMode::Create)
            .await
            .map_err(io_error("create"))?;
        reserve(
            &mut file,
            0,
            self.config.segment_size(),
            &mut self.unreserved,
        )
        .await?;
        file.sync().await.map_err(io_error("file sync"))?;
        self.sync_dir().await?;
        Ok(file)
    }

    async fn sync_dir(&self) -> Result<(), LogError> {
        self.storage
            .sync_directory(&self.dir)
            .await
            .map_err(io_error("directory sync"))
    }
}

/// Reserves the blocks of a segment of `size` bytes whose bytes before `from`, a block offset, are written. A file
/// system that reserves none (`EOPNOTSUPP`) gets zeros written from `from` to the end instead, unsynced, which reserve
/// the space as well, so a full disk still shows at the segment's creation; the first time, `unreserved` is set and a
/// warning logged.
pub(crate) async fn reserve<F: DurableFile>(
    file: &mut F,
    from: u64,
    size: u64,
    unreserved: &mut bool,
) -> Result<(), LogError> {
    match file.preallocate(size).await {
        Err(error) if error.kind() == io::ErrorKind::Unsupported => {
            if !*unreserved {
                *unreserved = true;
                tracing::warn!(
                    error = %error,
                    "the file system reserves no blocks: write-ahead log segments are filled with zeros"
                );
            }
            write_zeros(file, from, size)
                .await
                .map_err(io_error("write"))
        }
        other => other.map_err(io_error("preallocate")),
    }
}
