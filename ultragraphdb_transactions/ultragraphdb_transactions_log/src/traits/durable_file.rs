/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::future::Future;
use std::io;

use crate::types::aligned_buf::AlignedBuf;

/// An open file of a [`crate::DurableStorage`], Apache Iggy's `DurableFile`. A completed write is not durable: it
/// survives a power loss only once [`sync`](Self::sync) has completed after it. A file opened with
/// [`crate::DurableStorage::open_aligned`] takes reads and writes at offsets and lengths that are multiples of
/// 4,096 bytes only, and refuses others with [`io::ErrorKind::InvalidInput`].
pub trait DurableFile: std::fmt::Debug {
    /// The `len` bytes at `offset`, refused with [`io::ErrorKind::UnexpectedEof`] when they run past the end.
    fn read_at(&self, offset: u64, len: usize) -> impl Future<Output = io::Result<AlignedBuf>>;

    /// Writes `bytes` at `offset` in one write, growing the file when they run past its end.
    fn write_at(&mut self, offset: u64, bytes: AlignedBuf) -> impl Future<Output = io::Result<()>>;

    /// Grows the file to `len` bytes of zeros with its blocks allocated, so a later write within it changes no
    /// metadata a sync must flush; bytes already written are kept. A file at least `len` long is left as it is.
    fn preallocate(&mut self, len: u64) -> impl Future<Output = io::Result<()>>;

    /// The file's length in bytes.
    fn length(&self) -> impl Future<Output = io::Result<u64>>;

    /// Cuts or grows the file to `len` bytes, growing it with zeros.
    fn truncate(&mut self, len: u64) -> impl Future<Output = io::Result<()>>;

    /// Makes the file's contents and length durable: `fdatasync`.
    fn sync(&self) -> impl Future<Output = io::Result<()>>;
}
