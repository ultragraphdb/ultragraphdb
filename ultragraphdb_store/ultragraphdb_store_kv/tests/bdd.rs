/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The kv-store scenarios, on the fjall store.

// The scenarios use the harness's batch and drop kills; the crash test alone reads temporary keyspaces raw and
// names every stage.
#[allow(dead_code)]
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use cucumber::{World, then, when};
use fjall::compaction::filter::Verdict;
use fjall::{Database, KeyspaceCreateOptions};
use ultragraphdb_error_store::StoreError;
use ultragraphdb_store_kv::{DATA_KEYSPACE, Durability, FjallStore, GcFilter};
use ultragraphdb_traits_store::{Entry, Item, PersistMode, Scan, Store, StoreSnapshot, WriteBatch};

use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_tool_test::{
    DROPPED_KEYS, KEYS_PER_BATCH, batch_counts, drop_outcome, fresh_dir, kill_after, kill_at, open,
    raw_drop_state, versionless_counts,
};

#[derive(Debug, Default, World)]
struct Steps {
    dir: Option<PathBuf>,
    store: Option<FjallStore>,
    items: Vec<Item>,
    read: Option<Result<Option<Item>, StoreError>>,
    error: Option<StoreError>,
    reader_results: Vec<(usize, usize)>,
    outcomes: Vec<String>,
    verdict: Option<&'static str>,
    versionless: Vec<Option<Vec<u8>>>,
}

impl Drop for Steps {
    fn drop(&mut self) {
        self.store = None;
        if let Some(dir) = &self.dir {
            // A directory left behind is litter in the temporary directory, not a failure of the test.
            drop(std::fs::remove_dir_all(dir));
        }
    }
}

impl Steps {
    fn open(&mut self) -> TestResult<&FjallStore> {
        if self.dir.is_none() {
            self.dir = Some(fresh_dir(&format!("bdd-{}", next())));
        }
        if self.store.is_none() {
            let dir = self.dir.clone().unwrap_or_default();
            self.store = Some(FjallStore::open(dir, Durability::Process).context("opens")?);
        }
        self.store.as_ref().context("open")
    }

    fn reopen(&mut self) -> TestResult<&FjallStore> {
        self.store = None;
        self.open()
    }

    fn write(&mut self, entries: Vec<Entry>) -> TestResult {
        let mut batch = WriteBatch::new();
        entries.into_iter().for_each(|e| batch.push(e));
        self.open()?
            .apply(batch, PersistMode::Buffer)
            .context("written")
    }

    fn held(&self, key: &[u8]) -> TestResult<Vec<(u64, bool)>> {
        Ok(self
            .store
            .as_ref()
            .context("open")?
            .dump()
            .context("dumps")?
            .iter()
            .filter(|e| e.key() == key)
            .map(|e| (e.version(), e.is_deletion()))
            .collect())
    }
}

fn next() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    N.fetch_add(1, Ordering::Relaxed)
}

fn put(key: &[u8], version: u64, value: &[u8]) -> Entry {
    Entry::put(key.to_vec(), version, 0x04, value.to_vec())
}

/// The forward list of subject 1 and predicate 7 in namespace `ns`, in the quad layout: the keyspace byte, then the
/// namespace, graph, predicate and subject, each a u64 big-endian.
fn ns_key(ns: u64) -> Vec<u8> {
    let mut key = vec![0x10];
    [ns, 0, 7, 1]
        .iter()
        .for_each(|n| key.extend_from_slice(&n.to_be_bytes()));
    key
}

/// A value index key of the quad layout for `term`: predicate 7's prefix and the tokenizer byte, the term appended
/// raw.
fn index_key(term: &[u8]) -> Vec<u8> {
    let mut key = vec![0x14];
    [0u64, 0, 7]
        .iter()
        .for_each(|n| key.extend_from_slice(&n.to_be_bytes()));
    key.push(0x02);
    key.extend_from_slice(term);
    key
}

fn versions_of(snapshot: &impl StoreSnapshot, key: &[u8]) -> TestResult<Vec<Item>> {
    snapshot.versions(key).map(|i| i.context("read")).collect()
}

#[when("a key is written at versions 3, 7 and 5")]
fn three_versions(w: &mut Steps) -> TestResult {
    for v in [3, 7, 5] {
        w.write(vec![Entry::put(b"k".to_vec(), v, 0x04 + v as u8, vec![])])?;
    }
    Ok(())
}

#[then("iterating its versions yields 7, 5 and 3, each with its meta byte")]
fn newest_first(w: &mut Steps) -> TestResult {
    let items = versions_of(&w.open()?.snapshot(10)?, b"k")?;
    let got: Vec<(u64, u8)> = items.iter().map(|i| (i.version(), i.meta())).collect();
    assert_eq!(got, vec![(7, 11), (5, 9), (3, 7)]);
    Ok(())
}

#[when("a key holds version 9 and a write at version 6 follows")]
fn below_newest(w: &mut Steps) -> TestResult {
    w.write(vec![put(b"k", 9, b"a")])?;
    w.write(vec![put(b"k", 6, b"b")])?;
    Ok(())
}

#[then("both versions are kept, and the key's versions iterate as 9 then 6")]
fn both_kept(w: &mut Steps) -> TestResult {
    let items = versions_of(&w.open()?.snapshot(10)?, b"k")?;
    assert_eq!(
        items.iter().map(Item::version).collect::<Vec<_>>(),
        vec![9, 6]
    );
    Ok(())
}

#[when("the same key and version are written twice with different values")]
fn twice(w: &mut Steps) -> TestResult {
    w.write(vec![put(b"k", 4, b"first")])?;
    w.write(vec![put(b"k", 4, b"second")])?;
    Ok(())
}

#[then("a read at that version returns the second value")]
fn second_value(w: &mut Steps) -> TestResult {
    assert_eq!(
        w.open()?
            .get_at(b"k", 4)?
            .context("the key\'s item")?
            .value(),
        b"second"
    );
    assert_eq!(
        w.reopen()?
            .get_at(b"k", 4)?
            .context("the key\'s item")?
            .value(),
        b"second"
    );
    Ok(())
}

#[when("index keys for the terms ab and abc are each written at versions 3 and 5")]
fn prefix_terms(w: &mut Steps) -> TestResult {
    for v in [3, 5] {
        w.write(vec![
            put(&index_key(b"abc"), v, b""),
            put(&index_key(b"ab"), v, b""),
        ])?;
    }
    Ok(())
}

#[then("a scan yields ab before abc, each key's versions contiguous and newest first")]
fn ab_first(w: &mut Steps) -> TestResult {
    let snapshot = w.open()?.snapshot(9)?;
    let got: Vec<(Vec<u8>, u64)> = snapshot
        .scan(&Scan::prefix(index_key(b"a")).all_versions())
        .map(|i| i.map(|i| (i.key().to_vec(), i.version())))
        .collect::<Result<_, _>>()?;
    let (ab, abc) = (index_key(b"ab"), index_key(b"abc"));
    assert_eq!(
        got,
        vec![(ab.clone(), 5), (ab, 3), (abc.clone(), 5), (abc, 3)]
    );
    Ok(())
}

fn oversized(w: &mut Steps, key: &[u8]) -> TestResult {
    let mut batch = WriteBatch::new();
    batch.push(put(b"fine", 1, b""));
    batch.push(put(key, 1, b""));
    w.error = w.open()?.apply(batch, PersistMode::Buffer).err();
    Ok(())
}

#[when("a batch writes a key whose user key is 65,001 bytes long")]
fn user_key_too_long(w: &mut Steps) -> TestResult {
    oversized(w, &[1; 65_001])
}

#[when(
    "a batch writes a key whose user key is 44,000 bytes of 0x00 0x01, 66,010 bytes once encoded and versioned"
)]
fn stored_key_too_long(w: &mut Steps) -> TestResult {
    let key: Vec<u8> = (0..44_000).map(|i| (i % 2) as u8).collect();
    oversized(w, &key)
}

#[then(
    regex = r#"^the batch fails with an error beginning "(.+)", and none of its keys is found$"#
)]
fn refused(w: &mut Steps, args: &[String]) -> TestResult {
    let text = &args[0];
    let error = w.error.take().context("the batch failed")?.to_string();
    assert!(error.starts_with(text.as_str()), "{error}");
    assert_eq!(w.open()?.get_at(b"fine", 1)?, None);
    assert!(!w.open()?.is_poisoned());
    Ok(())
}

#[when("a key holds versions 3 and 7 and is read at 5")]
fn read_at_five(w: &mut Steps) -> TestResult {
    w.write(vec![put(b"k", 3, b""), put(b"k", 7, b"")])?;
    w.read = Some(w.open()?.get_at(b"k", 5));
    Ok(())
}

#[then("the read returns version 3")]
fn version_three(w: &mut Steps) -> TestResult {
    let read = w
        .read
        .take()
        .context("the read's result")??
        .context("the read item")?;
    assert_eq!(read.version(), 3);
    Ok(())
}

#[when("a key holds a value at 3 and a deletion at 6")]
fn value_and_deletion(w: &mut Steps) -> TestResult {
    w.write(vec![put(b"k", 3, b"v"), Entry::deletion(b"k".to_vec(), 6)])?;
    Ok(())
}

#[then("a read at 7 finds nothing and a read at 4 finds the value")]
fn deletion_hides(w: &mut Steps) -> TestResult {
    assert_eq!(w.open()?.get_at(b"k", 7)?, None);
    assert_eq!(
        w.open()?
            .get_at(b"k", 4)?
            .context("the key\'s item")?
            .value(),
        b"v"
    );
    Ok(())
}

#[when("the discard timestamp is 10 and a key is read at 5")]
fn too_old(w: &mut Steps) -> TestResult {
    w.write(vec![put(b"k", 3, b"v")])?;
    w.open()?.set_discard_ts(10)?;
    w.read = Some(w.open()?.get_at(b"k", 5));
    Ok(())
}

#[then(regex = r#"^the read fails with "(.+)"$"#)]
fn read_fails(w: &mut Steps, args: &[String]) -> TestResult {
    let text = &args[0];
    assert_eq!(
        w.read
            .take()
            .context("the read's result")?
            .err()
            .context("the expected error")?
            .to_string(),
        *text
    );
    Ok(())
}

#[when("a scan is open and a batch writes keys inside its range")]
fn scan_then_batch(w: &mut Steps) -> TestResult {
    w.write(vec![put(b"p1", 1, b"")])?;
    let snapshot = w.open()?.snapshot(100)?;
    let mut scan = snapshot.scan(&Scan::prefix(b"p".to_vec()));
    w.write(vec![
        put(b"p0", 2, b""),
        put(b"p2", 2, b""),
        put(b"p1", 3, b""),
    ])?;
    let mut items: Vec<Item> = scan.by_ref().map(Result::unwrap).collect();
    items.extend(
        snapshot
            .scan(&Scan::prefix(b"p".to_vec()))
            .map(Result::unwrap),
    );
    w.items = items;
    Ok(())
}

#[then("the scan returns none of the batch's keys")]
fn none_of_the_batch(w: &mut Steps) {
    assert_eq!(w.items.len(), 2);
    assert!(
        w.items
            .iter()
            .all(|i| (i.key(), i.version()) == (&b"p1"[..], 1))
    );
}

#[when(
    "three keys under a prefix hold several versions each and the prefix is scanned at a timestamp"
)]
fn three_keys(w: &mut Steps) -> TestResult {
    for v in 1..=4 {
        w.write(vec![
            put(b"pa", v, b""),
            put(b"pb", v * 2, b""),
            put(b"pc", v + 1, b""),
        ])?;
    }
    let snapshot = w.open()?.snapshot(5)?;
    w.items = snapshot
        .scan(&Scan::prefix(b"p".to_vec()))
        .map(Result::unwrap)
        .collect();
    Ok(())
}

#[then("each key appears once, with its newest version at or below that timestamp")]
fn each_once(w: &mut Steps) {
    let got: Vec<(&[u8], u64)> = w.items.iter().map(|i| (i.key(), i.version())).collect();
    assert_eq!(got, vec![(&b"pa"[..], 4), (&b"pb"[..], 4), (&b"pc"[..], 5)]);
}

#[when("the same prefix is scanned for all versions")]
fn all_versions(w: &mut Steps) -> TestResult {
    three_keys(w)?;
    let snapshot = w.open()?.snapshot(5)?;
    w.items = snapshot
        .scan(&Scan::prefix(b"p".to_vec()).all_versions())
        .map(Result::unwrap)
        .collect();
    Ok(())
}

#[then("each key's versions at or below the timestamp appear newest first")]
fn every_version(w: &mut Steps) {
    let got: Vec<(&[u8], u64)> = w.items.iter().map(|i| (i.key(), i.version())).collect();
    let expected: Vec<(&[u8], u64)> = vec![
        (b"pa", 4),
        (b"pa", 3),
        (b"pa", 2),
        (b"pa", 1),
        (b"pb", 4),
        (b"pb", 2),
        (b"pc", 5),
        (b"pc", 4),
        (b"pc", 3),
        (b"pc", 2),
    ];
    assert_eq!(got, expected);
}

#[when("a batch writing 10,000 keys commits while readers read those keys repeatedly")]
#[allow(
    clippy::disallowed_methods,
    reason = "the readers run on threads beside the writer, as the production store's readers do"
)]
fn readers_during_batch(w: &mut Steps) -> TestResult {
    let keys: Vec<Vec<u8>> = (0..10_000u32).map(|i| i.to_be_bytes().to_vec()).collect();
    let _store: &FjallStore = w.open()?;
    let store = Arc::new(w.store.take().context("open")?);
    w.reader_results = std::thread::scope(|scope| -> TestResult<Vec<(usize, usize)>> {
        let readers: Vec<_> = (0..4)
            .map(|_| {
                let (store, keys) = (store.clone(), keys.clone());
                scope.spawn(move || {
                    (0..40)
                        .map(|_| {
                            let snapshot = store.snapshot(10)?;
                            let found = snapshot.get_many(&keys)?.iter().flatten().count();
                            Ok((found, snapshot.scan(&Scan::prefix(Vec::new())).count()))
                        })
                        .collect::<Result<Vec<_>, StoreError>>()
                })
            })
            .collect();
        let mut batch = WriteBatch::new();
        keys.iter().for_each(|k| batch.push(put(k, 3, b"")));
        store.apply(batch, PersistMode::Buffer)?;
        let mut results = Vec::new();
        for reader in readers {
            results.extend(
                reader
                    .join()
                    .map_err(|_| TestError::new("a reader thread panicked"))??,
            );
        }
        Ok(results)
    })?;
    w.store = Arc::into_inner(store);
    Ok(())
}

#[then("every reader sees either none or all of the batch's keys")]
fn whole_or_none(w: &mut Steps) {
    assert!(!w.reader_results.is_empty());
    for (found, scanned) in &w.reader_results {
        assert!(*found == 0 || *found == 10_000, "a reader saw {found}");
        assert_eq!(found, scanned);
    }
}

#[when("the process is killed while a batch is written and the store is reopened")]
fn killed_during_batch(w: &mut Steps) -> TestResult {
    for delay in [0u64, 2, 5] {
        let dir = fresh_dir(&format!("bdd-batch-{}", next()));
        let lines = kill_after(&dir, "batches", "batch 2", Duration::from_millis(delay))?;
        let started = lines.iter().filter(|l| l.starts_with("batch ")).count() as u64;
        let store = open(&dir)?;
        let counts = batch_counts(&store, started + 1)?;
        w.outcomes
            .push(format!("{counts:?} applied {}", store.applied_ts()));
        drop(store);
        // A directory left behind is litter in the temporary directory, not a failure of the test.
        drop(std::fs::remove_dir_all(&dir));
    }
    Ok(())
}

#[then("either the whole batch or none of it is present")]
fn whole_batch(w: &mut Steps) -> TestResult {
    for outcome in &w.outcomes {
        let counts: Vec<usize> = outcome
            .trim_start_matches('[')
            .split(']')
            .next()
            .unwrap_or_default()
            .split(", ")
            .map(str::parse)
            .collect::<Result<_, _>>()?;
        assert!(
            counts.iter().all(|&c| c == 0 || c == KEYS_PER_BATCH),
            "{outcome}"
        );
    }
    Ok(())
}

#[when(
    "a batch writes a versioned key and a key without a version, and the process crashes during the batch"
)]
fn killed_during_mixed_batch(w: &mut Steps) -> TestResult {
    for delay in [0u64, 2, 5] {
        let dir = fresh_dir(&format!("bdd-mixed-{}", next()));
        let lines = kill_after(&dir, "mixed", "batch 2", Duration::from_millis(delay))?;
        let started = lines.iter().filter(|l| l.starts_with("batch ")).count() as u64;
        let store = open(&dir)?;
        let versioned = batch_counts(&store, started + 1)?;
        let versionless = versionless_counts(&store, started + 1)?;
        w.outcomes.push(format!("{versioned:?} {versionless:?}"));
        drop(store);
        // A directory left behind is litter in the temporary directory, not a failure of the test.
        drop(std::fs::remove_dir_all(&dir));
    }
    Ok(())
}

#[then("after reopening, either both are found or neither is")]
fn both_or_neither(w: &mut Steps) -> TestResult {
    assert_eq!(w.outcomes.len(), 3);
    for outcome in &w.outcomes {
        let lists: Vec<Vec<usize>> = outcome
            .split("] [")
            .map(|l| {
                l.trim_matches(['[', ']'])
                    .split(", ")
                    .map(str::parse)
                    .collect::<Result<Vec<usize>, _>>()
            })
            .collect::<Result<_, _>>()?;
        assert_eq!(lists[0], lists[1], "{outcome}");
        assert!(
            lists[0].iter().all(|&c| c == 0 || c == KEYS_PER_BATCH),
            "{outcome}"
        );
        // Batch 1 was applied before batch 2 was announced, so both its halves are found.
        assert_eq!(lists[0].first(), Some(&KEYS_PER_BATCH), "{outcome}");
    }
    Ok(())
}

#[when("a key that was never written is read from the keyspace without versions")]
fn read_absent_versionless(w: &mut Steps) -> TestResult {
    let mut batch = WriteBatch::new();
    (0u32..1000).for_each(|i| batch.put_versionless(format!("t{i}").into_bytes(), vec![1]));
    let store = w.open()?;
    store.apply(batch, PersistMode::Buffer).context("written")?;
    // Flushed into a table, so the read below is answered by the table's bloom filter, not the memtable.
    store.compact().context("compacts")?;
    w.versionless = vec![
        store.get_versionless(b"never").context("read")?,
        store
            .snapshot(1)
            .context("opens")?
            .get_versionless(b"never")
            .context("read")?,
        store.get_versionless(b"t7").context("read")?,
    ];
    Ok(())
}

#[then("the read returns nothing, through one point lookup")]
fn absent_read_nothing(w: &mut Steps) {
    // `get_versionless` is one `Keyspace::get`, and a snapshot's one `Snapshot::get`; neither seeks a range.
    assert_eq!(w.versionless, vec![None, None, Some(vec![1])]);
}

#[when("keys without versions are written and everything is dropped")]
fn versionless_dropped(w: &mut Steps) -> TestResult {
    let mut batch = WriteBatch::new();
    batch.put_versionless(b"t1".to_vec(), vec![1]);
    batch.put_versionless(b"t2".to_vec(), vec![2]);
    batch.push(put(b"k", 3, b"v"));
    let store = w.open()?;
    store.apply(batch, PersistMode::Buffer).context("written")?;
    store.drop_all(Vec::new()).context("dropped")?;
    let _reopened: &FjallStore = w.reopen()?;
    Ok(())
}

#[then("none of them is found")]
fn none_found(w: &mut Steps) -> TestResult {
    let store = w.store.as_ref().context("open")?;
    assert_eq!(store.get_versionless(b"t1").context("read")?, None);
    assert_eq!(store.get_versionless(b"t2").context("read")?, None);
    assert!(store.dump_versionless().context("dumps")?.is_empty());
    Ok(())
}

#[when("a key without a version is read, then rewritten with another value, then removed")]
fn versionless_rewritten(w: &mut Steps) -> TestResult {
    let write = |steps: &mut Steps, value: Option<&[u8]>| -> TestResult {
        let mut batch = WriteBatch::new();
        match value {
            Some(v) => batch.put_versionless(b"t".to_vec(), v.to_vec()),
            None => batch.remove_versionless(b"t".to_vec()),
        }
        steps
            .open()?
            .apply(batch, PersistMode::Buffer)
            .context("written")
    };
    write(w, Some(b"old"))?;
    let first = w.open()?.get_versionless(b"t").context("read")?;
    write(w, Some(b"new"))?;
    let rewritten = w.open()?.get_versionless(b"t").context("read")?;
    write(w, None)?;
    let removed = w.open()?.get_versionless(b"t").context("read")?;
    w.versionless = vec![first, rewritten, removed];
    Ok(())
}

#[then(
    "the read after the rewrite returns the new value and the read after the removal returns nothing"
)]
fn versionless_gave_way(w: &mut Steps) {
    assert_eq!(
        w.versionless,
        vec![Some(b"old".to_vec()), Some(b"new".to_vec()), None]
    );
}

#[when("one batch writes key a at version 12 and key b at version 13")]
fn two_versions_one_batch(w: &mut Steps) -> TestResult {
    w.write(vec![put(b"a", 12, b""), put(b"b", 13, b"")])?;
    Ok(())
}

#[then("a read at 12 finds a and not b, and a read at 13 finds both")]
fn at_twelve_and_thirteen(w: &mut Steps) -> TestResult {
    let at12 = w.open()?.snapshot(12)?.get_many(&[b"a", b"b"])?;
    assert_eq!(
        at12.iter().map(Option::is_some).collect::<Vec<_>>(),
        vec![true, false]
    );
    let at13 = w.open()?.snapshot(13)?.get_many(&[b"a", b"b"])?;
    assert!(at13.iter().all(Option::is_some));
    Ok(())
}

#[when("a prefix is dropped and the store is closed and reopened")]
fn drop_and_reopen(w: &mut Steps) -> TestResult {
    w.write(vec![
        put(b"\x00a1", 3, b""),
        put(b"\x00a\x002", 4, b""),
        put(b"\x00b1", 3, b""),
    ])?;
    w.open()?
        .drop_prefixes(8, &[b"\x00a".to_vec()], Vec::new())?;
    let _reopened: &FjallStore = w.reopen()?;
    Ok(())
}

#[then("no key under the prefix is found")]
fn prefix_gone(w: &mut Steps) -> TestResult {
    let snapshot = w.open()?.snapshot(20)?;
    assert_eq!(
        snapshot
            .scan(&Scan::prefix(b"\x00a".to_vec()).all_versions())
            .count(),
        0
    );
    assert!(snapshot.get(b"\x00b1")?.is_some());
    Ok(())
}

#[when(
    "a prefix drop has synced its record and written part of its removals, and the process is killed"
)]
fn killed_during_drop(w: &mut Steps) -> TestResult {
    // The writer stops after a batch of removals, its record synced, and is killed there.
    for stage in ["removing-10000", "removing-30000", "removing-50000"] {
        let dir = fresh_dir(&format!("bdd-drop-{}", next()));
        let _lines: Vec<String> = kill_at(&dir, "drop", stage)?;
        let (record, left) = raw_drop_state(&dir)?;
        let store = open(&dir)?;
        let (under, entry, kept) = drop_outcome(&store)?;
        w.outcomes.push(format!(
            "{record} {} {under} {entry} {kept}",
            left > 0 && left < DROPPED_KEYS
        ));
        drop(store);
        // A directory left behind is litter in the temporary directory, not a failure of the test.
        drop(std::fs::remove_dir_all(&dir));
    }
    Ok(())
}

#[then(
    "on reopening, the store finishes the drop before it serves a read, and no key under the prefix is found"
)]
fn drop_finished(w: &mut Steps) -> TestResult {
    assert!(
        w.outcomes.iter().all(|o| o.starts_with("true true")),
        "a kill landed outside the removals: {:?}",
        w.outcomes
    );
    for outcome in &w.outcomes {
        if outcome.starts_with("true") {
            // The entry the drop writes once its prefix is empty is the one key under it.
            assert!(outcome.ends_with(" 1 true true"), "{outcome}");
        }
    }
    Ok(())
}

#[when(
    "a key under a prefix holds a version at 3, the prefix is dropped at 8, and the key is read at 5"
)]
fn read_below_drop(w: &mut Steps) -> TestResult {
    w.write(vec![put(b"\x00a1", 3, b"")])?;
    w.open()?
        .drop_prefixes(8, &[b"\x00a".to_vec()], Vec::new())?;
    w.read = Some(w.open()?.get_at(b"\x00a1", 5));
    Ok(())
}

#[then("the read finds nothing")]
fn finds_nothing(w: &mut Steps) -> TestResult {
    assert_eq!(w.read.take().context("the read's result")??, None);
    Ok(())
}

#[when(
    "everything is dropped with one entry to write at timestamp 1, and the store is closed and reopened"
)]
fn drop_everything(w: &mut Steps) -> TestResult {
    let mut batch = WriteBatch::new();
    batch.push(put(b"old", 3, b""));
    batch.set_applied_ts(3);
    w.open()?.apply(batch, PersistMode::Buffer)?;
    w.open()?.set_discard_ts(2)?;
    w.open()?.drop_all(vec![put(b"\x01schema", 1, b"s")])?;
    let _reopened: &FjallStore = w.reopen()?;
    Ok(())
}

#[then(
    "that entry is the only key found, and the applied timestamp and the discard timestamp are 0"
)]
fn only_the_entry(w: &mut Steps) -> TestResult {
    let s = w.open()?;
    let keys: Vec<Vec<u8>> = s.dump()?.iter().map(|e| e.key().to_vec()).collect();
    assert_eq!(keys, vec![b"\x01schema".to_vec()]);
    assert_eq!((s.applied_ts(), s.discard_ts()), (0, 0));
    Ok(())
}

#[when("namespace 5 is banned and a batch writes a key in it and a key in namespace 6")]
fn banned_batch(w: &mut Steps) -> TestResult {
    w.open()?.ban_namespace(5)?;
    let mut batch = WriteBatch::new();
    batch.push(put(&ns_key(5), 2, b""));
    batch.push(put(&ns_key(6), 2, b""));
    w.error = w.open()?.apply(batch, PersistMode::Buffer).err();
    Ok(())
}

#[then("the batch succeeds, the key in namespace 5 is not found and the key in namespace 6 is")]
fn ban_holds(w: &mut Steps) -> TestResult {
    assert!(w.error.is_none());
    assert_eq!(w.open()?.get_at(&ns_key(5), 3)?, None);
    assert_eq!(
        w.open()?.get_at(&ns_key(6), 3)?.map(|i| i.version()),
        Some(2)
    );
    assert!(
        w.held(&ns_key(5))?.is_empty(),
        "the write was dropped, not hidden"
    );
    Ok(())
}

#[when(
    "namespace 5 is banned, everything is dropped, and a batch then writes a key in namespace 5"
)]
fn ban_then_drop_all(w: &mut Steps) -> TestResult {
    w.open()?.ban_namespace(5)?;
    w.open()?.drop_all(Vec::new())?;
    w.write(vec![put(&ns_key(5), 1, b"")])?;
    Ok(())
}

#[then("the key is found")]
fn key_found(w: &mut Steps) -> TestResult {
    assert_eq!(
        w.open()?.get_at(&ns_key(5), 1)?.map(|i| i.version()),
        Some(1)
    );
    Ok(())
}

/// The catalog key of predicate 7's declaration in namespace 3, in the quad layout: the keyspace byte, the
/// namespace, the entry kind and the predicate.
fn catalog_key_ns3() -> Vec<u8> {
    let mut key = vec![0x01];
    key.extend_from_slice(&3u64.to_be_bytes());
    key.push(0x01);
    key.extend_from_slice(&7u64.to_be_bytes());
    key
}

#[when(
    "everything in a namespace is dropped, its initial catalog is written at timestamp 1, the discard timestamp is raised above the drop and the store compacts"
)]
fn namespace_drop_and_compact(w: &mut Steps) -> TestResult {
    let catalog = catalog_key_ns3();
    w.write(vec![
        Entry::put(catalog.clone(), 5, 0x01, b"old".to_vec()).with_discard(),
    ])?;
    // A reader opened before the drop, at the timestamp the discard timestamp is raised to: it keeps fjall's
    // garbage-collection watermark below the drop's removals, so the removed version at 5 reaches the compaction.
    let reader = w.open()?.snapshot(9)?;
    w.open()?.compact()?;
    let prefixes: Vec<Vec<u8>> = [0x01u8, 0x10, 0x11, 0x14]
        .iter()
        .map(|p| [vec![*p], 3u64.to_be_bytes().to_vec()].concat())
        .collect();
    w.open()?.drop_prefixes(
        7,
        &prefixes,
        vec![Entry::put(catalog, 1, 0x01, b"initial".to_vec()).with_discard()],
    )?;
    w.open()?.set_discard_ts(9)?;
    w.open()?.compact()?;
    drop(reader);
    Ok(())
}

#[then("the namespace's initial catalog is still read at the discard timestamp")]
fn initial_catalog_read(w: &mut Steps) -> TestResult {
    let read = w.open()?.get_at(&catalog_key_ns3(), 9)?;
    assert_eq!(read.map(Item::into_value), Some(b"initial".to_vec()));
    let _reopened: &FjallStore = w.reopen()?;
    w.open()?.compact()?;
    let compacted_read = w.open()?.get_at(&catalog_key_ns3(), 9)?;
    assert_eq!(
        compacted_read.map(Item::into_value),
        Some(b"initial".to_vec())
    );
    Ok(())
}

#[when(
    "a version below the discard timestamp is collected, its deletion is swept and the store is closed and reopened"
)]
fn collect_sweep_reopen(w: &mut Steps) -> TestResult {
    w.write(vec![put(b"k", 2, b"v")])?;
    w.write(vec![Entry::deletion(b"k".to_vec(), 4)])?;
    w.open()?.set_discard_ts(6)?;
    w.open()?.compact()?;
    w.open()?.sweep_deletions()?;
    assert!(w.held(b"k")?.is_empty());
    let _reopened: &FjallStore = w.reopen()?;
    Ok(())
}

#[then("a read above the deletion's timestamp finds no version of the key")]
fn stays_collected(w: &mut Steps) -> TestResult {
    assert_eq!(w.open()?.get_at(b"k", 7)?, None);
    assert!(
        w.held(b"k")?.is_empty(),
        "fjall's journal replay brought {:?} back",
        w.held(b"k")?
    );
    Ok(())
}

#[when(
    "a key holds deltas at 2 and 4, a complete version with the discard flag at 5 and a delta at 8, the discard timestamp is 6, and the memtables are flushed and a full compaction runs"
)]
fn discard_point(w: &mut Steps) -> TestResult {
    w.write(vec![put(b"k", 2, b"d2"), put(b"k", 4, b"d4")])?;
    w.write(vec![
        Entry::put(b"k".to_vec(), 5, 0x08, b"c5".to_vec()).with_discard(),
        put(b"k", 8, b"d8"),
    ])?;
    w.open()?.set_discard_ts(6)?;
    w.open()?.compact()?;
    Ok(())
}

#[then("versions 2 and 4 are gone, and versions 5 and 8 remain")]
fn collected(w: &mut Steps) -> TestResult {
    assert_eq!(w.held(b"k")?, vec![(8, false), (5, false)]);
    let _reopened: &FjallStore = w.reopen()?;
    assert_eq!(
        w.held(b"k")?,
        vec![(8, false), (5, false)],
        "fjall's journal replay does not bring them back"
    );
    Ok(())
}

#[when(
    "a key's value at 2 and its deletion at 4 lie in two different last-level tables, the discard timestamp is 6, and a compaction merges the table holding the deletion without the other"
)]
fn deletion_alone(w: &mut Steps) -> TestResult {
    // The value is compacted into a last-level table of its own; the deletion is written after it. lsm-tree offers
    // no way to choose a compaction's tables, so the compaction of the deletion's table alone is its items run
    // through the store's compaction filter, into the last level.
    w.write(vec![put(b"k", 2, b"v")])?;
    w.open()?.compact()?;
    w.write(vec![Entry::deletion(b"k".to_vec(), 4)])?;
    w.open()?.set_discard_ts(6)?;
    let mut stored = b"k\x00\x01".to_vec();
    stored.extend_from_slice(&(!4u64).to_be_bytes());
    let mut filter = GcFilter::new(6, true);
    w.verdict = Some(match filter.verdict(&stored, &[0x00, 0x02]) {
        Verdict::Keep => "keep",
        Verdict::Remove | Verdict::RemoveWeak | Verdict::ReplaceValue(_) | Verdict::Destroy | _ => {
            "collect"
        }
    });
    Ok(())
}

#[then("the deletion remains, and a read at 7 finds nothing")]
fn deletion_remains(w: &mut Steps) -> TestResult {
    assert_eq!(w.verdict, Some("keep"));
    assert_eq!(w.open()?.get_at(b"k", 7)?, None);
    w.open()?.compact()?;
    assert_eq!(w.held(b"k")?, vec![(4, true)]);
    assert_eq!(w.open()?.get_at(b"k", 7)?, None);
    Ok(())
}

fn deletion_case(w: &mut Steps, deleted_at: u64, horizon: Option<u64>) -> TestResult {
    w.write(vec![put(b"k", 2, b"v")])?;
    w.write(vec![Entry::deletion(b"k".to_vec(), deleted_at)])?;
    w.open()?.set_discard_ts(6)?;
    if let Some(h) = horizon {
        w.open()?.set_tombstone_horizon(h)?;
    }
    w.open()?.compact()?;
    w.open()?.sweep_deletions()?;
    Ok(())
}

#[when(
    "a key holds a value at 2 and a deletion at 4, the discard timestamp and the tombstone horizon are 6, and a full compaction and the sweep run"
)]
fn sweep_all(w: &mut Steps) -> TestResult {
    deletion_case(w, 4, Some(6))
}

#[then("no version of the key remains")]
fn nothing_remains(w: &mut Steps) -> TestResult {
    assert!(w.held(b"k")?.is_empty());
    let _reopened: &FjallStore = w.reopen()?;
    assert!(
        w.held(b"k")?.is_empty(),
        "the value the compaction collected does not return at reopening"
    );
    assert_eq!(w.open()?.get_at(b"k", 7)?, None);
    Ok(())
}

#[when(
    "a key holds a value at 2 and a deletion at 5, the discard timestamp is 6 and the tombstone horizon is 4, and a full compaction and the sweep run"
)]
fn sweep_horizon(w: &mut Steps) -> TestResult {
    deletion_case(w, 5, Some(4))
}

#[then("the value at 2 is gone and the deletion at 5 remains")]
fn deletion_kept(w: &mut Steps) -> TestResult {
    assert_eq!(w.held(b"k")?, vec![(5, true)]);
    Ok(())
}

#[when("a stored value too short to hold the version prefix lies in a compaction")]
fn short_value(w: &mut Steps) -> TestResult {
    w.write(vec![
        Entry::put(b"k".to_vec(), 5, 0x08, b"c".to_vec()).with_discard(),
    ])?;
    w.store = None;
    let dir = w.dir.clone().context("open")?;
    {
        let db = Database::builder(&dir).open()?;
        let data = db.keyspace(DATA_KEYSPACE, KeyspaceCreateOptions::default)?;
        let mut stored = b"k\x00\x01".to_vec();
        stored.extend_from_slice(&(!2u64).to_be_bytes());
        data.insert(stored, vec![0x04])?;
        db.persist(fjall::PersistMode::SyncAll)?;
    }
    w.open()?.set_discard_ts(6)?;
    w.open()?.compact()?;
    Ok(())
}

#[then("the compaction completes, the store is not poisoned, and the item remains")]
fn item_remains(w: &mut Steps) -> TestResult {
    let s = w.open()?;
    assert!(!s.is_poisoned());
    assert_eq!(s.undecodable_kept(), 1);
    s.apply(WriteBatch::new(), PersistMode::Buffer)?;
    let versions: Vec<_> = s.snapshot(6)?.versions(b"k").collect();
    assert_eq!(
        versions,
        vec![
            Ok(Item::new(b"k".to_vec(), 5, 0x08, true, b"c".to_vec())),
            Err(StoreError::Corrupted(
                b"k".to_vec(),
                "value too short to hold the version prefix".to_string()
            )),
        ]
    );
    Ok(())
}

#[when("a reader at timestamp 3 is open and a discard timestamp of 6 is requested")]
fn reader_holds(w: &mut Steps) -> TestResult {
    w.write(vec![
        put(b"k", 2, b"v"),
        Entry::put(b"k".to_vec(), 5, 0x08, b"c".to_vec()).with_discard(),
    ])?;
    let reader = w.open()?.snapshot(3)?;
    w.open()?.set_discard_ts(6)?;
    let held = w.open()?.discard_ts().to_string();
    w.outcomes.push(held);
    w.open()?.compact()?;
    w.outcomes
        .push(reader.get(b"k")?.map_or(0, |i| i.version()).to_string());
    drop(reader);
    let released = w.open()?.discard_ts().to_string();
    w.outcomes.push(released);
    Ok(())
}

#[then("the discard timestamp used is at most 3 until the reader closes")]
fn held_back(w: &mut Steps) {
    assert_eq!(w.outcomes, vec!["3", "2", "6"]);
}

fn main() -> TestResult {
    futures::executor::block_on(
        Steps::cucumber()
            .max_concurrent_scenarios(4)
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()?),
    );
    Ok(())
}
