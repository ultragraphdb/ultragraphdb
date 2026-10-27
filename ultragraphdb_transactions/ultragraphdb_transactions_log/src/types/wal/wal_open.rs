/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::{BTreeMap, VecDeque};
use std::io;
use std::path::PathBuf;

use ultragraphdb_error_log::LogError;
use ultragraphdb_traits_log::Position;

use crate::traits::durable_storage::DurableStorage;
use crate::types::open_mode::OpenMode;
use crate::types::recovery::Recovery;
use crate::types::superblock::Superblock;
use crate::types::superblock_read::SuperblockRead;
use crate::types::wal::{Wal, segment_index};
use crate::types::wal_config::WalConfig;
use crate::utils::io_error::io_error;

impl<S: DurableStorage> Wal<S> {
    /// Opens the log in `dir`, creating `dir` and a fresh log when neither superblock copy holds a record, and
    /// recovers it: the segments are scanned from the first kept, a torn tail is cut off and reported, and the log
    /// is ready to append after the last durable round.
    ///
    /// Refused as [`LogError::SuperblockUnreadable`] when a copy does not verify; as [`LogError::Diverged`] when the
    /// superblock names another cluster, when segments exist without a superblock, or when the rounds contradict the
    /// checkpoint; as [`LogError::Corrupt`] or [`LogError::CorruptChunk`] when damage has a complete round after it;
    /// as [`LogError::Io`] when the storage fails. Nothing is written before the superblock is read and accepted.
    pub async fn open(
        storage: S,
        dir: impl Into<PathBuf>,
        config: WalConfig,
    ) -> Result<(Self, Recovery), LogError> {
        let dir = dir.into();
        match storage.create_directory(&dir).await {
            Err(error) if error.kind() != io::ErrorKind::AlreadyExists => {
                return Err(io_error("create directory")(error));
            }
            _ => {}
        }
        // The directory's own entry, whether this open or an earlier one created it.
        if let Some(parent) = dir.parent() {
            storage
                .sync_directory(parent)
                .await
                .map_err(io_error("directory sync"))?;
        }
        let names = storage.list(&dir).await.map_err(io_error("list"))?;
        let segments: Vec<u64> = names
            .iter()
            .filter_map(|name| segment_index(name))
            .collect();
        let superblock = match Superblock::read(&storage, &dir).await? {
            SuperblockRead::Unreadable { copy } => {
                return Err(LogError::SuperblockUnreadable(copy));
            }
            SuperblockRead::Present(record) if record.cluster() != config.cluster() => {
                return Err(LogError::Diverged(
                    "the cluster",
                    config.cluster(),
                    record.cluster(),
                ));
            }
            SuperblockRead::Present(record) => record,
            SuperblockRead::Empty if !segments.is_empty() => {
                return Err(LogError::Diverged(
                    "segments without a superblock",
                    0,
                    segments.len() as u64,
                ));
            }
            SuperblockRead::Empty => {
                let record = Superblock::new(1, config.cluster(), 1, Position::ZERO, 0, 0);
                record.write(&storage, &dir).await?;
                record
            }
        };
        let start = (superblock.checkpoint(), superblock.checkpoint_checksum());
        let mut wal = Self {
            storage,
            dir,
            config,
            superblock,
            files: BTreeMap::new(),
            index: VecDeque::new(),
            staged: VecDeque::new(),
            committed: start,
            tail: start,
            cursor: (superblock.first_segment(), 0),
            sealed: false,
            poisoned: false,
        };
        wal.sweep(&names, &segments).await?;
        let truncation = wal.recover().await?;
        let recovery = Recovery::new(wal.checkpoint(), wal.committed.0, truncation);
        Ok((wal, recovery))
    }

    /// Removes what an interrupted superblock write or checkpoint left (temporary copies, segments below the first
    /// kept), opens every segment kept, and syncs the directory: a segment created before a failed directory sync is
    /// read back now, and its entry must be durable before anything is written into it.
    async fn sweep(&mut self, names: &[String], segments: &[u64]) -> Result<(), LogError> {
        let temporaries = names
            .iter()
            .filter(|name| name.starts_with("superblock.") && name.ends_with(".tmp"));
        for name in temporaries {
            self.storage
                .remove_file(&self.dir.join(name))
                .await
                .map_err(io_error("remove"))?;
        }
        let first = self.superblock.first_segment();
        for &index in segments {
            let path = self.segment_path(index);
            if index < first {
                self.storage
                    .remove_file(&path)
                    .await
                    .map_err(io_error("remove"))?;
            } else {
                let file = self
                    .storage
                    .open_aligned(&path, OpenMode::ReadWrite)
                    .await
                    .map_err(io_error("open"))?;
                self.files.insert(index, file);
            }
        }
        self.storage
            .sync_directory(&self.dir)
            .await
            .map_err(io_error("directory sync"))
    }
}
