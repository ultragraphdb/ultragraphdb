/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The store's layout in fjall and its durability modes.

use std::time::Duration;

use fjall::{Database, KeyspaceCreateOptions, Readable};
use ultragraphdb_store_kv::{DATA_KEYSPACE, Durability, FjallStore, META_KEYSPACE};
use ultragraphdb_traits_store::{Entry, PersistMode, Store, WriteBatch};

use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_tool_test::stored;
use ultragraphdb_tool_test::{TestDir, put, write};

#[test]
fn test_versions_are_part_of_the_stored_key_and_the_metadata_a_value_prefix() -> TestResult {
    let dir = TestDir::new("layout");
    let s = dir.open()?;
    let mut batch = WriteBatch::new();
    batch.push(Entry::put(b"a\x00b".to_vec(), 3, 0x08, b"v".to_vec()).with_discard());
    batch.push(Entry::deletion(b"a\x00b".to_vec(), 5));
    batch.set_applied_ts(5);
    s.apply(batch, PersistMode::Sync)?;
    drop(s);
    let db = Database::builder(dir.path()).open()?;
    let data = db.keyspace(DATA_KEYSPACE, KeyspaceCreateOptions::default)?;
    let meta = db.keyspace(META_KEYSPACE, KeyspaceCreateOptions::default)?;
    let snapshot = db.snapshot();
    let raw: Vec<(Vec<u8>, Vec<u8>)> = snapshot
        .iter(&data)
        .map(|g| g.into_inner().map(|(k, v)| (k.to_vec(), v.to_vec())))
        .collect::<Result<_, _>>()?;
    let mut five = b"a\x00\xfeb\x00\x01".to_vec();
    five.extend_from_slice(&(!5u64).to_be_bytes());
    assert_eq!(
        raw,
        vec![
            (five, vec![0x00, 0x02]),
            (stored(b"a\x00b", 3), vec![0x08, 0x01, b'v'])
        ]
    );
    assert_eq!(
        snapshot
            .get(&meta, b"applied_ts")?
            .context("the applied timestamp")?
            .to_vec(),
        5u64.to_be_bytes().to_vec()
    );
    Ok(())
}

/// A stored key is a version only when it ends in a zero byte, an end code and the version: one whose end code follows a
/// byte other than zero is refused as corruption, naming its length, though its end code alone is in range.
#[test]
fn test_a_stored_key_without_its_zero_before_the_end_code_is_corruption() -> TestResult {
    let dir = TestDir::new("unterminated");
    drop(dir.open()?);
    {
        let db = Database::builder(dir.path()).open()?;
        let data = db.keyspace(DATA_KEYSPACE, KeyspaceCreateOptions::default)?;
        // `ab`, then 0x07 where the zero byte belongs and the end code 0x02, then the complemented version 1.
        let mut stored = b"ab\x07\x02".to_vec();
        stored.extend_from_slice(&(!1u64).to_be_bytes());
        data.insert(stored, vec![0x04, 0x00, b'v'])?;
        db.persist(fjall::PersistMode::SyncAll)?;
    }
    let s = dir.open()?;
    let refusal = s.dump().err().context("the dump refuses the key")?;
    assert!(
        refusal
            .to_string()
            .contains("stored key of 12 bytes does not end in a terminated version"),
        "{refusal}"
    );
    Ok(())
}

#[test]
fn test_the_dump_lists_every_version_with_its_flags() -> TestResult {
    let dir = TestDir::new("dump");
    let s = dir.open()?;
    write(
        &s,
        vec![
            put(b"b", 1, b"x"),
            Entry::put(b"a".to_vec(), 2, 0x08, b"y".to_vec()).with_discard(),
        ],
    )?;
    write(&s, vec![Entry::deletion(b"b".to_vec(), 4)])?;
    assert_eq!(
        s.dump()?,
        vec![
            Entry::put(b"a".to_vec(), 2, 0x08, b"y".to_vec()).with_discard(),
            Entry::deletion(b"b".to_vec(), 4),
            put(b"b", 1, b"x"),
        ]
    );
    assert_eq!(
        format!("{s:?}"),
        "FjallStore { durability: Process, applied_ts: 0, poisoned: false, view: View { banned: {}, readers: {}, \
         discard_ts: 0, horizon: None }, .. }"
    );
    Ok(())
}

#[test]
fn test_the_filesystem_mode_syncs_every_batch_and_the_process_mode_on_its_period() -> TestResult {
    for (durability, period) in [
        (Durability::Filesystem, Duration::from_secs(60)),
        (Durability::Process, Duration::from_millis(5)),
    ] {
        let dir = TestDir::new("durability");
        let s = FjallStore::open_with_sync_period(dir.path(), durability, period)?;
        write(&s, vec![put(b"k", 1, b"")])?;
        std::thread::sleep(Duration::from_millis(50));
        assert!(!s.is_poisoned());
        drop(s);
        assert_eq!(
            dir.open()?.get_at(b"k", 1)?.map(|i| i.version()),
            Some(1),
            "{durability:?}"
        );
    }
    Ok(())
}

/// Whether a journal file of the store in `dir`, fjall's `*.jnl` at most two directories down, holds `needle` on
/// disk. fjall preallocates its journal, so its length says nothing; its bytes do.
fn journal_holds(dir: &std::path::Path, needle: &[u8]) -> TestResult<bool> {
    let mut level = vec![dir.to_path_buf()];
    for _ in 0..3 {
        let mut below = Vec::new();
        for folder in level {
            for entry in std::fs::read_dir(&folder)? {
                let path = entry?.path();
                if path.is_dir() {
                    below.push(path);
                } else if path.extension().is_some_and(|e| e == "jnl")
                    && std::fs::read(&path)?
                        .windows(needle.len())
                        .any(|window| window == needle)
                {
                    return Ok(true);
                }
            }
        }
        level = below;
    }
    Ok(false)
}

/// fjall's journal persisted manually: in the log mode a batch written to be buffered stays in fjall's journal buffer,
/// handed to the operating system by nothing but a sync, while the process mode hands each batch over as it is
/// written. A batch of one small key fits the 8 KiB buffer whole, and its value is written uncompressed.
#[test]
fn test_the_log_mode_hands_the_journal_over_only_at_a_sync() -> TestResult {
    let value = b"a value the journal holds once it is handed over";
    for (durability, handed_over) in [(Durability::Log, false), (Durability::Process, true)] {
        let dir = TestDir::new("journal");
        let s = FjallStore::open(dir.path(), durability)?;
        write(&s, vec![put(b"k", 1, value)])?;
        assert_eq!(
            journal_holds(dir.path(), value)?,
            handed_over,
            "{durability:?}"
        );
        s.sync()?;
        assert!(
            journal_holds(dir.path(), value)?,
            "{durability:?}: a sync hands the batch over"
        );
    }
    Ok(())
}

/// In the log mode the store is synced only when asked: batches synced by a checkpoint's sync, or written to be
/// synced, are held after a reopen with the applied position of the last.
#[test]
fn test_the_log_mode_holds_what_a_sync_covered() -> TestResult {
    let dir = TestDir::new("log-mode");
    let s = FjallStore::open(dir.path(), Durability::Log)?;
    let mut first = WriteBatch::new();
    first.push(put(b"a", 1, b"x"));
    first.set_applied_ts(1);
    s.apply(first, PersistMode::Buffer)?;
    s.sync()?;
    let mut second = WriteBatch::new();
    second.push(put(b"b", 2, b"y"));
    second.set_applied_ts(2);
    s.apply(second, PersistMode::Sync)?;
    assert!(!s.is_poisoned());
    drop(s);
    let reopened = FjallStore::open(dir.path(), Durability::Log)?;
    assert_eq!(reopened.get_at(b"a", 1)?.map(|i| i.version()), Some(1));
    assert_eq!(reopened.get_at(b"b", 2)?.map(|i| i.version()), Some(2));
    assert_eq!(reopened.applied_ts(), 2);
    Ok(())
}

#[test]
fn test_the_write_pressure_fjall_stalls_on_is_read_from_the_data_keyspace() -> TestResult {
    let dir = TestDir::new("pressure");
    let s = dir.open()?;
    assert_eq!((s.l0_table_count(), s.sealed_memtable_count()), (0, 0));
    write(&s, vec![put(b"a", 3, b"v")])?;
    s.compact()?;
    // A compaction flushes the memtable and moves every table out of level 0.
    assert_eq!((s.l0_table_count(), s.sealed_memtable_count()), (0, 0));
    assert!(s.data_path().starts_with(dir.path()));
    Ok(())
}
