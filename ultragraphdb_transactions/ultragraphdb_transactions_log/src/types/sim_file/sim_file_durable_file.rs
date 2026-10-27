/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io;

use crate::traits::durable_file::DurableFile;
use crate::types::aligned_buf::AlignedBuf;
use crate::types::sim_file::SimFile;
use crate::types::sim_storage::sim_state::error;
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
            let len = if tear {
                state.tear(bytes.len())
            } else {
                bytes.len()
            };
            let end = at
                .checked_add(len)
                .ok_or_else(|| error(io::ErrorKind::InvalidInput, "the range overflows"))?;
            let file = state.file_mut(inode)?;
            if file.len() < end {
                file.resize(end, 0);
            }
            file[at..end].copy_from_slice(&bytes.as_slice()[..len]);
            Ok(())
        })
    }

    async fn preallocate(&mut self, len: u64) -> io::Result<()> {
        let aligned_file = self.aligned;
        self.perform(StorageOperation::Preallocate, |state, inode, _| {
            let file = state.file_mut(inode)?;
            let len = usize::try_from(len)
                .map_err(|_| error(io::ErrorKind::InvalidInput, "too large"))?;
            if file.len() >= len {
                return Ok(());
            }
            if aligned_file && !aligned(file.len() as u64, len as u64) {
                return Err(error(
                    io::ErrorKind::InvalidInput,
                    "an aligned file takes whole blocks",
                ));
            }
            file.resize(len, 0);
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
