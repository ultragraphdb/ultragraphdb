/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod compio_storage_durable_storage;

/// The storage of the running node: files through compio, io_uring on Linux. An aligned file is opened with
/// `O_DIRECT` on Linux, so its writes bypass the page cache and its reads see the drive. macOS has no `O_DIRECT`, and
/// its replacement, `fcntl(F_NOCACHE)`, is reachable only through an unsafe call: there an aligned file is an
/// ordinary buffered file, and compio's `sync_data` is `fsync`, which on macOS does not flush the drive's cache
/// (`F_FULLFSYNC` would), and a preallocation reserves nothing. macOS is a development platform; the node ships for
/// Linux.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CompioStorage;
