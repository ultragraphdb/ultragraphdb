/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The metadata keyspace: what compaction acts on is durable when its call returns, and read back at open.

use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_kv::{Durability, FjallStore, META_KEYSPACE};
use ultragraphdb_traits_store::Store;

use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_tool_test::{TestDir, ns_key, put, write, write_raw_in};

#[test]
fn test_the_settings_survive_a_reopen() -> TestResult {
    let dir = TestDir::new("meta");
    let s = dir.open()?;
    s.set_discard_ts(5)?;
    s.set_tombstone_horizon(3)?;
    s.ban_namespace(7)?;
    drop(s);
    let reopened = dir.open()?;
    assert_eq!(
        (reopened.discard_ts(), reopened.tombstone_horizon()),
        (5, 3)
    );
    let seven = ns_key(7, b"x");
    write(&reopened, vec![put(&seven, 6, b"")])?;
    assert_eq!(reopened.get_at(&seven, 6)?, None);
    Ok(())
}

#[test]
fn test_the_horizon_never_exceeds_the_discard_timestamp() -> TestResult {
    let dir = TestDir::new("meta-horizon");
    let s = dir.open()?;
    s.set_tombstone_horizon(9)?;
    assert_eq!(s.tombstone_horizon(), 0);
    s.set_discard_ts(4)?;
    assert_eq!(s.tombstone_horizon(), 4);
    s.set_discard_ts(12)?;
    assert_eq!(s.tombstone_horizon(), 9);
    Ok(())
}

#[test]
fn test_a_reopen_restores_the_banned_set_of_several_namespaces() -> TestResult {
    let dir = TestDir::new("meta-banned");
    let s = dir.open()?;
    for ns in [0, 1, 256, u64::MAX] {
        s.ban_namespace(ns)?;
    }
    drop(s);
    let reopened = dir.open()?;
    for ns in [0, 1, 256, u64::MAX] {
        write(&reopened, vec![put(&ns_key(ns, b"x"), 2, b"")])?;
        assert_eq!(reopened.get_at(&ns_key(ns, b"x"), 2)?, None, "{ns}");
    }
    write(&reopened, vec![put(&ns_key(2, b"x"), 2, b"")])?;
    assert_eq!(
        reopened.get_at(&ns_key(2, b"x"), 2)?.map(|i| i.version()),
        Some(2),
        "a namespace no ban names"
    );
    Ok(())
}

#[test]
fn test_unreadable_metadata_refuses_the_open() -> TestResult {
    let mut short_ban = b"banned/".to_vec();
    short_ban.extend_from_slice(&[0, 0, 0, 7]);
    for (key, value, reason) in [
        (
            b"applied_ts".to_vec(),
            vec![0; 3],
            "3 bytes where eight belong",
        ),
        (
            b"discard_ts".to_vec(),
            vec![0; 9],
            "9 bytes where eight belong",
        ),
        (
            b"tombstone_horizon".to_vec(),
            Vec::new(),
            "0 bytes where eight belong",
        ),
        (short_ban, Vec::new(), "a banned namespace is eight bytes"),
    ] {
        let dir = TestDir::new("meta-corrupt");
        drop(dir.open()?);
        write_raw_in(&dir, META_KEYSPACE, &key, &value)?;
        assert_eq!(
            FjallStore::open(dir.path(), Durability::Process)
                .err()
                .context("the expected error")?,
            StoreError::corrupted(key.clone(), reason.to_string()),
            "{reason}"
        );
    }
    Ok(())
}
