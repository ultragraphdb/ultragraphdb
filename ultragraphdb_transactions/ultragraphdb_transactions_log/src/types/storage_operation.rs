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
    /// Reading a file's bytes.
    Read,
    /// Writing a file's bytes.
    Write,
    /// Reserving a file's blocks.
    Preallocate,
    /// Reading a file's length.
    Length,
    /// Setting a file's length.
    Truncate,
    /// Making a file's contents durable.
    FileSync,
    /// Creating a directory.
    CreateDirectory,
    /// Making a directory's entries durable.
    DirectorySync,
    /// Renaming a file.
    Rename,
    /// Removing a file.
    Remove,
    /// Listing a directory's names.
    List,
}
