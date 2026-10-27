/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::io;
use std::path::Path;

use compio::fs::OpenOptions;

use crate::traits::durable_storage::DurableStorage;
use crate::types::compio_file::CompioFile;
use crate::types::compio_storage::CompioStorage;
use crate::types::open_mode::OpenMode;

/// `O_DIRECT` from Linux's `asm-generic/fcntl.h` on x86-64 and from `arch/arm64/include/uapi/asm/fcntl.h` on arm64.
/// Written here rather than taken from `libc`, which the crate does not depend on.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const O_DIRECT: i32 = 0o40_000;
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
const O_DIRECT: i32 = 0o200_000;

/// The options of `mode`, with `O_DIRECT` for an aligned file on the Linux targets the node ships for.
fn options(mode: OpenMode, aligned: bool) -> OpenOptions {
    let mut options = OpenOptions::new();
    options.read(true).write(mode != OpenMode::Read);
    if matches!(mode, OpenMode::Create | OpenMode::CreateOrOpen) {
        options.create(true).truncate(mode == OpenMode::Create);
    }
    #[cfg(all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    ))]
    if aligned {
        options.custom_flags(O_DIRECT);
    }
    let _ = aligned;
    options
}

impl DurableStorage for CompioStorage {
    type File = CompioFile;

    async fn open(&self, path: &Path, mode: OpenMode) -> io::Result<CompioFile> {
        let file = options(mode, false).open(path).await?;
        Ok(CompioFile::new(file, false))
    }

    async fn open_aligned(&self, path: &Path, mode: OpenMode) -> io::Result<CompioFile> {
        let file = options(mode, true).open(path).await?;
        Ok(CompioFile::new(file, true))
    }

    async fn create_directory(&self, path: &Path) -> io::Result<()> {
        compio::fs::create_dir(path).await
    }

    /// The directory opened for reading and synced, as `fsync(2)` on a directory's descriptor makes its entries
    /// durable.
    async fn sync_directory(&self, path: &Path) -> io::Result<()> {
        compio::fs::File::open(path).await?.sync_all().await
    }

    async fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        compio::fs::rename(from, to).await
    }

    async fn remove_file(&self, path: &Path) -> io::Result<()> {
        compio::fs::remove_file(path).await
    }

    /// Read through `std::fs`: compio has no asynchronous directory listing, and the log lists only when it opens.
    async fn list(&self, path: &Path) -> io::Result<Vec<String>> {
        let mut names = std::fs::read_dir(path)?
            .map(|entry| entry.map(|e| e.file_name().to_string_lossy().into_owned()))
            .collect::<io::Result<Vec<String>>>()?;
        names.sort();
        Ok(names)
    }
}
