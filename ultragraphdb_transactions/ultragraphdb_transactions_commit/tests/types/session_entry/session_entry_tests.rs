/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A session's entry in the keyspace without versions: its key under the system prefix by the session's 16 bytes, its
//! bytes byte for byte, the stored reply's token, results and node texts among them, and every malformed entry refused
//! as corrupted, naming its key.

use ultragraphdb_core_ids::{Epoch, SessionToken};
use ultragraphdb_error_store::{StoreError, StoreErrorEnum};
use ultragraphdb_protocol_wire::{OperationResult, TermTable};
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_transactions_commit::{SessionEntry, StoredReply};

/// Request 7 committed at version 0x0100_0002 (epoch 0x40, sequence 2), in partition 3 at position 9, with a node
/// result naming `http://e/x`, created, and a counter at -2.
fn committed() -> TestResult<SessionEntry> {
    let mut terms = TermTable::new();
    let node = terms.iri("http://e/x");
    let reply = StoredReply::new(
        0x0100_0002,
        SessionToken::new(3, 9, Epoch::new(0x40).context("the epoch")?),
        vec![
            OperationResult::for_node(node, true),
            OperationResult::counter(-2),
        ],
        terms,
    );
    Ok(SessionEntry::new(7, reply))
}

#[test]
fn test_the_keys_sit_under_the_system_prefix_the_session_big_endian() -> TestResult {
    assert_eq!(SessionEntry::INDEX_KEY, b"\xffultragraphdb/sessions");
    let mut key = b"\xffultragraphdb/session/".to_vec();
    key.extend_from_slice(&[0; 14]);
    key.extend_from_slice(&[0x01, 0x02]);
    assert_eq!(SessionEntry::key(0x0102), key);
    assert!(
        SessionEntry::key(1) < SessionEntry::key(1 << 64),
        "ordered by identity, the high bits first"
    );
    Ok(())
}

/// The entry's bytes, written by hand from the layout `SessionEntry` documents.
fn golden() -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&7u64.to_le_bytes());
    bytes.extend_from_slice(&0x0100_0002u64.to_le_bytes());
    bytes.extend_from_slice(&3u32.to_le_bytes());
    bytes.extend_from_slice(&[0; 4]);
    bytes.extend_from_slice(&9u64.to_le_bytes());
    bytes.extend_from_slice(&0x40u64.to_le_bytes());
    bytes.extend_from_slice(&2u32.to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes());
    // §6.3's result: value i64, node u32, created u8, 3 zero bytes. The node result has value 0, node 0, created 1.
    bytes.extend_from_slice(&0i64.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&[1, 0, 0, 0]);
    // The counter: value -2, node ABSENT (u32::MAX), created 0.
    bytes.extend_from_slice(&(-2i64).to_le_bytes());
    bytes.extend_from_slice(&u32::MAX.to_le_bytes());
    bytes.extend_from_slice(&[0, 0, 0, 0]);
    // The node's text, its length first.
    bytes.extend_from_slice(&10u32.to_le_bytes());
    bytes.extend_from_slice(b"http://e/x");
    bytes
}

#[test]
fn test_an_entry_is_its_request_reply_token_results_and_nodes() -> TestResult {
    let entry = committed()?;
    assert_eq!(entry.encode(), golden());
    let key = SessionEntry::key(5);
    let decoded = SessionEntry::decode(&key, &golden()).context("the entry decodes")?;
    assert_eq!(decoded, entry);
    assert_eq!(decoded.request(), 7);
    assert_eq!(decoded.reply().version(), 0x0100_0002);
    assert_eq!(decoded.reply().token().position(), 9);
    assert_eq!(decoded.reply().results().len(), 2);
    assert_eq!(decoded.reply().terms().len(), 1);
    let reply = decoded.into_reply();
    assert_eq!(reply.token().partition(), 3);
    Ok(())
}

#[test]
fn test_an_entry_without_results_is_its_fixed_part_alone() -> TestResult {
    let reply = StoredReply::new(
        8,
        SessionToken::new(0, 1, Epoch::ZERO),
        Vec::new(),
        TermTable::new(),
    );
    let bytes = SessionEntry::new(1, reply.clone()).encode();
    assert_eq!(bytes.len(), 48);
    let decoded = SessionEntry::decode(b"k", &bytes)?;
    assert_eq!(decoded.reply(), &reply);
    Ok(())
}

#[test]
fn test_the_fuzz_seeds_are_the_suites_entries() -> TestResult {
    let dir = ultragraphdb_tool_bdd::testdata_dir()?.join("fuzz_seeds/session_entry");
    let read = |name: &str| std::fs::read(dir.join(name)).context("a seed is read");
    assert_eq!(read("stage55_golden.bin")?, golden());
    let without_results = StoredReply::new(
        8,
        SessionToken::new(0, 1, Epoch::ZERO),
        Vec::new(),
        TermTable::new(),
    );
    assert_eq!(
        read("stage55_no_results.bin")?,
        SessionEntry::new(1, without_results).encode()
    );
    Ok(())
}

/// The reason `bytes` are refused with, naming the key.
fn refusal(bytes: &[u8]) -> TestResult<String> {
    let key = SessionEntry::key(5);
    match SessionEntry::decode(&key, bytes) {
        Err(StoreError(StoreErrorEnum::Corrupted { key: named, reason })) if named == key => {
            Ok(reason)
        }
        other => Err(ultragraphdb_tool_bdd::TestError::new(format!(
            "not refused as corrupted naming its key: {other:?}"
        ))),
    }
}

/// `golden()` with `bytes` written at `at`.
fn with(at: usize, bytes: &[u8]) -> Vec<u8> {
    let mut good = golden();
    if let Some(slot) = good.get_mut(at..at + bytes.len()) {
        slot.copy_from_slice(bytes);
    }
    good
}

#[test]
fn test_every_malformed_entry_is_refused_as_corrupted() -> TestResult {
    const SHORT: &str = "a session entry shorter than its fixed part";
    const FILL: &str = "a session entry whose results and nodes do not fill it";
    const RESULT_PADDING: &str = "a session entry's result has padding or a created above 1";
    const NODE_AND_VALUE: &str = "a session entry's result names a node and a value";
    const NODE_PAST: &str = "a session entry's result names a node the table does not hold";
    let good = golden();
    assert_eq!(refusal(&good[..47])?, SHORT);
    assert_eq!(refusal(&[])?, SHORT);
    assert_eq!(refusal(&good[..good.len() - 1])?, FILL);
    let mut longer = good.clone();
    longer.push(0);
    assert_eq!(refusal(&longer)?, FILL);
    assert_eq!(refusal(&with(40, &u32::MAX.to_le_bytes()))?, FILL);
    assert_eq!(refusal(&with(44, &9u32.to_le_bytes()))?, FILL);
    // A node's length past the bytes left.
    assert_eq!(refusal(&with(80, &11u32.to_le_bytes()))?, FILL);
    assert_eq!(
        refusal(&with(20, &[1]))?,
        "a session entry's padding is not zero"
    );
    // A result's padding, each of its 3 bytes, and a created above 1.
    for at in [61, 62, 63] {
        assert_eq!(refusal(&with(at, &[1]))?, RESULT_PADDING, "{at}");
    }
    assert_eq!(refusal(&with(60, &[2]))?, RESULT_PADDING);
    // A result naming a node and a value, a node past the table, with a value or without, and a created naming none.
    assert_eq!(refusal(&with(48, &[1]))?, NODE_AND_VALUE);
    assert_eq!(refusal(&with(56, &[1]))?, NODE_PAST);
    assert_eq!(refusal(&with(48, &[1, 0, 0, 0, 0, 0, 0, 0, 1]))?, NODE_PAST);
    assert_eq!(
        refusal(&with(64 + 12, &[1]))?,
        "a session entry's result creates no node"
    );
    // Request 0, a version 0 or odd, an epoch past 42 bits; 2^42 − 1 is the last epoch taken.
    assert_eq!(refusal(&with(0, &[0]))?, "a session entry of request 0");
    let no_version = "a session entry whose version is no commit version";
    assert_eq!(refusal(&with(8, &0u64.to_le_bytes()))?, no_version);
    assert_eq!(refusal(&with(8, &[3]))?, no_version);
    assert_eq!(
        refusal(&with(32, &(1u64 << 42).to_le_bytes()))?,
        "a session entry whose epoch is past 2^42 − 1"
    );
    let last = SessionEntry::decode(
        &SessionEntry::key(5),
        &with(32, &((1u64 << 42) - 1).to_le_bytes()),
    )?;
    assert_eq!(last.reply().token().epoch().get(), (1 << 42) - 1);
    // A node's text not UTF-8.
    assert_eq!(
        refusal(&with(84, &[0xFF]))?,
        "a session entry's node is not UTF-8"
    );
    Ok(())
}

/// A compare-and-set's result and a counter's at either end of i64 read back as they were stored.
#[test]
fn test_a_compare_and_set_and_counters_at_the_ends_of_i64_read_back() -> TestResult {
    let results = vec![
        OperationResult::compare_and_set(),
        OperationResult::counter(i64::MIN),
        OperationResult::counter(i64::MAX),
    ];
    let reply = StoredReply::new(
        u64::MAX - 1,
        SessionToken::new(u32::MAX, u64::MAX, Epoch::ZERO),
        results.clone(),
        TermTable::new(),
    );
    let entry = SessionEntry::new(u64::MAX, reply);
    let bytes = entry.encode();
    assert_eq!(bytes.len(), 48 + 3 * 16);
    let decoded = SessionEntry::decode(b"k", &bytes)?;
    assert_eq!(decoded, entry);
    assert_eq!(decoded.reply().results(), &results[..]);
    assert_eq!(decoded.request(), u64::MAX);
    Ok(())
}
