/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A batch of 1 GiB is one fjall batch: no size limit splits it, and it is visible whole, before and after a reopen.

use std::path::PathBuf;

use ultragraphdb_store_kv::{Durability, FjallStore};
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_store::{Entry, PersistMode, Scan, Store, StoreSnapshot, WriteBatch};

const VALUES: u32 = 1024;
const VALUE_SIZE: usize = 1 << 20;

fn key(i: u32) -> Vec<u8> {
    [b"\x00gib".as_slice(), &i.to_be_bytes()].concat()
}

#[test]
fn test_a_batch_of_one_gib_is_written_whole() -> TestResult {
    let root = std::env::var_os("TEST_TMPDIR").map_or_else(std::env::temp_dir, PathBuf::from);
    let dir = root.join(format!("kv-gib-{}", std::process::id()));
    // A directory an earlier run left behind, which is usually absent.
    drop(std::fs::remove_dir_all(&dir));
    let store = FjallStore::open(&dir, Durability::Process).context("opens")?;
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
        .context("one batch of 1 GiB is written")?;
    let check = |opened: &FjallStore| -> TestResult {
        let snapshot = opened.snapshot(5).context("readable")?;
        let mut n = 0u32;
        for item in snapshot.scan(&Scan::prefix(b"\x00gib".to_vec())) {
            let item = item.context("read")?;
            assert_eq!(item.value().len(), VALUE_SIZE);
            assert_eq!(item.value()[VALUE_SIZE - 1], (n % 251) as u8);
            n += 1;
        }
        assert_eq!(n, VALUES);
        assert!(
            opened
                .snapshot(5)
                .context("readable")?
                .get(&key(0))
                .context("read")?
                .is_some()
        );
        assert_eq!(
            opened
                .snapshot(4)
                .context("readable")?
                .get(&key(0))
                .context("read")?,
            None
        );
        assert_eq!(opened.applied_ts(), 5);
        Ok(())
    };
    check(&store)?;
    drop(store);
    let reopened = FjallStore::open(&dir, Durability::Process).context("reopens")?;
    check(&reopened)?;
    drop(reopened);
    // A directory left behind is litter in the temporary directory, not a failure of the test.
    drop(std::fs::remove_dir_all(&dir));
    println!("wrote and read back {VALUES} values of {VALUE_SIZE} bytes in one batch");
    Ok(())
}
