/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The simulated store's temporary keyspaces: apart from the data, under the same faults, dropped at recovery.

use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_store_sim::{Fault, FaultConfig, OpRecord, Seed, SimClock, SimStore};
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_store::{
    Entry, MAX_USER_KEY_SIZE, PersistMode, Scan, Store, StoreSnapshot, WriteBatch,
};

fn store() -> SimStore {
    SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new())
}

fn batch(entries: &[(&[u8], u64)]) -> WriteBatch {
    let mut batch = WriteBatch::new();
    entries
        .iter()
        .for_each(|(k, v)| batch.push(Entry::put(k.to_vec(), *v, 0x04, vec![*v as u8])));
    batch
}

#[test]
fn test_a_temporary_keyspace_holds_its_versions_apart_from_the_data() -> TestResult {
    let s = store();
    s.apply(batch(&[(b"\x00k", 3)]), PersistMode::Buffer)?;
    let t = s.create_temporary()?;
    s.apply_temporary(&t, batch(&[(b"\x00k", 1), (b"\x00k", 2), (b"\x00m", 9)]))?;
    let snapshot = s.temporary_snapshot(&t)?;
    assert_eq!(snapshot.read_ts(), u64::MAX);
    let versions: Vec<u64> = snapshot
        .versions(b"\x00k")
        .map(|i| i.map(|item| item.version()))
        .collect::<Result<_, _>>()?;
    assert_eq!(versions, [2, 1], "every version, newest first");
    let keys: Vec<Vec<u8>> = snapshot
        .scan(&Scan::prefix(b"\x00".to_vec()))
        .map(|i| i.map(|item| item.key().to_vec()))
        .collect::<Result<_, _>>()?;
    assert_eq!(keys, [b"\x00k".to_vec(), b"\x00m".to_vec()]);
    assert_eq!(
        s.contents().versions(b"\x00k").len(),
        1,
        "the data holds none of it"
    );
    assert_eq!(s.contents().versions(b"\x00m").len(), 0);
    assert!(s.get_at(b"\x00m", u64::MAX)?.is_none());
    Ok(())
}

#[test]
fn test_a_temporary_snapshot_holds_no_discard_timestamp_back() -> TestResult {
    let s = store();
    let t = s.create_temporary()?;
    let _snapshot = s.temporary_snapshot(&t)?;
    s.set_discard_ts(7)?;
    assert_eq!(s.discard_ts(), 7);
    Ok(())
}

#[test]
fn test_a_temporary_keyspace_keeps_entries_of_a_banned_tenant() -> TestResult {
    let s = store();
    s.ban_tenant(3)?;
    let mut key = vec![0x00];
    key.extend_from_slice(&3u64.to_be_bytes());
    key.push(b'a');
    let t = s.create_temporary()?;
    s.apply_temporary(&t, batch(&[(&key, 1)]))?;
    let item = s.temporary_snapshot(&t)?.get(&key)?;
    assert_eq!(
        item.map(|i| (i.version(), i.meta(), i.value().to_vec())),
        Some((1, 0x04, vec![1]))
    );
    Ok(())
}

#[test]
fn test_an_oversized_key_refuses_the_temporary_batch_whole() -> TestResult {
    let s = store();
    let t = s.create_temporary()?;
    let mut b = batch(&[(b"\x00k", 1)]);
    b.push(Entry::put(vec![1; MAX_USER_KEY_SIZE + 1], 1, 0x04, vec![]));
    let refused = s.apply_temporary(&t, b).err().context("an error")?;
    assert!(matches!(
        refused.0,
        StoreErrorEnum::KeyTooLarge {
            size: 65_001,
            limit: MAX_USER_KEY_SIZE,
            ..
        }
    ));
    assert!(matches!(
        s.take_log().last(),
        Some(OpRecord::TemporaryRefused { temporary, error, .. }) if *temporary == t.id() && *error == refused
    ));
    assert!(s.temporary_snapshot(&t)?.get(b"\x00k")?.is_none());
    assert!(!s.needs_recovery(), "a refusal poisons nothing");
    Ok(())
}

#[test]
fn test_a_dropped_temporary_keyspace_is_gone() -> TestResult {
    let s = store();
    let t = s.create_temporary()?;
    let id = t.id();
    let again = ultragraphdb_traits_store::Temporary::new(id);
    s.drop_temporary(t)?;
    assert_eq!(
        s.apply_temporary(&again, batch(&[(b"\x00k", 1)])),
        Err(StoreError::unknown_temporary(id))
    );
    assert_eq!(
        s.temporary_snapshot(&again).err(),
        Some(StoreError::unknown_temporary(id))
    );
    assert_eq!(
        s.drop_temporary(ultragraphdb_traits_store::Temporary::new(id)),
        Err(StoreError::unknown_temporary(id))
    );
    assert_eq!(
        s.drop_temporary(ultragraphdb_traits_store::Temporary::new(99)),
        Err(StoreError::unknown_temporary(99)),
        "a number never handed out is unknown"
    );
    assert_eq!(s.create_temporary()?.id(), id + 1, "a number is not reused");
    Ok(())
}

#[test]
fn test_temporary_numbers_count_up_from_one() -> TestResult {
    let s = store();
    let ids: Vec<u64> = (0..3)
        .map(|_| s.create_temporary().map(|t| t.id()))
        .collect::<Result<_, _>>()?;
    assert_eq!(ids, [1, 2, 3]);
    Ok(())
}

#[test]
fn test_a_failed_temporary_write_is_a_failed_batch() -> TestResult {
    let s = store();
    let t = s.create_temporary()?;
    s.inject(Fault::FailNextBatch);
    assert_eq!(
        s.apply_temporary(&t, batch(&[(b"\x00k", 1)])),
        Err(StoreError::write_failed(
            "simulated failure during a temporary batch's persist".into()
        ))
    );
    assert!(s.needs_recovery());
    assert_eq!(
        s.temporary_snapshot(&t)?.get(b"\x00k")?,
        None,
        "a poisoned process still reads its temporary keyspaces, without the failed batch"
    );
    assert_eq!(
        s.apply(batch(&[(b"\x00a", 1)]), PersistMode::Buffer),
        Err(StoreError::poisoned())
    );
    assert_eq!(s.create_temporary().err(), Some(StoreError::poisoned()));
    s.recover();
    assert_eq!(
        s.temporary_snapshot(&t).err(),
        Some(StoreError::unknown_temporary(t.id())),
        "recovery drops every temporary keyspace"
    );
    Ok(())
}

#[test]
fn test_a_crash_during_a_temporary_write_takes_the_process_down() -> TestResult {
    let s = store();
    s.apply(batch(&[(b"\x00a", 1)]), PersistMode::Buffer)?;
    let t = s.create_temporary()?;
    s.inject(Fault::CrashDuringNextBatch);
    assert_eq!(
        s.apply_temporary(&t, batch(&[(b"\x00k", 1)])),
        Err(StoreError::write_failed(
            "simulated process crash while a temporary batch was written".into()
        ))
    );
    assert_eq!(s.temporary_snapshot(&t).err(), Some(StoreError::poisoned()));
    assert!(s.take_log().contains(&OpRecord::TemporaryFailed {
        temporary: t.id(),
        batch: batch(&[(b"\x00k", 1)]),
        crashed: true
    }));
    s.recover();
    assert_eq!(
        s.contents().versions(b"\x00a").len(),
        1,
        "the data keeps what was handed to the system"
    );
    Ok(())
}

#[test]
fn test_a_crash_or_a_power_loss_takes_every_temporary_keyspace() -> TestResult {
    for power_loss in [false, true] {
        let s = store();
        let t = s.create_temporary()?;
        s.apply_temporary(&t, batch(&[(b"\x00k", 1)]))?;
        s.sync()?;
        if power_loss {
            s.power_loss();
        } else {
            s.crash();
        }
        assert_eq!(s.temporary_snapshot(&t).err(), Some(StoreError::poisoned()));
        s.recover();
        assert_eq!(
            s.apply_temporary(&t, batch(&[(b"\x00k", 2)])),
            Err(StoreError::unknown_temporary(t.id()))
        );
    }
    Ok(())
}

#[test]
fn test_reads_of_a_temporary_keyspace_fail_as_forced() -> TestResult {
    let s = store();
    let t = s.create_temporary()?;
    s.apply_temporary(&t, batch(&[(b"\x00k", 1)]))?;
    let snapshot = s.temporary_snapshot(&t)?;
    s.inject(Fault::FailNextRead);
    assert_eq!(
        snapshot.get(b"\x00k"),
        Err(StoreError::read_failed(
            b"\x00k".to_vec(),
            "simulated read failure".into()
        ))
    );
    s.inject(Fault::FailNextScanAfter(0));
    let items: Vec<_> = snapshot.scan(&Scan::prefix(Vec::new())).collect();
    assert_eq!(
        items,
        [Err(StoreError::read_failed(
            b"\x00k".to_vec(),
            "simulated read failure partway through a scan".into()
        ))]
    );
    Ok(())
}

#[test]
fn test_temporary_operations_are_logged() -> TestResult {
    let s = store();
    let t = s.create_temporary()?;
    s.apply_temporary(&t, batch(&[(b"\x00k", 1)]))?;
    drop(s.temporary_snapshot(&t)?);
    let id = t.id();
    s.drop_temporary(t)?;
    assert_eq!(
        s.take_log(),
        [
            OpRecord::TemporaryCreated { temporary: id },
            OpRecord::TemporaryApplied {
                temporary: id,
                batch: batch(&[(b"\x00k", 1)])
            },
            OpRecord::TemporarySnapshotOpened {
                id: 0,
                temporary: id
            },
            OpRecord::SnapshotClosed { id: 0 },
            OpRecord::TemporaryDropped { temporary: id },
        ]
    );
    Ok(())
}
