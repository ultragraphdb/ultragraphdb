/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What the log's tests share: a small log over the simulated storage, deterministic round bodies, the paths of its
//! files, an independent frame checksum, and a temporary directory for the compio tests. Shared by the unit tests and,
//! by path, the BDD target.

use std::future::Future;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use ultragraphdb_error_log::LogError;
use ultragraphdb_traits_log::{OrderedLog, Position, RoundRef};
use ultragraphdb_transactions_log::{DurableStorage, Recovery, SimStorage, Wal, WalConfig};

/// The cluster of every test log.
pub const CLUSTER: u64 = 0x0123_4567_89AB_CDEF;

/// The log's directory on the simulated storage.
pub const DIR: &str = "/wal";

/// A body of at most 3,840 bytes is one chunk, so its frame is one 4,096-byte block.
pub const ONE_BLOCK: usize = 1_000;

/// A body of 3,840 bytes is one chunk filling its block: a 4,096-byte frame, so a tear anywhere inside it is damage.
pub const FULL_BLOCK: usize = 3_840;

/// A body of 7,920 bytes takes two chunks and a 16-byte table: an 8,192-byte frame filling two blocks.
pub const FULL_TWO_BLOCKS: usize = 7_920;

/// A body of 5,000 bytes takes two chunks and a 16-byte table: a 5,272-byte frame, two blocks.
pub const TWO_BLOCKS: usize = 5_000;

pub fn dir() -> PathBuf {
    PathBuf::from(DIR)
}

/// Segment `index` of the log in `dir`.
pub fn segment_in(dir: &Path, index: u64) -> PathBuf {
    dir.join(format!("{index:020}.segment"))
}

/// Segment `index` of the log in [`DIR`].
pub fn segment(index: u64) -> PathBuf {
    segment_in(&dir(), index)
}

/// Superblock copy `copy` of the log in [`DIR`].
pub fn superblock(copy: char) -> PathBuf {
    dir().join(format!("superblock.{copy}"))
}

/// A small log: segments of four blocks, 16,384 bytes, at most eight of them.
pub fn config() -> WalConfig {
    WalConfig::new(CLUSTER, false)
        .with_segment_blocks(NonZeroU32::new(4).expect("nonzero"))
        .with_capacity(NonZeroU32::new(8).expect("nonzero"))
}

/// Runs a future of the simulated storage, which completes when first polled, by polling it with a waker that does
/// nothing. It is not an executor, so it runs inside the BDD target's own.
pub fn block_on<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    loop {
        if let std::task::Poll::Ready(output) = future.as_mut().poll(&mut context) {
            return output;
        }
    }
}

/// Runs a future on a compio runtime of its own.
pub fn run<F: Future>(future: F) -> F::Output {
    compio::runtime::Runtime::new()
        .expect("a compio runtime")
        .block_on(future)
}

/// The log in [`DIR`] on `storage`, with [`config`].
pub fn open(storage: &SimStorage) -> Result<(Wal<SimStorage>, Recovery), LogError> {
    block_on(Wal::open(storage.clone(), DIR, config()))
}

/// The body of round `position`: byte i is `position * 31 + i * 7`, so no two rounds' bodies agree.
pub fn body(position: u64, len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| (position.wrapping_mul(31) as usize).wrapping_add(i * 7) as u8)
        .collect()
}

/// Appends rounds `from` to `to` after the log's tail, each with a body of `len` bytes, first version `2 * position`
/// and one transaction, and syncs each: their checksums, from the tail after each append.
pub fn append_synced<S: DurableStorage>(
    wal: &mut Wal<S>,
    from: u64,
    to: u64,
    len: usize,
) -> Vec<u64> {
    (from..=to)
        .map(|position| {
            append(wal, position, len).expect("the round is accepted");
            block_on(wal.sync()).expect("the round syncs");
            wal.tail().1
        })
        .collect()
}

/// Appends round `position` after the log's tail, unsynced.
pub fn append<S: DurableStorage>(
    wal: &mut Wal<S>,
    position: u64,
    len: usize,
) -> Result<Position, LogError> {
    let parent = wal.tail().1;
    let bytes = body(position, len);
    wal.append(RoundRef::new(
        Position::new(position),
        parent,
        position.wrapping_mul(2),
        1,
        &bytes,
    ))
}

/// XXH3-64 of a frame's header bytes 8 to 256, its `checksum`, computed with `xxhash_rust` directly.
pub fn frame_checksum(frame: &[u8]) -> u64 {
    xxhash_rust::xxh3::xxh3_64(&frame[8..256])
}

static NEXT: AtomicU64 = AtomicU64::new(0);

/// A directory of its own under Bazel's `TEST_TMPDIR`, or the system's temporary directory under Cargo, removed when
/// the test ends.
#[derive(Debug)]
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(name: &str) -> Self {
        let root = std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, PathBuf::from);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = root.join(format!("wal-{name}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("the temporary directory is created");
        Self(dir)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
