/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_store::StoreErrorEnum;
use ultragraphdb_protocol_wire::OperationResult;
use ultragraphdb_store_sim::{FaultConfig, Seed, SimClock, SimStore};
use ultragraphdb_traits_store::{PersistMode, Store, VersionlessEntry, WriteBatch};
use ultragraphdb_transactions_commit::{CommitError, SessionEntry, Sessions};

#[test]
fn test_a_table_holds_at_least_one_session_and_accounts_48_bytes_each() {
    let mut table = Sessions::new(0);
    assert_eq!(table.capacity(), 1);
    assert!(table.is_empty());
    assert_eq!(table.register(10, 0), None);
    assert_eq!(table.bytes(), 48);
    assert_eq!(table.register(12, 0), Some(10));
    assert_eq!((table.len(), table.bytes()), (1, 48));
    assert_eq!(Sessions::new(Sessions::DEFAULT_CAPACITY).capacity(), 4_096);
}

#[test]
fn test_the_session_touched_longest_ago_is_evicted_one_without_a_commit_in_flight_first() {
    let mut table = Sessions::new(3);
    for s in [10, 12, 14] {
        table.register(s, 0);
    }
    // 10 commits at 20, 14 at 22: 12 is the oldest.
    table.committed(10, 1, 20, Vec::new());
    table.committed(14, 1, 22, Vec::new());
    assert_eq!(table.register(30, 0), Some(12));
    // 10 is now the oldest, but has a commit in flight: 14 goes instead.
    table.begin(10);
    assert_eq!(table.register(32, 0), Some(14));
    assert_eq!(table.last(10), Some((1, 20)));
    // Every session in flight: the oldest goes.
    table.begin(30);
    table.begin(32);
    assert_eq!(table.register(34, 0), Some(10));
}

#[test]
fn test_a_check_follows_the_lean_models_order() {
    let mut table = Sessions::new(4);
    assert_eq!(table.check(0, 99, 0), Ok(None), "no session");
    assert_eq!(table.check(5, 1, 0), Err(CommitError::SessionEvicted(5)));
    table.register(5, 0);
    assert_eq!(table.check(5, 0, 0), Err(CommitError::RequestOutOfOrder(1)));
    assert_eq!(table.check(5, 1, 0), Ok(None));
    assert_eq!(table.check(5, 2, 0), Err(CommitError::RequestOutOfOrder(1)));
    table.begin(5);
    assert_eq!(table.check(5, 1, 0), Err(CommitError::SessionBusy(5)));
    let results = vec![OperationResult::new(9, true)];
    table.committed(5, 1, 40, results.clone());
    assert_eq!(
        table.check(5, 1, 0),
        Ok(Some((40, results.as_slice()))),
        "the stored reply, its results with it"
    );
    assert_eq!(table.bytes(), 64, "16 bytes a result");
    assert_eq!(table.check(5, 2, 0), Ok(None));
    assert_eq!(table.check(5, 3, 0), Err(CommitError::RequestOutOfOrder(2)));
    // Unknown sessions take no state.
    table.begin(77);
    table.committed(77, 1, 50, Vec::new());
    assert_eq!(table.last(77), None);
}

/// The entries `table` asks the store to write, drained.
fn drained(table: &mut Sessions) -> Vec<VersionlessEntry> {
    let mut entries = Vec::new();
    table.drain_entries(&mut entries);
    entries
}

fn put(session: u64, entry: SessionEntry) -> VersionlessEntry {
    VersionlessEntry::put(SessionEntry::key(session), entry.encode())
}

fn index(sessions: &[u64]) -> VersionlessEntry {
    VersionlessEntry::put(
        SessionEntry::INDEX_KEY.to_vec(),
        sessions.iter().flat_map(|s| s.to_le_bytes()).collect(),
    )
}

#[test]
fn test_the_table_hands_out_each_change_once_as_store_entries() {
    let mut table = Sessions::new(2);
    assert!(drained(&mut table).is_empty(), "nothing changed");
    table.register(12, 0);
    table.register(10, 0);
    assert_eq!(
        drained(&mut table),
        vec![
            put(10, SessionEntry::new(0, 0, 0, Vec::new())),
            put(12, SessionEntry::new(0, 0, 0, Vec::new())),
            index(&[10, 12]),
        ],
        "each session's entry in order, then the index"
    );
    let results = vec![OperationResult::new(3, false)];
    table.committed(10, 1, 20, results.clone());
    assert_eq!(
        drained(&mut table),
        vec![put(10, SessionEntry::new(1, 20, 0, results.clone()))],
        "a commit leaves the index alone"
    );
    // 12 is the oldest: evicted, its entry removed.
    assert_eq!(table.register(14, 0), Some(12));
    assert_eq!(
        drained(&mut table),
        vec![
            put(14, SessionEntry::new(0, 0, 0, Vec::new())),
            VersionlessEntry::removal(SessionEntry::key(12)),
            index(&[10, 14]),
        ]
    );
    // 16 registered and evicted before the entries are drained: removed only, never written.
    assert_eq!(table.register(16, 0), Some(14));
    assert_eq!(table.register(18, 0), Some(16));
    assert_eq!(
        drained(&mut table),
        vec![
            put(18, SessionEntry::new(0, 0, 0, Vec::new())),
            VersionlessEntry::removal(SessionEntry::key(14)),
            VersionlessEntry::removal(SessionEntry::key(16)),
            index(&[10, 18]),
        ]
    );
    assert!(drained(&mut table).is_empty());
}

fn store() -> SimStore {
    SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new())
}

fn write(store: &SimStore, entries: &[VersionlessEntry]) {
    let mut batch = WriteBatch::new();
    for entry in entries {
        match entry.value() {
            Some(value) => batch.put_versionless(entry.key().to_vec(), value.to_vec()),
            None => batch.remove_versionless(entry.key().to_vec()),
        }
    }
    store
        .apply(batch, PersistMode::Buffer)
        .expect("the batch applies");
}

#[test]
fn test_a_table_loads_as_its_entries_left_it() {
    let empty = Sessions::load(&store()).expect("an empty store loads");
    assert!(empty.is_empty());
    assert_eq!(empty.capacity(), Sessions::DEFAULT_CAPACITY);
    let store = store();
    let mut table = Sessions::new(8);
    for s in [10, 12, 14] {
        table.register(s, 0);
    }
    let results = vec![
        OperationResult::new(5, true),
        OperationResult::new(6, false),
    ];
    table.committed(10, 3, 30, results.clone());
    table.committed(14, 1, 22, Vec::new());
    write(&store, &drained(&mut table));
    let mut loaded = Sessions::load(&store).expect("the table loads");
    assert_eq!(loaded.len(), 3);
    assert_eq!(loaded.check(10, 3, 0), Ok(Some((30, results.as_slice()))));
    assert_eq!(loaded.last(12), Some((0, 0)));
    assert_eq!(loaded.last(14), Some((1, 22)));
    assert!(drained(&mut loaded).is_empty(), "loading changes nothing");
    // Touched as before: 12 at its registration, then 14 at 22, then 10 at 30.
    let mut bounded = loaded.bounded(2);
    assert_eq!(bounded.len(), 2);
    assert_eq!(bounded.last(12), None, "the oldest trimmed");
    assert_eq!(
        drained(&mut bounded),
        vec![
            VersionlessEntry::removal(SessionEntry::key(12)),
            index(&[10, 14])
        ]
    );
    assert_eq!(bounded.register(40, 0), Some(14));
}

/// The reason `store` is refused with, and the key it names.
fn refusal(store: &SimStore) -> (Vec<u8>, String) {
    match Sessions::load(store).map(|t| t.len()) {
        Err(error) => match error.0 {
            StoreErrorEnum::Corrupted { key, reason } => (key, reason),
            other => panic!("not corrupted: {other:?}"),
        },
        Ok(len) => panic!("loaded {len} sessions"),
    }
}

#[test]
fn test_a_malformed_table_is_refused_as_corrupted() {
    let ragged = store();
    write(
        &ragged,
        &[VersionlessEntry::put(
            SessionEntry::INDEX_KEY.to_vec(),
            vec![1; 9],
        )],
    );
    assert_eq!(refusal(&ragged).0, SessionEntry::INDEX_KEY);
    let unordered = store();
    write(&unordered, &[index(&[12, 10])]);
    assert_eq!(
        refusal(&unordered),
        (
            SessionEntry::INDEX_KEY.to_vec(),
            "a session index whose numbers are not ascending from above 0".to_string()
        )
    );
    // A number twice is not ascending either, though each entry exists.
    let repeated = store();
    write(
        &repeated,
        &[
            index(&[10, 10]),
            put(10, SessionEntry::new(0, 0, 0, Vec::new())),
        ],
    );
    assert_eq!(refusal(&repeated).0, SessionEntry::INDEX_KEY);
    assert!(refusal(&repeated).1.contains("ascending"));
    let zero = store();
    write(&zero, &[index(&[0])]);
    assert!(refusal(&zero).1.contains("ascending"));
    let missing = store();
    write(&missing, &[index(&[10])]);
    assert_eq!(
        refusal(&missing),
        (
            SessionEntry::key(10),
            "the index names a session with no entry".to_string()
        )
    );
    let early = store();
    write(
        &early,
        &[
            index(&[10]),
            put(10, SessionEntry::new(1, 10, 0, Vec::new())),
        ],
    );
    assert!(refusal(&early).1.contains("registration"));
    let bad = store();
    write(
        &bad,
        &[
            index(&[10]),
            VersionlessEntry::put(SessionEntry::key(10), vec![0; 3]),
        ],
    );
    assert_eq!(refusal(&bad).0, SessionEntry::key(10));
}

/// A session belongs to the principal that registered it: another's commit, or its retry, is refused before anything
/// else is checked, and the owner's next request is served (`PROTOCOL.md` §11.5).
#[test]
fn test_a_session_serves_its_owner_alone() {
    let mut table = Sessions::new(4);
    table.register(5, 7);
    assert_eq!(table.check(5, 1, 8), Err(CommitError::PermissionDenied(5)));
    assert_eq!(table.retry(5, 1, 8), Err(CommitError::PermissionDenied(5)));
    assert_eq!(table.check(5, 1, 7), Ok(None));
    table.begin(5);
    assert_eq!(
        table.check(5, 1, 8),
        Err(CommitError::PermissionDenied(5)),
        "the owner is checked before the request in flight"
    );
    table.committed(5, 1, 30, Vec::new());
    assert_eq!(
        table.check(5, 1, 8),
        Err(CommitError::PermissionDenied(5)),
        "nor its stored reply"
    );
    assert_eq!(table.check(5, 2, 7), Ok(None));
}

/// A retry sent with no transaction open for it: the last committed request's stored reply, the next one not
/// committed, any other out of order, busy while a request is in flight, evicted when the table holds no session.
#[test]
fn test_a_retry_is_answered_from_the_table() {
    let results = vec![OperationResult::new(3, true)];
    let mut table = Sessions::new(4);
    assert_eq!(table.retry(5, 1, 7), Err(CommitError::SessionEvicted(5)));
    table.register(5, 7);
    assert_eq!(table.retry(5, 1, 7), Err(CommitError::NotCommitted(1)));
    assert_eq!(table.retry(5, 0, 7), Err(CommitError::RequestOutOfOrder(1)));
    table.begin(5);
    assert_eq!(table.retry(5, 1, 7), Err(CommitError::SessionBusy(5)));
    table.committed(5, 1, 30, results.clone());
    assert_eq!(table.retry(5, 1, 7), Ok((30, results.as_slice())));
    assert_eq!(table.retry(5, 2, 7), Err(CommitError::NotCommitted(2)));
    assert_eq!(table.retry(5, 3, 7), Err(CommitError::RequestOutOfOrder(2)));
    assert!(CommitError::NotCommitted(2).is_retryable());
    assert_eq!(CommitError::NotCommitted(2).detail(), 2);
    assert_eq!(
        CommitError::PermissionDenied(5).detail(),
        2,
        "the action, write"
    );
    assert!(!CommitError::PermissionDenied(5).is_retryable());
}

/// The owner is durable: the entries drained carry it, and the table loaded back refuses another principal.
#[test]
fn test_the_owner_survives_a_reload() {
    let store = store();
    let mut table = Sessions::new(4);
    table.register(5, 7);
    let mut entries = Vec::new();
    table.drain_entries(&mut entries);
    write(&store, &entries);
    let loaded = Sessions::load(&store).unwrap();
    assert_eq!(loaded.check(5, 1, 8), Err(CommitError::PermissionDenied(5)));
    assert_eq!(loaded.check(5, 1, 7), Ok(None));
}

/// A read the store fails while loading is the store's error, passed on as it is, not reported as corruption.
#[test]
fn test_a_failed_read_while_loading_is_the_stores_error() {
    use ultragraphdb_store_sim::Fault;
    let failing = store();
    failing.inject(Fault::FailNextRead);
    assert_eq!(
        Sessions::load(&failing).map(|t| t.len()).unwrap_err().0,
        StoreErrorEnum::ReadFailed {
            key: SessionEntry::INDEX_KEY.to_vec(),
            reason: "simulated read failure".to_string()
        }
    );
}
