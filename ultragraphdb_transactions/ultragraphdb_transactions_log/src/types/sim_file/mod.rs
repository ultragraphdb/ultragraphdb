/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod sim_file_durable_file;

use std::cell::RefCell;
use std::io;
use std::path::PathBuf;
use std::rc::Rc;

use crate::types::sim_storage::sim_state::{SimState, error};
use crate::types::sim_storage::sim_tear::SimTear;
use crate::types::storage_operation::StorageOperation;

/// A file of a [`crate::SimStorage`], open on its inode: a rename or removal of its entry does not close it.
#[derive(Debug)]
pub struct SimFile {
    state: Rc<RefCell<SimState>>,
    inode: usize,
    aligned: bool,
    path: PathBuf,
}

impl SimFile {
    pub(crate) fn new(
        state: Rc<RefCell<SimState>>,
        inode: usize,
        aligned: bool,
        path: PathBuf,
    ) -> Self {
        Self {
            state,
            inode,
            aligned,
            path,
        }
    }

    /// Runs `action` on this file's inode as operation `op`, traced and faulted.
    fn perform<T>(
        &self,
        op: StorageOperation,
        action: impl FnOnce(&mut SimState, usize, Option<SimTear>) -> io::Result<T>,
    ) -> io::Result<T> {
        let mut state = self
            .state
            .try_borrow_mut()
            .map_err(|_| error(io::ErrorKind::WouldBlock, "the simulated storage is in use"))?;
        let inode = self.inode;
        state.perform(op, &self.path, |state, tear| action(state, inode, tear))
    }

    /// `offset` as an index, refused as [`io::ErrorKind::InvalidInput`] on an aligned file unless it and `len` are
    /// whole blocks.
    fn at(&self, offset: u64, len: usize) -> io::Result<usize> {
        if self.aligned && !crate::utils::block::aligned(offset, len as u64) {
            return Err(error(
                io::ErrorKind::InvalidInput,
                "an aligned file takes whole blocks",
            ));
        }
        usize::try_from(offset)
            .map_err(|_| error(io::ErrorKind::InvalidInput, "the offset is too large"))
    }
}
