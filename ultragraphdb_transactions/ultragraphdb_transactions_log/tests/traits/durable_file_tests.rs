/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The file contract, written once over `DurableFile` and run on both implementations.

use std::io::ErrorKind;
use std::path::Path;

use ultragraphdb_transactions_log::{
    AlignedBuf, CompioStorage, DurableFile, DurableStorage, OpenMode, SimStorage,
};

use crate::utils::helpers::{TempDir, block_on, run};

async fn reads_writes_and_lengths<S: DurableStorage>(storage: &S, root: &Path) {
    let mut file = storage
        .open(&root.join("f"), OpenMode::Create)
        .await
        .expect("created");
    assert_eq!(file.length().await.expect("length"), 0);
    file.write_at(0, AlignedBuf::from_slice(b"hello"))
        .await
        .expect("written");
    // A write past the end grows the file, the gap zeros.
    file.write_at(8, AlignedBuf::from_slice(b"!"))
        .await
        .expect("written");
    assert_eq!(file.length().await.expect("length"), 9);
    assert_eq!(
        file.read_at(0, 9).await.expect("read").as_slice(),
        b"hello\0\0\0!"
    );
    assert_eq!(file.read_at(1, 3).await.expect("read").as_slice(), b"ell");
    assert!(file.read_at(9, 0).await.expect("read").is_empty());
    let past = file.read_at(5, 5).await.expect_err("past the end");
    assert_eq!(past.kind(), ErrorKind::UnexpectedEof);
    file.sync().await.expect("synced");
}

async fn preallocate_and_truncate<S: DurableStorage>(storage: &S, root: &Path) {
    let mut file = storage
        .open(&root.join("p"), OpenMode::Create)
        .await
        .expect("created");
    file.write_at(0, AlignedBuf::from_slice(b"abc"))
        .await
        .expect("written");
    // The blocks are reserved where the platform reserves them; the length and the bytes are unchanged.
    file.preallocate(8_192).await.expect("preallocated");
    assert_eq!(file.length().await.expect("length"), 3);
    assert_eq!(file.read_at(0, 3).await.expect("read").as_slice(), b"abc");
    let past = file.read_at(0, 8_192).await.expect_err("past the end");
    assert_eq!(past.kind(), ErrorKind::UnexpectedEof);
    // A write within the reservation grows the file to the write's end, the gap zeros.
    file.write_at(4_096, AlignedBuf::from_slice(b"xyz"))
        .await
        .expect("written");
    assert_eq!(file.length().await.expect("length"), 4_099);
    let bytes = file.read_at(0, 4_099).await.expect("read");
    assert!(bytes.as_slice()[3..4_096].iter().all(|b| *b == 0));
    assert_eq!(&bytes.as_slice()[4_096..], b"xyz");
    // A file already longer is left as it is.
    file.preallocate(4_096).await.expect("preallocated");
    assert_eq!(file.length().await.expect("length"), 4_099);
    file.truncate(2).await.expect("cut");
    assert_eq!(file.length().await.expect("length"), 2);
    assert_eq!(file.read_at(0, 2).await.expect("read").as_slice(), b"ab");
    file.truncate(4).await.expect("grown");
    assert_eq!(
        file.read_at(0, 4).await.expect("read").as_slice(),
        b"ab\0\0"
    );
}

async fn aligned_files_take_blocks_only<S: DurableStorage>(storage: &S, root: &Path) {
    let mut file = storage
        .open_aligned(&root.join("a"), OpenMode::Create)
        .await
        .expect("created");
    file.preallocate(16_384).await.expect("preallocated");
    let mut block = AlignedBuf::zeroed(8_192);
    block.as_mut_slice()[0] = 0xAB;
    block.as_mut_slice()[8_191] = 0xCD;
    file.write_at(4_096, block).await.expect("an aligned write");
    let back = file.read_at(4_096, 8_192).await.expect("an aligned read");
    assert_eq!(back.as_slice()[0], 0xAB);
    assert_eq!(back.as_slice()[8_191], 0xCD);
    file.sync().await.expect("synced");
    let refusals = [
        file.write_at(100, AlignedBuf::zeroed(4_096)).await,
        file.write_at(0, AlignedBuf::zeroed(100)).await,
    ];
    for refusal in refusals {
        assert_eq!(
            refusal.expect_err("unaligned").kind(),
            ErrorKind::InvalidInput
        );
    }
    for (offset, len) in [(100, 4_096), (0, 100)] {
        let refusal = file.read_at(offset, len).await.expect_err("unaligned");
        assert_eq!(refusal.kind(), ErrorKind::InvalidInput);
    }
    let mut short = storage
        .open_aligned(&root.join("b"), OpenMode::Create)
        .await
        .expect("created");
    let past_a_block = short
        .preallocate(4_097)
        .await
        .expect_err("not whole blocks");
    assert_eq!(past_a_block.kind(), ErrorKind::InvalidInput);
    // The reservation's length is checked, not the file's: a file of 100 bytes reserves a whole block.
    short.truncate(100).await.expect("cut");
    short.preallocate(4_096).await.expect("preallocated");
    assert_eq!(short.length().await.expect("length"), 100);
    // The write's end, not the reservation's.
    let reopened = storage
        .open_aligned(&root.join("a"), OpenMode::Read)
        .await
        .expect("opened");
    assert_eq!(reopened.length().await.expect("length"), 12_288);
}

#[test]
fn test_reads_writes_and_lengths_on_the_simulated_storage() {
    block_on(reads_writes_and_lengths(
        &SimStorage::new(1),
        Path::new("/"),
    ));
}

#[test]
fn test_reads_writes_and_lengths_on_compio() {
    let tmp = TempDir::new("rw");
    run(reads_writes_and_lengths(&CompioStorage, tmp.path()));
}

#[test]
fn test_preallocate_and_truncate_on_the_simulated_storage() {
    block_on(preallocate_and_truncate(
        &SimStorage::new(1),
        Path::new("/"),
    ));
}

#[test]
fn test_preallocate_and_truncate_on_compio() {
    let tmp = TempDir::new("prealloc");
    run(preallocate_and_truncate(&CompioStorage, tmp.path()));
}

#[test]
fn test_aligned_files_on_the_simulated_storage() {
    block_on(aligned_files_take_blocks_only(
        &SimStorage::new(1),
        Path::new("/"),
    ));
}

#[test]
fn test_aligned_files_on_compio() {
    let tmp = TempDir::new("aligned");
    run(aligned_files_take_blocks_only(&CompioStorage, tmp.path()));
}

/// `st_blocks` of the file at `path` after a compio file there reserved 1 MiB, with the file's length.
fn reserved_blocks(name: &str) -> (u64, u64) {
    use std::os::unix::fs::MetadataExt;
    let tmp = TempDir::new(name);
    let path = tmp.path().join("r");
    let length = run(async {
        let mut file = CompioStorage
            .open_aligned(&path, OpenMode::Create)
            .await
            .expect("created");
        file.preallocate(1 << 20).await.expect("preallocated");
        file.length().await.expect("length")
    });
    (std::fs::metadata(&path).expect("metadata").blocks(), length)
}

#[cfg(target_os = "linux")]
#[test]
fn test_preallocate_reserves_the_blocks_on_linux() {
    let (blocks, length) = reserved_blocks("reserve");
    assert_eq!(length, 0, "FALLOC_FL_KEEP_SIZE leaves the length");
    // st_blocks counts 512-byte units (stat(2)): 1 MiB reserved is at least 2,048 of them.
    assert!(blocks >= 2_048, "{blocks} blocks");
}

#[cfg(target_os = "macos")]
#[test]
fn test_preallocate_reserves_nothing_on_macos() {
    let (blocks, length) = reserved_blocks("reserve");
    assert_eq!((blocks, length), (0, 0));
}
