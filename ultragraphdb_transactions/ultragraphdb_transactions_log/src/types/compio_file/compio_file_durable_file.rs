/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io;

use compio::buf::{BufResult, IntoInner, IoBuf};
use compio::io::{AsyncReadAtExt, AsyncWriteAtExt};

use crate::traits::durable_file::DurableFile;
use crate::types::aligned_buf::AlignedBuf;
use crate::types::compio_file::CompioFile;
use crate::utils::block::aligned;

impl CompioFile {
    /// Refused as [`io::ErrorKind::InvalidInput`] on an aligned file unless `offset` and `len` are whole blocks: what
    /// `O_DIRECT` demands on Linux, checked here on every platform so a misaligned call fails the same way everywhere.
    fn check(&self, offset: u64, len: u64) -> io::Result<()> {
        if self.aligned && !aligned(offset, len) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "an aligned file takes whole blocks",
            ));
        }
        Ok(())
    }
}

impl DurableFile for CompioFile {
    async fn read_at(&self, offset: u64, len: usize) -> io::Result<AlignedBuf> {
        self.check(offset, len as u64)?;
        let (bytes, start, len) = AlignedBuf::zeroed(len).into_parts();
        let BufResult(result, slice) = self
            .file
            .read_exact_at(bytes.slice(start..start + len), offset)
            .await;
        result?;
        Ok(AlignedBuf::from_parts(slice.into_inner(), start, len))
    }

    async fn write_at(&mut self, offset: u64, bytes: AlignedBuf) -> io::Result<()> {
        self.check(offset, bytes.len() as u64)?;
        let (bytes, start, len) = bytes.into_parts();
        let BufResult(result, _) = self
            .file
            .write_all_at(bytes.slice(start..start + len), offset)
            .await;
        result
    }

    /// On Linux, `fallocate(2)` with `FALLOC_FL_KEEP_SIZE` over the first `len` bytes, through `rustix`: the blocks
    /// are reserved as unwritten extents and the length is left, so the bytes past it still read as absent. The call
    /// blocks, so it runs on compio's blocking pool, as compio runs its own blocking file calls, on a duplicate of the
    /// descriptor. The first write into an unwritten extent converts it, a metadata change the round's `fdatasync`
    /// then flushes with the length it extends (measured in task 12.4). A file system without `fallocate`
    /// (`EOPNOTSUPP`) is refused as [`io::ErrorKind::Unsupported`]. On macOS nothing is reserved.
    async fn preallocate(&mut self, len: u64) -> io::Result<()> {
        self.check(0, len)?;
        #[cfg(target_os = "linux")]
        {
            let fd = std::os::fd::AsFd::as_fd(&self.file).try_clone_to_owned()?;
            compio::runtime::spawn_blocking(move || {
                rustix::fs::fallocate(&fd, rustix::fs::FallocateFlags::KEEP_SIZE, 0, len).map_err(
                    |errno| {
                        let error = io::Error::from_raw_os_error(errno.raw_os_error());
                        if errno == rustix::io::Errno::OPNOTSUPP {
                            io::Error::new(io::ErrorKind::Unsupported, error)
                        } else {
                            error
                        }
                    },
                )
            })
            .await
            .map_err(|_| io::Error::other("the preallocation's blocking task failed"))??;
        }
        Ok(())
    }

    async fn length(&self) -> io::Result<u64> {
        Ok(self.file.metadata().await?.len())
    }

    async fn truncate(&mut self, len: u64) -> io::Result<()> {
        self.file.set_len(len).await
    }

    /// `fdatasync` on Linux through compio's `sync_data`; on macOS compio issues `fsync`, which does not flush the
    /// drive's cache.
    async fn sync(&self) -> io::Result<()> {
        self.file.sync_data().await
    }
}
