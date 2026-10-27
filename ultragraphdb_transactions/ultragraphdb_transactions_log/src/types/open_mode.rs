/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// How [`crate::DurableStorage::open`] opens a file. Every mode but [`OpenMode::Read`] allows writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OpenMode {
    /// An existing file, for reads only.
    Read,
    /// An existing file, for reads and writes.
    ReadWrite,
    /// A new file, or an existing one cut to empty.
    Create,
    /// An existing file as it is, or a new empty one.
    CreateOrOpen,
}
