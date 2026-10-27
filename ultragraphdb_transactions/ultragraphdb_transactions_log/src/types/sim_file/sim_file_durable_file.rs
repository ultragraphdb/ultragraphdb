/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io;

use crate::traits::durable_file::DurableFile;
use crate::types::aligned_buf::AlignedBuf;
use crate::types::sim_file::SimFile;
use crate::types::sim_storage::sim_state::error;
use crate::types::sim_storage::sim_tear::SimTear;
use crate::types::storage_operation::StorageOperation;
use crate::utils::block::aligned;

impl DurableFile for SimFile {
    async fn read_at(&self, offset: u64, len: usize) -> io::Result<AlignedBuf> {
        let at = self.at(offset, len)?;
        self.perform(StorageOperation::Read, |state, inode, _| {
            let end = at
                .checked_add(len)
                .ok_or_else(|| error(io::ErrorKind::InvalidInput, "the range overflows"))?;
            state
                .file(inode)?
                .get(at..end)
                .map(AlignedBuf::from_slice)
                .ok_or_else(|| error(io::ErrorKind::UnexpectedEof, "the range runs past the end"))
        })
    }

    async fn write_at(&mut self, offset: u64, bytes: AlignedBuf) -> io::Result<()> {
        let at = self.at(offset, bytes.len())?;
        self.perform(StorageOperation::Write, |state, inode, tear| {
            let kept = match tear {
                Some(_) => state.tear(bytes.len()),
                None => bytes.len(),
            };
            // The file system records the whole write, whatever the drive kept of it, unless the tear is short.
            let recorded = match tear {
                Some(SimTear::ShortLength) => kept,
                _ => bytes.len(),
            };
            let end = at
                .checked_add(recorded)
                .ok_or_else(|| error(io::ErrorKind::InvalidInput, "the range overflows"))?;
            let file = state.file_mut(inode)?;
            if file.len() < end {
                file.resize(end, 0);
            }
            file[at..at + kept].copy_from_slice(&bytes.as_slice()[..kept]);
            Ok(())
        })
    }

    /// Reserves nothing the model can observe: the contents and length are left, as `FALLOC_FL_KEEP_SIZE` leaves
    /// them. Traced and faulted as any operation, refused on an aligned file unless `len` is whole blocks, and refused
    /// as [`io::ErrorKind::Unsupported`] after [`crate::SimStorage::refuse_preallocation`].
    async fn preallocate(&mut self, len: u64) -> io::Result<()> {
        let aligned_file = self.aligned;
        self.perform(StorageOperation::Preallocate, |state, _, _| {
            if aligned_file && !aligned(0, len) {
                return Err(error(
                    io::ErrorKind::InvalidInput,
                    "an aligned file takes whole blocks",
                ));
            }
            if state.preallocation_refused() {
                return Err(error(
                    io::ErrorKind::Unsupported,
                    "the file system reserves no blocks",
                ));
            }
            Ok(())
        })
    }

    async fn length(&self) -> io::Result<u64> {
        self.perform(StorageOperation::Length, |state, inode, _| {
            Ok(state.file(inode)?.len() as u64)
        })
    }

    async fn truncate(&mut self, len: u64) -> io::Result<()> {
        self.perform(StorageOperation::Truncate, |state, inode, _| {
            let len = usize::try_from(len)
                .map_err(|_| error(io::ErrorKind::InvalidInput, "too large"))?;
            state.file_mut(inode)?.resize(len, 0);
            Ok(())
        })
    }

    async fn sync(&self) -> io::Result<()> {
        self.perform(StorageOperation::FileSync, |state, inode, _| {
            state.sync_file(inode)
        })
    }
}
