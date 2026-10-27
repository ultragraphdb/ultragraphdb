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

/// Zeros are written in pieces of at most this many bytes when a file is preallocated.
const ZEROS: u64 = 1 << 20;

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

    /// Zeros written from the end of the file to `len`: blocks written once need no allocation, and no extent
    /// conversion, when a round is later written over them and synced with `fdatasync`. `fallocate(2)` reserves
    /// blocks without writing them, and the first write into an unwritten extent changes metadata a data sync must
    /// flush.
    async fn preallocate(&mut self, len: u64) -> io::Result<()> {
        let mut at = self.length().await?;
        if at >= len {
            return Ok(());
        }
        self.check(at, len)?;
        while at < len {
            let piece = (len - at).min(ZEROS);
            self.write_at(at, AlignedBuf::zeroed(piece as usize))
                .await?;
            at += piece;
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
