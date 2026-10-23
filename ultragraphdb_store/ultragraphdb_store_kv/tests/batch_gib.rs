/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A batch of 1 GiB is one fjall batch: no size limit splits it, and it is visible whole, before and after a reopen.

use std::path::PathBuf;

use ultragraphdb_store_kv::{Durability, FjallStore};
use ultragraphdb_traits_store::{Entry, PersistMode, Scan, Store, StoreSnapshot, WriteBatch};

const VALUES: u32 = 1024;
const VALUE_SIZE: usize = 1 << 20;

fn key(i: u32) -> Vec<u8> {
    [b"\x00gib".as_slice(), &i.to_be_bytes()].concat()
}

#[test]
fn test_a_batch_of_one_gib_is_written_whole() {
    let root = std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, PathBuf::from);
    let dir = root.join(format!("kv-gib-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let store = FjallStore::open(&dir, Durability::Process).expect("opens");
    let mut batch = WriteBatch::new();
    (0..VALUES).for_each(|i| {
        batch.push(Entry::put(
            key(i),
            5,
            0x08,
            vec![(i % 251) as u8; VALUE_SIZE],
        ))
    });
    batch.set_applied_ts(5);
    store
        .apply(batch, PersistMode::Sync)
        .expect("one batch of 1 GiB is written");
    let check = |store: &FjallStore| {
        let snapshot = store.snapshot(5).expect("readable");
        let mut n = 0u32;
        for item in snapshot.scan(&Scan::prefix(b"\x00gib".to_vec())) {
            let item = item.expect("read");
            assert_eq!(item.value().len(), VALUE_SIZE);
            assert_eq!(item.value()[VALUE_SIZE - 1], (n % 251) as u8);
            n += 1;
        }
        assert_eq!(n, VALUES);
        assert!(
            store
                .snapshot(5)
                .expect("readable")
                .get(&key(0))
                .expect("read")
                .is_some()
        );
        assert_eq!(
            store
                .snapshot(4)
                .expect("readable")
                .get(&key(0))
                .expect("read"),
            None
        );
        assert_eq!(store.applied_ts(), 5);
    };
    check(&store);
    drop(store);
    let store = FjallStore::open(&dir, Durability::Process).expect("reopens");
    check(&store);
    drop(store);
    let _ = std::fs::remove_dir_all(&dir);
    println!("wrote and read back {VALUES} values of {VALUE_SIZE} bytes in one batch");
}
