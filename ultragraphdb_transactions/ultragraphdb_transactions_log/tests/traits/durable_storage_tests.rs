/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The storage seam's contract, written once over `DurableStorage` and run on both implementations: the simulated
//! storage under `/`, and compio in a temporary directory. What only one of them does (the simulator's durability
//! model and faults, direct I/O) is tested beside that implementation.

use std::io::ErrorKind;
use std::path::Path;

use ultragraphdb_transactions_log::{
    AlignedBuf, CompioStorage, DurableFile, DurableStorage, OpenMode, SimStorage,
};

use crate::utils::helpers::{TempDir, block_on, run};

async fn directories_and_names<S: DurableStorage>(storage: &S, root: &Path) {
    let dir = root.join("d");
    storage.create_directory(&dir).await.expect("created");
    let again = storage.create_directory(&dir).await.expect_err("exists");
    assert_eq!(again.kind(), ErrorKind::AlreadyExists);
    let missing_parent = storage
        .create_directory(&root.join("no").join("such"))
        .await
        .expect_err("no parent");
    assert_eq!(missing_parent.kind(), ErrorKind::NotFound);
    assert_eq!(
        storage.list(&dir).await.expect("listed"),
        Vec::<String>::new()
    );
    for name in ["b", "a", "c"] {
        storage
            .open(&dir.join(name), OpenMode::Create)
            .await
            .expect("created");
    }
    assert_eq!(storage.list(&dir).await.expect("listed"), ["a", "b", "c"]);
    storage.sync_directory(&dir).await.expect("synced");
    let missing = storage.list(&root.join("none")).await.expect_err("missing");
    assert_eq!(missing.kind(), ErrorKind::NotFound);
}

async fn rename_and_remove<S: DurableStorage>(storage: &S, root: &Path) {
    let from = root.join("from");
    let to = root.join("to");
    let mut file = storage
        .open(&from, OpenMode::Create)
        .await
        .expect("created");
    file.write_at(0, AlignedBuf::from_slice(b"new"))
        .await
        .expect("written");
    let mut old = storage.open(&to, OpenMode::Create).await.expect("created");
    old.write_at(0, AlignedBuf::from_slice(b"old!"))
        .await
        .expect("written");
    storage.rename(&from, &to).await.expect("renamed over");
    let names = storage.list(root).await.expect("listed");
    assert!(!names.contains(&"from".to_string()));
    let renamed = storage.open(&to, OpenMode::Read).await.expect("opened");
    assert_eq!(renamed.length().await.expect("length"), 3);
    assert_eq!(
        renamed.read_at(0, 3).await.expect("read").as_slice(),
        b"new"
    );
    // A file open before its entry moves keeps reading the same inode.
    assert_eq!(file.read_at(0, 3).await.expect("read").as_slice(), b"new");
    storage.remove_file(&to).await.expect("removed");
    let gone = storage
        .open(&to, OpenMode::Read)
        .await
        .expect_err("removed");
    assert_eq!(gone.kind(), ErrorKind::NotFound);
    let twice = storage.remove_file(&to).await.expect_err("already removed");
    assert_eq!(twice.kind(), ErrorKind::NotFound);
    let renamed_missing = storage.rename(&from, &to).await.expect_err("no source");
    assert_eq!(renamed_missing.kind(), ErrorKind::NotFound);
}

async fn open_modes<S: DurableStorage>(storage: &S, root: &Path) {
    let path = root.join("modes");
    let read = storage
        .open(&path, OpenMode::Read)
        .await
        .expect_err("missing");
    assert_eq!(read.kind(), ErrorKind::NotFound);
    let write = storage
        .open(&path, OpenMode::ReadWrite)
        .await
        .expect_err("missing");
    assert_eq!(write.kind(), ErrorKind::NotFound);
    let mut file = storage
        .open(&path, OpenMode::CreateOrOpen)
        .await
        .expect("created");
    file.write_at(0, AlignedBuf::from_slice(b"kept"))
        .await
        .expect("written");
    let kept = storage
        .open(&path, OpenMode::CreateOrOpen)
        .await
        .expect("opened");
    assert_eq!(kept.length().await.expect("length"), 4);
    let mut again = storage
        .open(&path, OpenMode::ReadWrite)
        .await
        .expect("opened");
    again
        .write_at(1, AlignedBuf::from_slice(b"E"))
        .await
        .expect("written");
    assert_eq!(kept.read_at(0, 4).await.expect("read").as_slice(), b"kEpt");
    let cut = storage.open(&path, OpenMode::Create).await.expect("cut");
    assert_eq!(cut.length().await.expect("length"), 0);
}

#[test]
fn test_directories_and_names_on_the_simulated_storage() {
    block_on(directories_and_names(&SimStorage::new(1), Path::new("/")));
}

#[test]
fn test_directories_and_names_on_compio() {
    let tmp = TempDir::new("names");
    run(directories_and_names(&CompioStorage, tmp.path()));
}

#[test]
fn test_rename_and_remove_on_the_simulated_storage() {
    block_on(rename_and_remove(&SimStorage::new(1), Path::new("/")));
}

#[test]
fn test_rename_and_remove_on_compio() {
    let tmp = TempDir::new("rename");
    run(rename_and_remove(&CompioStorage, tmp.path()));
}

#[test]
fn test_open_modes_on_the_simulated_storage() {
    block_on(open_modes(&SimStorage::new(1), Path::new("/")));
}

#[test]
fn test_open_modes_on_compio() {
    let tmp = TempDir::new("modes");
    run(open_modes(&CompioStorage, tmp.path()));
}
