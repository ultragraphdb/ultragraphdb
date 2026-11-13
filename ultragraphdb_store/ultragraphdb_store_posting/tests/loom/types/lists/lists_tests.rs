/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::Arc;

use loom::thread;
use ultragraphdb_store_posting::{CachedList, Lists, ListsConfig};
use ultragraphdb_store_sim::{FaultConfig, Seed, SimClock, SimListCache, SimStore, Stream};

type SimLists = Lists<SimStore, SimListCache<CachedList>>;

/// Lists over a simulated store with `faults`, shared as the commit path and the readers share them.
fn lists(faults: FaultConfig) -> Arc<SimLists> {
    let store = SimStore::new(Seed::new(1), faults, SimClock::new());
    let cache = SimListCache::new(16, Seed::new(1).stream(Stream::Cache));
    Arc::new(Lists::new(store, cache, ListsConfig::default()))
}

/// A read above the watermark waits at the gate while a round moves the watermark to it on another thread: whichever
/// comes first, the read goes ahead, so no interleaving loses the round's wakeup.
#[test]
fn test_a_read_waiting_at_the_gate_is_woken_by_its_round() {
    loom::model(|| {
        let lists = lists(FaultConfig::none());
        let reader = Arc::clone(&lists);
        let read = thread::spawn(move || reader.gate(2).is_ok());
        assert!(lists.commit_round_with(&[], &[], 2).is_ok());
        assert!(read.join().is_ok_and(|went_ahead| went_ahead));
        assert_eq!(lists.applied_ts(), 2);
    });
}

/// A read of everything applied, at `u64::MAX`, waits only for a write under way: it goes ahead whether it arrives
/// before the round, during it or after.
#[test]
fn test_a_read_of_everything_waits_only_for_the_write_under_way() {
    loom::model(|| {
        let lists = lists(FaultConfig::none());
        let reader = Arc::clone(&lists);
        let read = thread::spawn(move || reader.gate(u64::MAX).is_ok());
        assert!(lists.commit_round_with(&[], &[], 2).is_ok());
        assert!(read.join().is_ok_and(|went_ahead| went_ahead));
    });
}

/// A round whose batch the store refuses fails every later write and wakes the reads waiting on it: a read above the
/// watermark is refused in every interleaving, never left waiting for a watermark that no longer moves.
#[test]
fn test_a_failed_round_refuses_the_reads_waiting_on_it() {
    loom::model(|| {
        let lists = lists(FaultConfig::none().with_batch_failure(1_000));
        let reader = Arc::clone(&lists);
        let read = thread::spawn(move || reader.gate(2).is_err());
        assert!(lists.commit_round_with(&[], &[], 2).is_err());
        assert!(read.join().is_ok_and(|refused| refused));
        assert_eq!(lists.applied_ts(), 0);
    });
}

/// Two rounds written from two threads serialize through the writer: the watermark ends at the higher version, the
/// lower round is applied or refused as stale and never moves it back, and a read at the higher version goes ahead.
#[test]
fn test_two_rounds_racing_leave_the_watermark_at_the_higher() {
    loom::model(|| {
        let lists = lists(FaultConfig::none());
        let (lower, reader) = (Arc::clone(&lists), Arc::clone(&lists));
        let low = thread::spawn(move || lower.commit_round_with(&[], &[], 2).is_ok());
        let read = thread::spawn(move || reader.gate(4).is_ok());
        assert!(lists.commit_round_with(&[], &[], 4).is_ok());
        assert!(low.join().is_ok());
        assert!(read.join().is_ok_and(|went_ahead| went_ahead));
        assert_eq!(lists.applied_ts(), 4);
    });
}
