/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::path::PathBuf;

use ultragraphdb_tool_bench::DirSizes;

fn fresh(name: &str) -> PathBuf {
    let root = std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, PathBuf::from);
    let dir = root.join(format!("bench-sizes-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn test_the_sizes_split_data_journal_and_metadata() {
    let dir = fresh("split");
    let data = dir.join("keyspaces").join("1");
    std::fs::create_dir_all(data.join("tables")).unwrap();
    std::fs::create_dir_all(dir.join("keyspaces").join("2")).unwrap();
    std::fs::write(data.join("tables").join("0"), vec![1u8; 10_000]).unwrap();
    std::fs::write(data.join("current"), vec![1u8; 10]).unwrap();
    std::fs::write(dir.join("0.jnl"), vec![1u8; 3_000]).unwrap();
    std::fs::write(dir.join("1.jnl"), vec![1u8; 2_000]).unwrap();
    std::fs::write(dir.join("version"), vec![1u8; 7]).unwrap();
    std::fs::write(
        dir.join("keyspaces").join("2").join("current"),
        vec![1u8; 5],
    )
    .unwrap();
    // A journal's name below the top is no journal of the store's: metadata.
    std::fs::write(
        dir.join("keyspaces").join("2").join("9.jnl"),
        vec![1u8; 100],
    )
    .unwrap();
    // A file beside the data keyspace whose name only begins as it does is not in it.
    std::fs::write(dir.join("keyspaces").join("10"), vec![1u8; 1_000]).unwrap();
    let sizes = DirSizes::of(&dir, &data).unwrap();
    // data: 10,000 + 10; journal: 3,000 + 2,000; metadata: 7 + 5 + 100 + 1,000.
    assert_eq!(sizes.data().apparent(), 10_010);
    assert_eq!(sizes.journal().apparent(), 5_000);
    assert_eq!(sizes.metadata().apparent(), 1_112);
    assert_eq!(sizes.total().apparent(), 16_122);
    // What du reports is the blocks allocated, at least the bytes written, and each file a whole number of
    // 512-byte blocks.
    assert!(sizes.data().allocated() >= 10_010);
    assert!(sizes.journal().allocated() >= 5_000);
    assert!(sizes.metadata().allocated() >= 1_112);
    for size in [sizes.data(), sizes.journal(), sizes.metadata()] {
        assert_eq!(size.allocated() % 512, 0, "{size:?}");
    }
    assert_eq!(
        sizes.total().allocated(),
        sizes.data().allocated() + sizes.journal().allocated() + sizes.metadata().allocated()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_a_missing_directory_is_refused() {
    let dir = fresh("missing");
    let error = DirSizes::of(&dir, &dir.join("data")).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
}

#[test]
fn test_an_empty_directory_has_no_size() {
    let dir = fresh("empty");
    std::fs::create_dir_all(&dir).unwrap();
    assert_eq!(
        DirSizes::of(&dir, &dir.join("data")).unwrap(),
        DirSizes::default()
    );
    let _ = std::fs::remove_dir_all(&dir);
}
