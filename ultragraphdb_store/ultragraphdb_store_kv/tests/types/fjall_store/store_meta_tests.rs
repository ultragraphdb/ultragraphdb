/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The metadata keyspace: what compaction acts on is durable when its call returns, and read back at open.

use std::fs::File;
use std::io::Read;

use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_kv::{Durability, FjallStore, META_KEYSPACE};
use ultragraphdb_traits_store::Store;

use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_tool_test::{TestDir, put, tenant_key, write, write_raw_in};

/// While a raised discard timestamp's sync runs, a commit finishes, its value reads back and a snapshot opens, none waiting for the
/// sync; the timestamp takes effect once it ends. A sync under the writer lock or fjall's journal lock times out here.
#[test]
#[expect(
    clippy::disallowed_methods,
    reason = "the test's subject is a commit on a thread beside the one a store's sync holds"
)]
fn test_a_commit_goes_on_while_the_store_syncs_a_discard() -> TestResult {
    let patience = std::time::Duration::from_secs(10);
    let dir = TestDir::new("discard-sync");
    let s = FjallStore::open(dir.path(), Durability::Log).context("the store opens")?;
    let k = tenant_key(1, b"x");
    let (entered, syncing) = std::sync::mpsc::channel();
    let (finished, released) = std::sync::mpsc::channel::<()>();
    let waited = std::thread::scope(|scope| -> TestResult<_> {
        let store = &s;
        let raising = scope.spawn(move || {
            let mut waited = None;
            let raised = store.set_discard_ts_observed(5, |_| {
                let _sent: Result<(), _> = entered.send(());
                waited = Some(released.recv_timeout(patience));
            });
            (raised, waited)
        });
        syncing.recv_timeout(patience)?;
        write(&s, vec![put(&k, 6, b"v")])?;
        assert!(s.get_at(&k, 6)?.is_some(), "its value reads back");
        assert_eq!(s.discard_ts(), 0, "not in effect before it is durable");
        drop(s.snapshot(1)?);
        finished.send(())?;
        let (raised, waited) = raising
            .join()
            .map_err(|_| TestError::new("the discard's thread panicked"))?;
        raised?;
        waited.context("the sync was reached")
    })?;
    waited?;
    assert_eq!(s.discard_ts(), 5);
    Ok(())
}

/// What a raised discard timestamp syncs holds its record already, in a store of the write-ahead log's durability, where a batch
/// otherwise stays in fjall's buffer: a crash past that sync keeps the record.
#[test]
fn test_a_raised_discard_syncs_the_journal_holding_its_record() -> TestResult {
    let dir = TestDir::new("discard-journal");
    let s = FjallStore::open(dir.path(), Durability::Log).context("the store opens")?;
    let mut held = None;
    s.set_discard_ts_observed(1, |journal| {
        // A journal is preallocated, and a record of a fresh one lies at its start.
        held = File::open(journal).ok().and_then(|file| {
            let mut start = Vec::new();
            file.take(1 << 16)
                .read_to_end(&mut start)
                .ok()
                .map(|_| start)
        });
    })?;
    let bytes = held.context("the journal reads")?;
    let name = b"discard_ts";
    assert!(
        bytes.windows(name.len()).any(|at| at == name),
        "the record reached the file"
    );
    Ok(())
}

/// The journal synced is the one of the highest number, by number and not by name, the one fjall writes to.
#[test]
fn test_a_raised_discard_syncs_the_journal_of_the_highest_number() -> TestResult {
    let dir = TestDir::new("discard-newest");
    let s = FjallStore::open(dir.path(), Durability::Log).context("the store opens")?;
    for number in [9, 10] {
        std::fs::write(dir.path().join(format!("{number}.jnl")), b"")?;
    }
    let mut synced = None;
    s.set_discard_ts_observed(1, |journal| {
        synced = journal.file_name().map(ToOwned::to_owned)
    })?;
    assert_eq!(synced, Some("10.jnl".into()));
    Ok(())
}

#[test]
fn test_the_settings_survive_a_reopen() -> TestResult {
    let dir = TestDir::new("meta");
    let s = dir.open()?;
    s.set_discard_ts(5)?;
    s.set_tombstone_horizon(3)?;
    s.ban_tenant(7)?;
    drop(s);
    let reopened = dir.open()?;
    assert_eq!(
        (reopened.discard_ts(), reopened.tombstone_horizon()),
        (5, 3)
    );
    let seven = tenant_key(7, b"x");
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
fn test_a_reopen_restores_the_banned_set_of_several_tenants() -> TestResult {
    let dir = TestDir::new("meta-banned");
    let s = dir.open()?;
    for tenant in [0, 1, 256, u64::MAX] {
        s.ban_tenant(tenant)?;
    }
    drop(s);
    let reopened = dir.open()?;
    for tenant in [0, 1, 256, u64::MAX] {
        write(&reopened, vec![put(&tenant_key(tenant, b"x"), 2, b"")])?;
        assert_eq!(
            reopened.get_at(&tenant_key(tenant, b"x"), 2)?,
            None,
            "{tenant}"
        );
    }
    write(&reopened, vec![put(&tenant_key(2, b"x"), 2, b"")])?;
    assert_eq!(
        reopened
            .get_at(&tenant_key(2, b"x"), 2)?
            .map(|i| i.version()),
        Some(2),
        "a tenant no ban names"
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
        (short_ban, Vec::new(), "a banned tenant is eight bytes"),
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
