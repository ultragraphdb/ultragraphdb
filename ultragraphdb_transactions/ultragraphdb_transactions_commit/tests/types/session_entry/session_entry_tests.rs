/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A session's entry in the keyspace without versions: its key under the system prefix, its bytes byte for byte, its
//! owner among them, and every malformed entry refused as corrupted, naming its key.

use ultragraphdb_error_store::StoreErrorEnum;
use ultragraphdb_protocol_wire::OperationResult;
use ultragraphdb_transactions_commit::SessionEntry;

fn committed() -> SessionEntry {
    SessionEntry::new(
        7,
        0x0102,
        9,
        vec![
            OperationResult::new(0x2A, true),
            OperationResult::new(0x2B, false),
        ],
    )
}

#[test]
fn test_the_keys_sit_under_the_system_prefix_the_session_big_endian() {
    assert_eq!(SessionEntry::INDEX_KEY, b"\xffultragraphdb/sessions");
    let mut key = b"\xffultragraphdb/session/".to_vec();
    key.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0x01, 0x02]);
    assert_eq!(SessionEntry::key(0x0102), key);
    assert!(
        SessionEntry::key(1) < SessionEntry::key(0x100),
        "ordered by number"
    );
}

#[test]
fn test_an_entry_is_its_request_version_owner_count_padding_and_results() {
    let mut expected = Vec::new();
    expected.extend_from_slice(&7u64.to_le_bytes());
    expected.extend_from_slice(&0x0102u64.to_le_bytes());
    expected.extend_from_slice(&9u64.to_le_bytes());
    expected.extend_from_slice(&2u32.to_le_bytes());
    expected.extend_from_slice(&[0; 4]);
    expected.extend_from_slice(&0x2Au64.to_le_bytes());
    expected.extend_from_slice(&[1, 0, 0, 0, 0, 0, 0, 0]);
    expected.extend_from_slice(&0x2Bu64.to_le_bytes());
    expected.extend_from_slice(&[0; 8]);
    let entry = committed();
    assert_eq!(entry.encode(), expected);
    assert_eq!(SessionEntry::decode(b"k", &expected), Ok(entry.clone()));
    assert_eq!(
        (entry.request(), entry.version(), entry.owner()),
        (7, 0x0102, 9)
    );
    assert_eq!(entry.results().len(), 2);
    // A session registered and not yet committed, owned by principal 9.
    let registered = SessionEntry::new(0, 0, 9, Vec::new());
    let mut bytes = vec![0; 32];
    bytes[16] = 9;
    assert_eq!(registered.encode(), bytes);
    assert_eq!(SessionEntry::decode(b"k", &bytes), Ok(registered));
}

/// The reason a malformed `bytes` is refused with, its key named.
fn refusal(bytes: &[u8]) -> String {
    match SessionEntry::decode(b"key", bytes) {
        Err(error) => match error.0 {
            StoreErrorEnum::Corrupted { key, reason } => {
                assert_eq!(key, b"key");
                reason
            }
            other => panic!("not corrupted: {other:?}"),
        },
        Ok(entry) => panic!("accepted: {entry:?}"),
    }
}

#[test]
fn test_every_malformed_entry_is_refused_as_corrupted() {
    let good = committed().encode();
    assert!(refusal(&good[..31]).contains("shorter"));
    assert!(refusal(&good[..good.len() - 1]).contains("results"));
    let mut longer = good.clone();
    longer.extend_from_slice(&[0; 16]);
    assert!(refusal(&longer).contains("results"));
    let mut huge = good.clone();
    huge[24..28].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(refusal(&huge).contains("results"));
    let mut padding = good.clone();
    padding[31] = 1;
    assert!(refusal(&padding).contains("padding"));
    let mut created = good.clone();
    created[40] = 2;
    assert!(refusal(&created).contains("created"));
    let mut result_padding = good.clone();
    result_padding[47] = 1;
    assert!(refusal(&result_padding).contains("padding"));
    // Request 0 is a registration alone: no version and no results.
    let mut version = vec![0; 32];
    version[8] = 2;
    assert!(refusal(&version).contains("request 0"));
    let mut results = vec![0; 32];
    results[24] = 1;
    results.extend_from_slice(&[0; 16]);
    assert!(refusal(&results).contains("request 0"));
    // A committed request has a version.
    let mut unversioned = vec![0; 32];
    unversioned[0] = 1;
    assert!(refusal(&unversioned).contains("version"));
}
