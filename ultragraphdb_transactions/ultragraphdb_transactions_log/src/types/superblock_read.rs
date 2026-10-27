/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::superblock::Superblock;

/// The three outcomes of reading the superblock. "Never written" and "written, now unreadable" ask for opposite
/// responses, so they are never merged: only [`SuperblockRead::Empty`] starts a fresh log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SuperblockRead {
    /// Neither copy holds a record: a fresh deployment.
    Empty,
    /// The newest record that verifies.
    Present(Superblock),
    /// Copy `copy` holds bytes that do not verify; its sequence is lost, so it may have been the newer.
    Unreadable { copy: char },
}
