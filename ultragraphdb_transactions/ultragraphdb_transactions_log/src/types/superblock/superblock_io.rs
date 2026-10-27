/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io;
use std::path::{Path, PathBuf};

use ultragraphdb_error_log::LogError;

use crate::traits::durable_file::DurableFile;
use crate::traits::durable_storage::DurableStorage;
use crate::types::aligned_buf::AlignedBuf;
use crate::types::open_mode::OpenMode;
use crate::types::superblock::Superblock;
use crate::types::superblock_read::SuperblockRead;
use crate::utils::io_error::io_error;

/// What one copy holds.
enum Copy {
    Absent,
    Unreadable,
    Valid(Superblock),
}

/// Copy `copy`'s file in `dir`.
pub(crate) fn copy_path(dir: &Path, copy: char) -> PathBuf {
    dir.join(format!("superblock.{copy}"))
}

/// Copy `copy` of `dir`: absent when the file is missing or empty; unreadable when it is not one record long, which
/// is known before any byte is read, or its record does not decode.
async fn read_copy<S: DurableStorage>(
    storage: &S,
    dir: &Path,
    copy: char,
) -> Result<Copy, LogError> {
    let file = match storage.open(&copy_path(dir, copy), OpenMode::Read).await {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Copy::Absent),
        Err(error) => return Err(io_error("open")(error)),
    };
    match file.length().await.map_err(io_error("length"))? {
        0 => return Ok(Copy::Absent),
        len if len != Superblock::SIZE as u64 => return Ok(Copy::Unreadable),
        _ => {}
    }
    let bytes = file
        .read_at(0, Superblock::SIZE)
        .await
        .map_err(io_error("read"))?;
    Ok(Superblock::decode(bytes.as_slice()).map_or(Copy::Unreadable, Copy::Valid))
}

impl Superblock {
    /// Both copies in `dir` (`superblock.a`, `superblock.b`) read and combined, as Apache Iggy's ping-pong
    /// superblock: neither present (absent or empty) is [`SuperblockRead::Empty`]; one valid copy beside an absent
    /// one, or two valid copies, is [`SuperblockRead::Present`] with the higher sequence; a copy holding bytes that do
    /// not decode is [`SuperblockRead::Unreadable`] naming it, `a` first, whatever the other holds. A failed read is
    /// [`LogError::Io`].
    pub async fn read<S: DurableStorage>(
        storage: &S,
        dir: &Path,
    ) -> Result<SuperblockRead, LogError> {
        let a = read_copy(storage, dir, 'a').await?;
        let b = read_copy(storage, dir, 'b').await?;
        Ok(match (a, b) {
            (Copy::Unreadable, _) => SuperblockRead::Unreadable { copy: 'a' },
            (_, Copy::Unreadable) => SuperblockRead::Unreadable { copy: 'b' },
            (Copy::Absent, Copy::Absent) => SuperblockRead::Empty,
            (Copy::Valid(record), Copy::Absent) | (Copy::Absent, Copy::Valid(record)) => {
                SuperblockRead::Present(record)
            }
            (Copy::Valid(a), Copy::Valid(b)) => {
                SuperblockRead::Present(if a.sequence() >= b.sequence() { a } else { b })
            }
        })
    }

    /// Writes the record to its copy: a temporary file beside it, synced, renamed over the copy, and `dir` synced, so
    /// a crash at any step leaves the other copy as it was. A failed step is [`LogError::Io`].
    pub async fn write<S: DurableStorage>(&self, storage: &S, dir: &Path) -> Result<(), LogError> {
        let target = copy_path(dir, self.copy());
        let temporary = dir.join(format!("superblock.{}.tmp", self.copy()));
        let mut file = storage
            .open(&temporary, OpenMode::Create)
            .await
            .map_err(io_error("create"))?;
        file.write_at(0, AlignedBuf::from_slice(&self.encode()))
            .await
            .map_err(io_error("write"))?;
        file.sync().await.map_err(io_error("file sync"))?;
        storage
            .rename(&temporary, &target)
            .await
            .map_err(io_error("rename"))?;
        storage
            .sync_directory(dir)
            .await
            .map_err(io_error("directory sync"))
    }
}
