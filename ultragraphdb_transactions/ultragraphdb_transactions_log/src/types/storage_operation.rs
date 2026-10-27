/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// An operation of the simulated storage, as its trace records it and as a fault is counted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StorageOperation {
    /// Opening an existing file: [`crate::OpenMode::Read`] or [`crate::OpenMode::ReadWrite`].
    Open,
    /// Opening with [`crate::OpenMode::Create`] or [`crate::OpenMode::CreateOrOpen`].
    Create,
    Read,
    Write,
    Preallocate,
    Length,
    Truncate,
    FileSync,
    CreateDirectory,
    DirectorySync,
    Rename,
    Remove,
    List,
}
