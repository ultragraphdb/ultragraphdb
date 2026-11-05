/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::utils::fixture::Refused;
use ultragraphdb_core_ids::{Epoch, SessionToken};
use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_protocol_wire::{OperationResult, TermTable};
use ultragraphdb_store_sim::{FaultConfig, Seed, SimClock, SimStore};
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_store::{PersistMode, Store, VersionlessEntry, WriteBatch};
use ultragraphdb_transactions_commit::{CommitError, SessionEntry, Sessions, StoredReply};

/// A session identity using all 128 bits, so a table keyed by 64 of them would merge sessions.
const WIDE: u128 = 0x0123_4567_89AB_CDEF_FEDC_BA98_7654_3210;

/// The stored reply of a commit at `version` in partition 1, at log position `position`, with no results.
fn reply(version: u64, position: u64) -> TestResult<StoredReply> {
    let epoch = Epoch::new(version >> 22).context("the epoch")?;
    Ok(StoredReply::new(
        version,
        SessionToken::new(1, position, epoch),
        Vec::new(),
        TermTable::new(),
    ))
}

/// The stored reply of a commit at `version` with one result naming the node `iri`, created.
fn reply_naming(version: u64, iri: &str) -> TestResult<StoredReply> {
    let epoch = Epoch::new(version >> 22).context("the epoch")?;
    let mut terms = TermTable::new();
    let node = terms.iri(iri);
    Ok(StoredReply::new(
        version,
        SessionToken::new(1, 3, epoch),
        vec![OperationResult::for_node(node, true)],
        terms,
    ))
}

#[test]
fn test_a_table_holds_at_least_one_session_and_accounts_64_bytes_each() -> TestResult {
    let mut table = Sessions::new(0);
    assert_eq!(table.capacity(), 1);
    assert!(table.is_empty());
    table.committed(5, 1, reply(40, 1)?);
    assert_eq!(table.len(), 1);
    assert_eq!(table.bytes(), 64);
    table.committed(6, 1, reply_naming(42, "http://ex.org/x")?);
    // The full table still takes its sessions' commits; 64 and 16 for the result.
    assert_eq!(table.bytes(), 64 + 64 + 16);
    Ok(())
}

#[test]
fn test_a_session_the_table_does_not_hold_commits_there_first() -> TestResult {
    let table = Sessions::new(4);
    assert_eq!(table.check(0, 0), Ok(()), "no session");
    assert_eq!(table.check_one_frame(0, 0), Ok(None), "no session");
    assert_eq!(table.check_one_frame(WIDE, 1), Ok(None));
    assert_eq!(table.check(WIDE, 1), Ok(()));
    assert_eq!(
        table.check(WIDE, 9),
        Ok(()),
        "any number: gaps are accepted"
    );
    assert_eq!(
        table.check_one_frame(WIDE, 9),
        Ok(None),
        "any number: gaps are accepted"
    );
    Ok(())
}

#[test]
fn test_a_one_frame_check_follows_the_lean_models_order() -> TestResult {
    let mut table = Sessions::new(4);
    table.begin(WIDE);
    assert_eq!(
        table.check_one_frame(WIDE, 1),
        Err(CommitError::session_busy(WIDE)),
        "a request decided and not yet applied comes first"
    );
    table.committed(WIDE, 3, reply(40, 2)?);
    assert_eq!(table.last(WIDE), Some((3, 40)));
    // A resend of the last before a number out of order.
    assert_eq!(
        table.check_one_frame(WIDE, 3)?.map(StoredReply::version),
        Some(40),
        "the stored reply"
    );
    assert_eq!(
        table.check_one_frame(WIDE, 2),
        Err(CommitError::request_out_of_order(3))
    );
    assert_eq!(table.check_one_frame(WIDE, 4), Ok(None));
    assert_eq!(
        table.check_one_frame(WIDE, 7),
        Ok(None),
        "a gap left by other partitions"
    );
    Ok(())
}

/// A Commit on an open transaction never reads a stored reply: at or below the last it is refused naming the last, above
/// it taken, and while a request is in flight busy comes first.
#[test]
fn test_a_commit_on_an_open_transaction_refuses_a_reused_number() -> TestResult {
    let mut table = Sessions::new(4);
    table.begin(WIDE);
    assert_eq!(
        table.check(WIDE, 3),
        Err(CommitError::session_busy(WIDE)),
        "a request decided and not yet applied comes first"
    );
    table.committed(WIDE, 3, reply(40, 2)?);
    let reused = CommitError::request_out_of_order(3);
    assert_eq!(table.check(WIDE, 3), Err(reused.clone()), "the last");
    assert_eq!(table.check(WIDE, 2), Err(reused), "below the last");
    assert_eq!(table.check(WIDE, 4), Ok(()), "one above the last");
    assert_eq!(
        table.check(WIDE, 7),
        Ok(()),
        "a gap left by other partitions"
    );
    Ok(())
}

#[test]
fn test_a_session_differing_only_in_its_high_bits_is_another() -> TestResult {
    let mut table = Sessions::new(4);
    table.committed(WIDE, 5, reply(40, 1)?);
    let low_half = WIDE & u128::from(u64::MAX);
    assert_eq!(table.last(low_half), None);
    assert_eq!(table.check(low_half, 1), Ok(()));
    Ok(())
}

#[test]
fn test_a_full_table_refuses_a_session_it_does_not_hold_and_serves_its_own() -> TestResult {
    let mut table = Sessions::new(2);
    table.committed(1, 1, reply(40, 1)?);
    table.committed(2, 1, reply(42, 1)?);
    assert_eq!(table.check(3, 1), Err(CommitError::busy(2)));
    assert_eq!(table.check_one_frame(3, 1), Err(CommitError::busy(2)));
    assert_eq!(table.check(1, 2), Ok(()));
    // Below the capacity, a new session is taken.
    let mut room = Sessions::new(3);
    room.committed(1, 1, reply(40, 1)?);
    room.committed(2, 1, reply(42, 1)?);
    assert_eq!(room.check(3, 1), Ok(()));
    Ok(())
}

#[test]
fn test_a_purge_removes_the_evicted_sessions_held_and_skips_the_others() -> TestResult {
    let mut table = Sessions::new(3);
    table.committed(1, 1, reply(40, 1)?);
    table.committed(2, 4, reply(42, 1)?);
    table.purge(&[2, 99]);
    assert_eq!(table.len(), 1);
    assert_eq!(table.last(2), None);
    assert_eq!(table.last(1), Some((1, 40)));
    // A purged session is unknown again; the registry's owner check keeps an evicted one from reaching here.
    assert_eq!(table.check(2, 1), Ok(()));
    table.purge(&[]);
    assert_eq!(table.len(), 1);
    Ok(())
}

/// A retry sent with no transaction open for it (the frontend's Recall, `PROTOCOL.md` §11.5): the last committed
/// request's stored reply, busy while one is in flight, out of order below the last, not committed otherwise.
#[test]
fn test_a_retry_is_answered_from_the_table() -> TestResult {
    let mut table = Sessions::new(4);
    assert_eq!(table.retry(5, 1), Err(CommitError::not_committed(1)));
    table.begin(5);
    assert_eq!(table.retry(5, 1), Err(CommitError::session_busy(5)));
    let stored = reply_naming(40, "http://ex.org/x")?;
    table.committed(5, 7, stored.clone());
    assert_eq!(
        table.retry(5, 7),
        Ok(&stored),
        "byte for byte, its results and token"
    );
    assert_eq!(table.retry(5, 8), Err(CommitError::not_committed(8)));
    assert_eq!(table.retry(5, 6), Err(CommitError::request_out_of_order(7)));
    assert_eq!(CommitError::not_committed(8).detail(), 8);
    Ok(())
}

#[test]
fn test_a_commit_clears_its_sessions_request_in_flight() -> TestResult {
    let mut table = Sessions::new(4);
    table.begin(5);
    table.begin(6);
    table.committed(5, 1, reply(40, 1)?);
    assert_eq!(table.check(5, 2), Ok(()));
    assert_eq!(table.check(6, 1), Err(CommitError::session_busy(6)));
    Ok(())
}

/// The entries `table` asks the store to write, drained.
fn drained(table: &mut Sessions) -> Vec<VersionlessEntry> {
    let mut entries = Vec::new();
    table.drain_entries(&mut entries);
    entries
}

fn put(session: u128, entry: &SessionEntry) -> VersionlessEntry {
    VersionlessEntry::put(SessionEntry::key(session), entry.encode())
}

/// The index of `sessions`: each identity's 16 big-endian bytes, ascending.
fn index(sessions: &[u128]) -> VersionlessEntry {
    VersionlessEntry::put(
        SessionEntry::INDEX_KEY.to_vec(),
        sessions.iter().flat_map(|s| s.to_be_bytes()).collect(),
    )
}

#[test]
fn test_the_table_hands_out_each_change_once_as_store_entries() -> TestResult {
    let mut table = Sessions::new(4);
    assert!(drained(&mut table).is_empty(), "nothing changed");
    table.committed(12, 1, reply(40, 1)?);
    table.committed(WIDE, 2, reply(42, 1)?);
    assert_eq!(
        drained(&mut table),
        vec![
            put(12, &SessionEntry::new(1, reply(40, 1)?)),
            put(WIDE, &SessionEntry::new(2, reply(42, 1)?)),
            index(&[12, WIDE]),
        ],
        "each session's entry in order, then the index"
    );
    table.committed(12, 5, reply(44, 2)?);
    assert_eq!(
        drained(&mut table),
        vec![put(12, &SessionEntry::new(5, reply(44, 2)?))],
        "a later commit of a session held leaves the index alone"
    );
    table.purge(&[12]);
    assert_eq!(
        drained(&mut table),
        vec![
            VersionlessEntry::removal(SessionEntry::key(12)),
            index(&[WIDE]),
        ]
    );
    // Committed and purged before the entries are drained: removed only, never written.
    table.committed(16, 1, reply(46, 3)?);
    table.purge(&[16]);
    assert_eq!(
        drained(&mut table),
        vec![
            VersionlessEntry::removal(SessionEntry::key(16)),
            index(&[WIDE]),
        ]
    );
    assert!(drained(&mut table).is_empty());
    Ok(())
}

fn store() -> SimStore {
    SimStore::new(Seed::new(1), FaultConfig::none(), SimClock::new())
}

fn write(store: &SimStore, entries: &[VersionlessEntry]) -> TestResult {
    let mut batch = WriteBatch::new();
    for entry in entries {
        match entry.value() {
            Some(value) => batch.put_versionless(entry.key().to_vec(), value.to_vec()),
            None => batch.remove_versionless(entry.key().to_vec()),
        }
    }
    store
        .apply(batch, PersistMode::Buffer)
        .context("the batch applies")?;
    Ok(())
}

#[test]
fn test_a_table_loads_as_its_entries_left_it() -> TestResult {
    let empty = Sessions::load(&store()).context("an empty store loads")?;
    assert!(empty.is_empty());
    assert_eq!(empty.capacity(), Sessions::DEFAULT_CAPACITY);
    let store = store();
    let mut table = Sessions::new(8);
    let named = reply_naming(48, "http://ex.org/x")?;
    table.committed(10, 3, named.clone());
    table.committed(WIDE, 1, reply(50, 2)?);
    write(&store, &drained(&mut table))?;
    let mut loaded = Sessions::load(&store).context("the table loads")?;
    assert_eq!(loaded.len(), 2);
    assert_eq!(loaded.check_one_frame(10, 3), Ok(Some(&named)));
    assert_eq!(loaded.last(WIDE), Some((1, 50)));
    assert!(drained(&mut loaded).is_empty(), "loading changes nothing");
    // Bounded below what it holds: none is evicted, and no new session is taken.
    let bounded = loaded.bounded(1);
    assert_eq!(bounded.len(), 2);
    assert_eq!(bounded.check(77, 1), Err(CommitError::busy(2)));
    Ok(())
}

/// The reason `store` is refused with, and the key it names.
fn refusal(store: &SimStore) -> TestResult<(Vec<u8>, String)> {
    let loaded = Sessions::load(store).map(|t| t.len());
    let refused = if let Err(StoreError(StoreErrorEnum::Corrupted { key, reason })) = &loaded {
        Some((key.clone(), reason.clone()))
    } else {
        None
    };
    refused
        .ok_or_else(|| format!("not refused as corrupted, sessions loaded: {loaded:?}"))
        .context("refused as corrupted")
}

#[test]
fn test_a_malformed_table_is_refused_as_corrupted() -> TestResult {
    let ragged = store();
    write(
        &ragged,
        &[VersionlessEntry::put(
            SessionEntry::INDEX_KEY.to_vec(),
            vec![1; 17],
        )],
    )?;
    assert_eq!(
        refusal(&ragged)?,
        (
            SessionEntry::INDEX_KEY.to_vec(),
            "a session index whose length is not a multiple of 16".to_string()
        )
    );
    let unordered = store();
    write(&unordered, &[index(&[12, 10])])?;
    assert_eq!(
        refusal(&unordered)?,
        (
            SessionEntry::INDEX_KEY.to_vec(),
            "a session index whose sessions are not ascending from above 0".to_string()
        )
    );
    let repeated = store();
    write(
        &repeated,
        &[
            index(&[10, 10]),
            put(10, &SessionEntry::new(1, reply(40, 1)?)),
        ],
    )?;
    assert!(refusal(&repeated)?.1.contains("ascending"));
    let zero = store();
    write(&zero, &[index(&[0])])?;
    assert!(refusal(&zero)?.1.contains("ascending"));
    let missing = store();
    write(&missing, &[index(&[10])])?;
    assert_eq!(
        refusal(&missing)?,
        (
            SessionEntry::key(10),
            "the index names a session with no entry".to_string()
        )
    );
    let bad = store();
    write(
        &bad,
        &[
            index(&[10]),
            VersionlessEntry::put(SessionEntry::key(10), vec![0; 3]),
        ],
    )?;
    assert_eq!(refusal(&bad)?.0, SessionEntry::key(10));
    Ok(())
}

/// A read the store fails while loading is the store's error, passed on as it is, not reported as corruption.
#[test]
fn test_a_failed_read_while_loading_is_the_stores_error() -> TestResult {
    use ultragraphdb_store_sim::Fault;
    let failing = store();
    failing.inject(Fault::FailNextRead);
    assert_eq!(
        Sessions::load(&failing)
            .map(|t| t.len())
            .refused("refused")?
            .0,
        StoreErrorEnum::ReadFailed {
            key: SessionEntry::INDEX_KEY.to_vec(),
            reason: "simulated read failure".to_string()
        }
    );
    Ok(())
}
