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
    let sizes = DirSizes::of(&dir, &data).unwrap();
    assert_eq!(sizes.data().apparent(), 10_010);
    assert_eq!(sizes.journal().apparent(), 5_000);
    assert_eq!(sizes.metadata().apparent(), 12);
    // What du reports is the blocks allocated, at least the bytes written.
    assert!(sizes.data().allocated() >= 10_010);
    assert!(sizes.journal().allocated() >= 5_000);
    assert_eq!(
        sizes.total().apparent(),
        sizes.data().apparent() + sizes.journal().apparent() + sizes.metadata().apparent()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_a_missing_directory_is_refused() {
    let dir = fresh("missing");
    assert!(DirSizes::of(&dir, &dir.join("data")).is_err());
}
